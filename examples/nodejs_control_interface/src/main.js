const protobuf = require('protobufjs');
const fs = require('fs');
const util = require('util')

const WAITING_TIME_IN_SEC = 5;
const REQUEST_ID = "dynamic_nginx@nodejs_control_interface"

let ToServer;
let FromServer;
let UpdateStrategyEnum;

function write_to_control_interface(root, message) {
    ToServer = root.lookupType("ankaios.ToServer");
    let buffer = ToServer.encodeDelimited(message).finish(); // use length-delimited encoding!!!

    const ci_output_path = '/run/ankaios/control_interface/output';
    fs.writeFile(ci_output_path, buffer, { flag: 'a+' }, err => {
        if (err) {
            console.error(err);
        }
    });
}

function create_request_to_add_new_workload(root) {
    /* Create the Request containing an UpdateStateRequest
    that contains the details for adding the new workload and
    the update mask to add only the new workload. */

    ToServer = root.lookupType("ankaios.ToServer");
    UpdateStrategyEnum = root.lookupEnum("ankaios.UpdateStrategy");
    let payload = {
        request: {
            requestId: REQUEST_ID,
            updateStateRequest: {
                newState: {
                    currentState: {
                        workloads: {
                            dynamic_nginx: {
                                agent: "agent_A",
                                runtime: "podman",
                                restart: true,
                                updateStrategy: UpdateStrategyEnum.AT_MOST_ONCE,
                                runtimeConfig: "image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]"
                            }
                        }
                    },
                },
                updateMask: [
                    "currentState.workloads.dynamic_nginx"
                ]
            }
        }
    };
    const errMsg = ToServer.verify(payload);
    if (errMsg) {
        throw Error(errMsg);
    }

    return ToServer.create(payload);
}

function create_request_for_complete_state(root) {
    /* Create a Request to request the CompleteState
    for querying the workload states. */

    ToServer = root.lookupType("ankaios.ToServer");
    let payload = {
        request: {
            requestId: REQUEST_ID,
            completeStateRequest: {
                fieldMask: ["workloadStates"]
            }
        }
    };
    if (ToServer.verify(payload)) {
        throw Error(errMsg);
    }

    return ToServer.create(payload);
}

function decode_from_server_response_message(root, data) {
    FromServer = root.lookupType("ankaios.FromServer");
    const decoded_message = FromServer.decodeDelimited(data);
    console.log(`[${new Date().toISOString()}] Receiving Response containing the workload states of the current state:\n FromServer `, util.inspect(decoded_message.toJSON(), { depth: null }));
}

function read_from_control_interface(root, decode_func) {
    const ci_input_path = '/run/ankaios/control_interface/input';
    const fifo = fs.createReadStream(ci_input_path);
    fifo.on('data', data => {
        try {
            decode_func(root, data)
        } catch (e) {
            if (e instanceof protobuf.util.ProtocolError) {
                console.error(e);
            } else {
                // wire format is invalid
                console.error(`invalid wire format: `, e);
            }
        }
    });
}

async function main() {
    protobuf.load("/usr/local/lib/ankaios/ankaios.proto", async function (err, root) {
        if (err) throw err;

        // Send request to add the new workload dynamic_nginx to Ankaios Server
        const message = create_request_to_add_new_workload(root);
        console.log(`[${new Date().toISOString()}] Sending Request containing details for adding the dynamic workload "dynamic_nginx":\n ToServer `, util.inspect(message.toJSON(), { depth: null }));
        write_to_control_interface(root, message);

        read_from_control_interface(root, decode_from_server_response_message);

        const send_request_for_complete_state = async () => {
            // Send the request to request the complete state containing the workload states to Ankaios Server
            const message = create_request_for_complete_state(root);
            console.log(`[${new Date().toISOString()}] Sending Request containing details for requesting all workload states:\n ToServer `, util.inspect(message.toJSON(), { depth: null }));
            write_to_control_interface(root, message);
        }

        setInterval(send_request_for_complete_state, WAITING_TIME_IN_SEC * 1000); // send the request for the complete state every x secs according to WAITING_TIME.
    });
}

main();



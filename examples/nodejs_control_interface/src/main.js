const protobuf = require('protobufjs');
const fs = require('fs');
const util = require('util')

const WAITING_TIME_IN_SEC = 5;
const REQUEST_ID = "dynamic_nginx@nodejs_control_interface"

let ToAnkaios;
let FromAnkaios;

function create_request_to_add_new_workload(root) {
    /* Create the Request containing an UpdateStateRequest
    that contains the details for adding the new workload and
    the update mask to add only the new workload. */

    ToAnkaios = root.lookupType("control_api.ToAnkaios");
    RestartEnum = root.lookupEnum("ank_base.RestartPolicy")
    let payload = {
        request: {
            requestId: REQUEST_ID,
            updateStateRequest: {
                newState: {
                    desiredState: {
                        apiVersion: "v0.1",
                        workloads: {
                                workloads: {
                                dynamic_nginx: {
                                    agent: "agent_A",
                                    runtime: "podman",
                                    restartPolicy: RestartEnum.NEVER,
                                    runtimeConfig: "image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]"
                                }
                            }
                        }
                    },
                },
                updateMask: [
                    "desiredState.workloads.dynamic_nginx"
                ]
            }
        }
    };
    const errMsg = ToAnkaios.verify(payload);
    if (errMsg) {
        throw Error(errMsg);
    }

    return ToAnkaios.create(payload);
}

function create_request_for_complete_state(root) {
    /* Create a Request to request the CompleteState
    for querying the workload states. */

    ToAnkaios = root.lookupType("control_api.ToAnkaios");
    let payload = {
        request: {
            requestId: REQUEST_ID,
            completeStateRequest: {
                fieldMask: ["workloadStates.agent_A.dynamic_nginx"]
            }
        }
    };
    if (ToAnkaios.verify(payload)) {
        throw Error(errMsg);
    }

    return ToAnkaios.create(payload);
}

function decode_from_server_response_message(root, data) {
    FromAnkaios = root.lookupType("control_api.FromAnkaios");
    const decoded_message = FromAnkaios.decodeDelimited(data);
    let requestId = decoded_message.response.requestId;
    if (requestId === REQUEST_ID) {
        console.log(`[${new Date().toISOString()}] Receiving Response containing the workload states of the current state:\nFromAnkaios `, util.inspect(decoded_message.toJSON(), { depth: null }));
    } else {
        console.log(`RequestId does not match. Skipping messages from requestId: ${requestId}`);
    }
}

function read_from_control_interface(root, decode_func) {
    // Reads from the control interface input fifo and prints the workload states.
    const ci_input_path = '/run/ankaios/control_interface/input';
    const fifo = fs.createReadStream(ci_input_path);
    fifo.on('data', data => {
        try {
            decode_func(root, data)
        } catch (e) {
            console.error(`Invalid response, parsing error: `, e.toString());
        }
    });
}

function write_to_control_interface(root, message) {
    /* Writes a Request into the control interface output fifo
    to add the new workload dynamically and every x sec according to WAITING_TIME_IN_SEC
    another Request to request the workload states. */

    ToAnkaios = root.lookupType("control_api.ToAnkaios");
    let buffer = ToAnkaios.encodeDelimited(message).finish(); // use length-delimited encoding!!!

    const ci_output_path = '/run/ankaios/control_interface/output';
    fs.writeFile(ci_output_path, buffer, { flag: 'a+' }, err => {
        if (err) {
            console.error(err);
        }
    });
}

async function main() {
    protobuf.load("/usr/local/lib/ankaios/control_api.proto", async function (err, root) {
        if (err) throw err;

        read_from_control_interface(root, decode_from_server_response_message);

        // Send request to add the new workload dynamic_nginx to Ankaios Server
        const message = create_request_to_add_new_workload(root);
        console.log(`[${new Date().toISOString()}] Sending Request containing details for adding the dynamic workload "dynamic_nginx":\nToAnkaios `, util.inspect(message.toJSON(), { depth: null }));
        write_to_control_interface(root, message);

        const send_request_for_complete_state = async () => {
            // Send the request to request the complete state containing the workload states to Ankaios Server
            const message = create_request_for_complete_state(root);
            console.log(`[${new Date().toISOString()}] Sending Request containing details for requesting all workload states:\nToAnkaios `, util.inspect(message.toJSON(), { depth: null }));
            write_to_control_interface(root, message);
        }

        setInterval(send_request_for_complete_state, WAITING_TIME_IN_SEC * 1000); // send the request for the complete state every x secs according to WAITING_TIME.
    });
}

main();

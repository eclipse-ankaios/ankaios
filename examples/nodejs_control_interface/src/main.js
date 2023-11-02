const protobuf = require('protobufjs');
const fs = require('fs');
const util = require('util')

let StateChangeRequestMessage;
let ExecutionRequestMessage;
let UpdateStrategyEnum;

async function main() {
    // For more details see https://eclipse-ankaios.github.io/ankaios/main/reference/_ankaios.proto/
    protobuf.load("/usr/local/lib/ankaios/ankaios.proto", async function (err, root) {
        if (err) throw err;

        StateChangeRequestMessage = root.lookupType("ankaios.StateChangeRequest");
        ExecutionRequestMessage = root.lookupType("ankaios.ExecutionRequest");
        UpdateStrategyEnum = root.lookupEnum("ankaios.UpdateStrategy");

        // Build StateChangeRequest.UpdateState request to add a new workload dynamic_nginx
        let payload = {
            updateState: {
                newState: {
                    currentState: {
                        workloads: {
                            dynamic_nginx: {
                                agent: "agent_A",
                                runtime: "podman",
                                restart: true,
                                updateStrategy: UpdateStrategyEnum.AT_MOST_ONCE,
                                runtimeConfig: "image: docker.io/library/nginx\nports:\n- containerPort: 80\n  hostPort: 8081"
                            }
                        }
                    },
                },
                updateMask: [
                    "currentState.workloads.dynamic_nginx"
                ]
            }
        };
        const errMsg = StateChangeRequestMessage.verify(payload);
        if (errMsg) {
            throw Error(errMsg);
        }

        const message = StateChangeRequestMessage.create(payload);
        let buffer = StateChangeRequestMessage.encodeDelimited(message).finish(); // use length-delimited encoding!!!

        // Send StateChangeRequest.UpdateState to Ankaios Server
        console.log(`[${new Date().toISOString()}] Sending StateChangeRequest containing details for adding the dynamic workload "dynamic_nginx":\n StateChangeRequest `, util.inspect(message.toJSON(), { depth: null }));
        const ci_output_path = '/run/ankaios/control_interface/output';
        fs.writeFile(ci_output_path, buffer, { flag: 'a+' }, err => {
            if (err) {
                console.error(err);
            }
        });

        const ci_input_path = '/run/ankaios/control_interface/input';
        const fifo = fs.createReadStream(ci_input_path);
        fifo.on('data', data => {
            // console.log('read FIFO: ', data);
            try {
                const decoded_message = ExecutionRequestMessage.decodeDelimited(data);
                console.log(`[${new Date().toISOString()}] Receiving ExecutionRequest containing the workload states of the current state:\n ExecutionRequest `, util.inspect(decoded_message.toJSON(), { depth: null }));
            } catch (e) {
                if (e instanceof protobuf.util.ProtocolError) {
                    console.error(e);
                } else {
                    // wire format is invalid
                    console.error(`invalid wire format: `, e);
                }
            }
        });

        const send_request_complete_state = async () => {
            // Build StateChangeRequest.RequestCompletestate request to request the workload states
            payload = {
                requestCompleteState: {
                    requestId: "request_id",
                    fieldMask: ["workloadStates"]
                }
            };
            if (StateChangeRequestMessage.verify(payload)) {
                throw Error(errMsg);
            }

            const message = StateChangeRequestMessage.create(payload);
            buffer = StateChangeRequestMessage.encodeDelimited(message).finish(); // use length-delimited encoding!!!

            // Send StateChangeRequest.RequestCompletestate to Ankaios Server
            console.log(`[${new Date().toISOString()}] Sending StateChangeRequest containing details for requesting all workload states:\n StateChangeRequest `, util.inspect(message.toJSON(), { depth: null }));
            fs.writeFile(ci_output_path, buffer, { flag: 'a+' }, err => {
                if (err) {
                    console.error(err);
                }
            });
        }

        setInterval(send_request_complete_state, 30000); // StateChangeRequest.RequestCompletestate every 10 secs.
    });
}

main();



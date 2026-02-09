// Copyright (c) 2023 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

const protobuf = require('protobufjs');
const fs = require('fs');
const util = require('util');
const assert = require('assert');


const ANKAIOS_CONTROL_INTERFACE_BASE_PATH = '/run/ankaios/control_interface';
const WAITING_TIME_IN_SEC = 5;
const TIMEOUT_IN_SEC = 1;
const UPDATE_STATE_REQUEST_ID = "RWNsaXBzZSBBbmthaW9z";
const COMPLETE_STATE_REQUEST_ID = "QW5rYWlvcyBpcyB0aGUgYmVzdA==";
const PROTOCOL_VERSION = process.env.ANKAIOS_VERSION;

// to_ankaios and from_ankaios are created here
// to avoid redefining them in every function.
let to_ankaios;
let from_ankaios;


function logInfo(message) {
    console.log(`[${new Date().toISOString()}] ${message}`);
}

function logError(message) {
    console.error(`[${new Date().toISOString()}] ${message}`);
}


function createHelloMessage(api_proto) {
    /* Create hello message for connection */
    to_ankaios = api_proto.lookupType("control_api.ToAnkaios");

    let payload = {
        hello: {
            protocolVersion: PROTOCOL_VERSION,
        }
    }

    const errMsg = to_ankaios.verify(payload);
    if (errMsg) {
        throw Error(errMsg);
    }

    return to_ankaios.create(payload);
}


function createRequestToAddNewWorkload(api_proto) {
    /* Return request for adding a new workload. */
    to_ankaios = api_proto.lookupType("control_api.ToAnkaios");
    let restart_enum = api_proto.lookupEnum("ank_base.RestartPolicy");
    let payload = {
        request: {
            requestId: UPDATE_STATE_REQUEST_ID,
            updateStateRequest: {
                newState: {
                    desiredState: {
                        apiVersion: "v0.1",
                        workloads: {
                                workloads: {
                                dynamic_nginx: {
                                    agent: "agent_A",
                                    runtime: "podman",
                                    restartPolicy: restart_enum.NEVER,
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
    const errMsg = to_ankaios.verify(payload);
    if (errMsg) {
        throw Error(errMsg);
    }

    return to_ankaios.create(payload);
}


function createRequestForWorkloadState(api_proto) {
    /* Return request for getting the complete state */
    to_ankaios = api_proto.lookupType("control_api.ToAnkaios");
    let payload = {
        request: {
            requestId: COMPLETE_STATE_REQUEST_ID,
            completeStateRequest: {
                fieldMask: ["workloadStates.agent_A.dynamic_nginx"]
            }
        }
    };
    if (to_ankaios.verify(payload)) {
        throw Error(errMsg);
    }

    return to_ankaios.create(payload);
}


function decodeProtobufData(api_proto, data) {
    /* Decode the protobuf message received from Ankaios. */
    from_ankaios = api_proto.lookupType("control_api.FromAnkaios");
    const decoded_message = from_ankaios.decodeDelimited(data);
    return decoded_message;
}


function readFromControlInterface(input_fifo_path, api_proto, { timeout_sec = 1 } = {}) {
    /* Read a message from Ankaios, with a timeout. */
    const pipe_handle = fs.createReadStream(input_fifo_path);

    return new Promise((resolve, reject) => {
        const cleanup = () => {
            clearTimeout(timer);
            pipe_handle.removeAllListeners();
            pipe_handle.destroy();
        };

        const timer = setTimeout(() => {
            cleanup();
            reject(new Error(`Error while reading from control interface: timeout while waiting for data.`));
        }, timeout_sec * 1000);

        pipe_handle.once('data', (data) => {
        try {
            const decoded = decodeProtobufData(api_proto, data);
            cleanup();
            resolve(decoded);
        } catch (e) {
            cleanup();
            reject(new Error(`Error while reading from control interface: ` + e.toString()));
        }
        });

        pipe_handle.once('error', (err) => {
            cleanup();
            reject(new Error(`Error while reading from control interface: ` + err.toString()));
        });

        pipe_handle.once('end', () => {
            cleanup();
            reject(new Error('Error while reading from control interface: stream ended unexpectedly.'));
        });
    });
}


function writeToControlInterface(api_proto, fifo_path, message) {
    /* Encode and write message to Ankaios
     * Use length-delimited encoding.
     */
    to_ankaios = api_proto.lookupType("control_api.ToAnkaios");
    let buffer = to_ankaios.encodeDelimited(message).finish();

    fs.writeFile(fifo_path, buffer, { flag: 'a+' }, err => {
        if (err) {
            logError(err);
        }
    });
}


async function main() {
    // Check that the control interface fifo files exist
    const input_fifo_path = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + '/input';
    const output_fifo_path = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + '/output';
    if (!fs.existsSync(input_fifo_path) || !fs.existsSync(output_fifo_path)) {
        logError(`Error: Control interface FIFO files do not exist. Exiting..`);
        process.exit(1);
    }

    // Response variable to hold the decoded protobuf message
    let response;

    // Load the protobuf definition
    protobuf.load("/usr/local/lib/ankaios/control_api.proto", async function (err, api_proto) {
        if (err) throw err;

        logInfo(`Sending initial Hello message to establish connection...`);
        const hello_message = createHelloMessage(api_proto);
        writeToControlInterface(api_proto, output_fifo_path, hello_message);
        response = await readFromControlInterface(input_fifo_path, api_proto, { timeout_sec: TIMEOUT_IN_SEC });
        assert(response.controlInterfaceAccepted, 'Response should have been ControlInterfaceAccepted');
        if (!response.controlInterfaceAccepted) {
            logError(`Connection to Ankaios not established.`);
            process.exit(1);
        }
        logInfo('Receiving answer to the initial Hello:\n' + JSON.stringify(response.toJSON(), null, 2));

        logInfo(`Requesting to add the dynamic_nginx workload...`);
        const update_workload_request = createRequestToAddNewWorkload(api_proto);
        writeToControlInterface(api_proto, output_fifo_path, update_workload_request);
        response = await readFromControlInterface(input_fifo_path, api_proto, { timeout_sec: TIMEOUT_IN_SEC });
        assert(response.response, 'Response should contain a response field');
        assert(response.response.UpdateStateSuccess, 'Response should be of type UpdateStateSuccess');
        assert(response.response.requestId === UPDATE_STATE_REQUEST_ID, `Response requestId should be ${UPDATE_STATE_REQUEST_ID}`);
        logInfo('Receiving response for the UpdateStateRequest:\n' + JSON.stringify(response.toJSON(), null, 2));

        const sendRequestForCompleteState = async () => {
            logInfo(`Requesting workload state of the dynamic_nginx workload...`);
            const complete_state_request = createRequestForWorkloadState(api_proto);
            writeToControlInterface(api_proto, output_fifo_path, complete_state_request);
            response = await readFromControlInterface(input_fifo_path, api_proto, { timeout_sec: TIMEOUT_IN_SEC });
            assert(response.response, 'Response should contain a response field');
            assert(response.response.completeStateResponse, 'Response should be of type CompleteStateResponse');
            assert(response.response.requestId === COMPLETE_STATE_REQUEST_ID, `Response requestId should be ${COMPLETE_STATE_REQUEST_ID}`);
            logInfo(`Receiving response for the CompleteStateRequest with filter 'workloadStates.agent_A.dynamic_nginx':\n` + JSON.stringify(response.toJSON(), null, 2));
        }

        setInterval(sendRequestForCompleteState, WAITING_TIME_IN_SEC * 1000);
    });
}


main();

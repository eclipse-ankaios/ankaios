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

// ======================================================================
// Imports
// ======================================================================
// - protobufjs for handling protobuf messages
const protobuf = require('protobufjs');
const fs = require('fs');
const util = require('util');

// =======================================================================
// Variables - general
// =======================================================================
// - ANKAIOS_CONTROL_INTERFACE_BASE_PATH is the path to the input and output
//     FIFO files of the control interface.
// - WAITING_TIME_IN_SEC represents the default waiting time.
// - UPDATE_STATE_REQUEST_ID is the request ID for the update state request.
// - COMPLETE_STATE_REQUEST_ID is the request ID for the complete state request.
// - PROTOCOL_VERSION is the version of Ankaios, which for this example
//     is set to the value of the environment variable 'ANKAIOS_VERSION'.
// - CONNECTED is a flag to check the connection is established.
// - CONNECTION_CLOSED is a flag to check if Ankaios closed the connection.
const ANKAIOS_CONTROL_INTERFACE_BASE_PATH = '/run/ankaios/control_interface';
const WAITING_TIME_IN_SEC = 5;
const UPDATE_STATE_REQUEST_ID = "dynamic_nginx@12345";
const COMPLETE_STATE_REQUEST_ID = "dynamic_nginx@67890";
const PROTOCOL_VERSION = process.env.ANKAIOS_VERSION;
let CONNECTED = false;
let CONNECTION_CLOSED = false;

// to_ankaios and from_ankaios are created here
// to avoid redefining them in every function.
let to_ankaios;
let from_ankaios;

// =======================================================================
// Setup logger
// =======================================================================
function logInfo(message) {
    console.log(`[${new Date().toISOString()}] ${message}`);
}

function logError(message) {
    console.error(`[${new Date().toISOString()}] ${message}`);
}

function logWarn(message) {
    console.warn(`[${new Date().toISOString()}] ${message}`);
}

// =======================================================================
// Functions for creating protobuf messages
// =======================================================================
// - createHelloMessage returns the initial required message to establish
//     a connection with Ankaios.
// - createRequestToAddNewWorkload returns the message used to update
//     the state of the cluster. In this example, it is used to add a new
//     workload dynamically. It contains the details for adding the new
//     workload and the update mask to add only the new workload.
// - createRequestForCompleteState returns a request for querying the
//     state of the dynamic_nginx workload.
function createHelloMessage(api_proto) {
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

function createRequestForCompleteState(api_proto) {
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

// =======================================================================
// Ankaios control interface methods
// =======================================================================
// - decodeProtobufData reads the protobuf message from the control interface
//     input fifo.
// - readFromControlInterface continuously reads from the control interface
//     input fifo and sends the response to be handled.
// - handleResponse processes the response from Ankaios. It checks the type
//     of the response and handles it accordingly.
// - writeToControlInterface writes a ToAnkaios message to the control
//     interface output fifo.
function decodeProtobufData(api_proto, data) {
    from_ankaios = api_proto.lookupType("control_api.FromAnkaios");
    const decoded_message = from_ankaios.decodeDelimited(data);
    return decoded_message;
}

function readFromControlInterface(input_fifo_path, api_proto) {
    const pipe_handle = fs.createReadStream(input_fifo_path);
    pipe_handle.on('data', data => {
        try {
            const decoded_protobuf = decodeProtobufData(api_proto, data);
            handleResponse(decoded_protobuf);
        } catch (e) {
            logError(`Error while reading from control interface: ` + e.toString());
        }
    });
}

function handleResponse(decoded_message) {
    // Check if the connection has been established or not
    if (!CONNECTED) {
        if (decoded_message.controlInterfaceAccepted) {
            CONNECTED = true;
            logInfo(`Received Control interface accepted response.`);
        }
        else if (decoded_message.connectionClosed) {
            logError(`Received Connection Closed response. Exiting..`);
            CONNECTION_CLOSED = true;
        }
        else {
            logWarn(`Received unexpected response before connection established. Skipping.`);
        }
        return;
    }
    // If the connection is established, handle the response accordingly
    else {
        if (decoded_message.response) {
            let request_id = decoded_message.response.requestId;
            if (request_id === UPDATE_STATE_REQUEST_ID) {
                // Get the list out of the repeated String fields
                let added_workloads = decoded_message.response.UpdateStateSuccess.addedWorkloads.join(', ');
                let deleted_workloads = decoded_message.response.UpdateStateSuccess.deletedWorkloads.join(', ');
                logInfo('Receiving Response for the UpdateStateRequest:\nadded workloads: ' + added_workloads + '\ndeleted workloads: ' + deleted_workloads);
            } else if (request_id === COMPLETE_STATE_REQUEST_ID) {
                logInfo(`Receiving Response for the CompleteStateRequest:\n` + util.inspect(decoded_message.toJSON(), { depth: null }));
            } else {
                logInfo(`RequestId does not match. Skipping messages from requestId: ${request_id}`);
            }
        } else if (decoded_message.connectionClosed) {
            logInfo(`Received Connection Closed response. Exiting..`);
            CONNECTION_CLOSED = true;
            CONNECTED = false;
        } else {
            logWarn(`Received unknown message type. Skipping message.`);
        }
    }
}

function writeToControlInterface(api_proto, fifo_path, message) {
    to_ankaios = api_proto.lookupType("control_api.ToAnkaios");
    // use length-delimited encoding!!!
    let buffer = to_ankaios.encodeDelimited(message).finish();

    fs.writeFile(fifo_path, buffer, { flag: 'a+' }, err => {
        if (err) {
            logError(err);
        }
    });
}

// =======================================================================
// Main
// =======================================================================
async function main() {
    // Check that the control interface fifo files exist
    const input_fifo_path = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + '/input';
    const output_fifo_path = ANKAIOS_CONTROL_INTERFACE_BASE_PATH + '/output';
    if (!fs.existsSync(input_fifo_path) || !fs.existsSync(output_fifo_path)) {
        logError(`Error: Control interface FIFO files do not exist. Exiting..`);
        process.exit(1);
    }

    // Load the protobuf definition
    protobuf.load("/usr/local/lib/ankaios/control_api.proto", async function (err, api_proto) {
        if (err) throw err;

        // Start reading from the control interface input fifo
        readFromControlInterface(input_fifo_path, api_proto);

        // Send the initial Hello message to initialize the session
        const hello_message = createHelloMessage(api_proto);
        logInfo(`Sending initial Hello message to establish connection...`);
        writeToControlInterface(api_proto, output_fifo_path, hello_message);
        await new Promise(resolve => setTimeout(resolve, 1000)); // Give some time for the connection to be established
        if (!CONNECTED) {
            logError(`Connection to Ankaios not established.`);
            process.exit(1);
        }

        // Send request to add the new workload dynamic_nginx to Ankaios Server
        const update_workload_request = createRequestToAddNewWorkload(api_proto);
        logInfo(`Requesting to add the dynamic_nginx workload...`);
        writeToControlInterface(api_proto, output_fifo_path, update_workload_request);
        await new Promise(resolve => setTimeout(resolve, 1000)); // Give some time for the request to be processed

        const sendRequestForCompleteState = async () => {
            // Send the request for the complete state
            const complete_state_request = createRequestForCompleteState(api_proto);
            logInfo(`Requesting complete state of the dynamic_nginx workload...`);
            writeToControlInterface(api_proto, output_fifo_path, complete_state_request);
        }

        // setInterval(sendRequestForCompleteState, WAITING_TIME_IN_SEC * 1000);
        sendRequestForCompleteState();
    });
}

main();

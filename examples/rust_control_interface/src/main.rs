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
// - api::{ank_base, control_api}: Ankaios protocol definitions
// - prost::Message: Used for encoding and decoding protobuf messages.
use api::ank_base::{
    request::RequestContent, response::ResponseContent, CompleteState, CompleteStateRequest,
    Dependencies, Request, RestartPolicy, State, Tag, Tags, UpdateStateRequest, Workload,
    WorkloadMap,
};
use api::control_api::{
    from_ankaios::FromAnkaiosEnum, to_ankaios::ToAnkaiosEnum, FromAnkaios, Hello, ToAnkaios,
};

use prost::Message;
use std::{
    collections::HashMap,
    fs::File,
    io,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::exit,
    time::Duration,
};

// =======================================================================
// Variables - general
// =======================================================================
// - ANKAIOS_CONTROL_INTERFACE_BASE_PATH is the path to the input and output
//     FIFO files of the control interface.
// - MAX_VARINT_SIZE is the maximum size of a varint encoded data.
// - WAITING_TIME_IN_SEC represents the default waiting time.
// - UPDATE_STATE_REQUEST_ID is the request ID for the update state request.
// - COMPLETE_STATE_REQUEST_ID is the request ID for the complete state request.
// - PROTOCOL_VERSION is the version of Ankaios, which for this example
//     is set to the value of the environment variable 'ANKAIOS_VERSION'.
const ANKAIOS_CONTROL_INTERFACE_BASE_PATH: &str = "/run/ankaios/control_interface";
const MAX_VARINT_SIZE: usize = 19;
const WAITING_TIME_IN_SEC: u64 = 5;
const UPDATE_STATE_REQUEST_ID: &str = "dynamic_nginx@12345";
const COMPLETE_STATE_REQUEST_ID: &str = "dynamic_nginx@67890";
const PROTOCOL_VERSION: &str = env!("ANKAIOS_VERSION");

// =======================================================================
// Setup logger
// =======================================================================
mod logging {
    pub fn log(msg: &str) {
        println!(
            "[{}] {}",
            chrono::offset::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            msg
        );
    }
}

// =======================================================================
// Functions for creating protobuf messages
// =======================================================================
// - create_hello_message returns the initial required message to establish
//     a connection with Ankaios.
// - create_request_to_add_new_workload returns the message used to update
//     the state of the cluster. In this example, it is used to add a new
//     workload dynamically. It contains the details for adding the new
//     workload and the update mask to add only the new workload.
// - create_request_for_complete_state returns a request for querying the
//     state of the dynamic_nginx workload.
fn create_hello_message() -> ToAnkaios {
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Hello(Hello {
            protocol_version: PROTOCOL_VERSION.to_string(),
        })),
    }
}

fn create_request_to_add_new_workload() -> ToAnkaios {
    let new_workloads = Some(WorkloadMap {
        workloads: HashMap::from([(
            "dynamic_nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                restart_policy: Some(RestartPolicy::Never.into()),
                tags: Some(Tags {
                    tags: vec![Tag {
                        key: "owner".to_string(),
                        value: "Ankaios team".to_string(),
                    }],
                }),
                runtime_config: Some(
                    "image: docker.io/library/nginx\ncommandOptions: [\"-p\", \"8080:80\"]"
                        .to_string(),
                ),
                dependencies: Some(Dependencies {
                    dependencies: HashMap::new(),
                }),
                ..Default::default()
            },
        )]),
    });

    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: UPDATE_STATE_REQUEST_ID.to_string(),
            request_content: Some(RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    new_state: Some(CompleteState {
                        desired_state: Some(State {
                            api_version: "v0.1".into(),
                            workloads: new_workloads,
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    update_mask: vec!["desiredState.workloads.dynamic_nginx".to_string()],
                },
            ))),
        })),
    }
}

fn create_request_for_complete_state() -> ToAnkaios {
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: COMPLETE_STATE_REQUEST_ID.to_string(),
            request_content: Some(RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![String::from("workloadStates.agent_A.dynamic_nginx")],
            })),
        })),
    }
}

// =======================================================================
// Ankaios control interface methods
// =======================================================================
// - read_varint_data and read_protobuf_data are used to read the protobuf
//     messages from the control interface input fifo.
// - read_from_control_interface continuously reads from the control interface
//     input fifo and sends the response to be handled.
// - handle_response processes the response from Ankaios. It checks the type
//     of the response and handles it accordingly.
// - write_to_control_interface writes a ToAnkaios message to the control
//     interface output fifo.
fn read_varint_data(file: &mut File) -> Result<[u8; MAX_VARINT_SIZE], io::Error> {
    let mut res = [0u8; MAX_VARINT_SIZE];
    let mut one_byte_buffer = [0u8; 1];
    for item in res.iter_mut() {
        file.read_exact(&mut one_byte_buffer)?;
        *item = one_byte_buffer[0];
        // check if most significant bit is set to 0 if so it is the last byte to be read
        if *item & 0b10000000 == 0 {
            break;
        }
    }
    Ok(res)
}

fn read_protobuf_data(file: &mut File) -> Result<Box<[u8]>, io::Error> {
    let varint_data = read_varint_data(file)?;
    let mut varint_data = Box::new(&varint_data[..]);

    // determine the exact size for exact reading of the bytes later by decoding the varint data
    let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

    let mut buf = vec![0; size];
    file.read_exact(&mut buf[..])?; // read exact bytes from file
    Ok(buf.into_boxed_slice())
}

/// Reads from the control interface input fifo and prints the workload states.
fn read_from_control_interface(
    input_pipe: PathBuf,
    connected: std::sync::Arc<std::sync::Mutex<bool>>,
    connection_closed: std::sync::Arc<std::sync::Mutex<bool>>,
) {
    let mut pipe_handle = File::open(&input_pipe).unwrap_or_else(|err| {
        logging::log(&format!(
            "Error: cannot open '{}': '{}'",
            input_pipe.to_str().unwrap(),
            err
        ));
        exit(1);
    });

    while !*connection_closed.lock().unwrap() {
        if let Ok(binary) = read_protobuf_data(&mut pipe_handle) {
            match FromAnkaios::decode(&mut Box::new(binary.as_ref())) {
                Ok(from_ankaios) => {
                    let connected_clone = connected.clone();
                    let connection_closed_clone = connection_closed.clone();
                    handle_response(from_ankaios, connected_clone, connection_closed_clone);
                }
                Err(err) => logging::log(&format!("Invalid response, parsing error: '{}'", err)),
            }
        }
    }
}

fn handle_response(
    from_ankaios: FromAnkaios,
    connected: std::sync::Arc<std::sync::Mutex<bool>>,
    connection_closed: std::sync::Arc<std::sync::Mutex<bool>>,
) {
    // Check if the connection has been established or not
    if !*connected.lock().unwrap() {
        match &from_ankaios.from_ankaios_enum {
            Some(FromAnkaiosEnum::ControlInterfaceAccepted(_)) => {
                logging::log("Received Control interface accepted response.");
                *connected.lock().unwrap() = true;
            }
            Some(FromAnkaiosEnum::ConnectionClosed(_)) => {
                logging::log("Received Connection Closed response. Exiting..");
                *connection_closed.lock().unwrap() = true;
            }
            _ => {
                logging::log(
                    "Received unexpected response before connection established. Skipping.",
                );
            }
        }
    }
    // If the connection is established, handle the response accordingly
    else {
        match &from_ankaios.from_ankaios_enum {
            Some(FromAnkaiosEnum::Response(response)) => {
                let request_id: &String = &response.request_id;
                if request_id == UPDATE_STATE_REQUEST_ID {
                    if let ResponseContent::UpdateStateSuccess(update_state_success) =
                        response.response_content.clone().unwrap()
                    {
                        let added_workloads = &update_state_success.added_workloads.clone();
                        let deleted_workloads = &update_state_success.deleted_workloads.clone();
                        logging::log(&format!(
                            "Receiving Response for the UpdateStateRequest:\nadded workloads: {:#?}, deleted workloads: {:#?}",
                            added_workloads, deleted_workloads
                        ));
                    } else {
                        logging::log("Received UpdateStateRequest response, but no content found.");
                    }
                } else if request_id == COMPLETE_STATE_REQUEST_ID {
                    logging::log(&format!(
                        "Receiving Response for the CompleteStateRequest:\n{:#?}",
                        from_ankaios
                    ));
                } else {
                    logging::log(&format!(
                        "RequestId does not match. Skipping messages from requestId: {}",
                        request_id
                    ));
                }
            }
            Some(FromAnkaiosEnum::ConnectionClosed(_)) => {
                logging::log("Received Connection Closed response. Exiting..");
                *connection_closed.lock().unwrap() = true;
                *connected.lock().unwrap() = false;
            }
            _ => {
                logging::log("Received unknown message type. Skipping message.");
            }
        }
    }
}

fn write_to_control_interface(file_handle: &mut File, message: ToAnkaios) {
    let encoded_message = message.encode_length_delimited_to_vec();
    file_handle
        .write_all(&encoded_message)
        .unwrap_or_else(|err| {
            logging::log(&format!("Error writing to control interface: '{}'", err));
            exit(1);
        });
}

// =======================================================================
// Main
// =======================================================================
fn main() {
    // Check if the control interface fifo files exist
    let pipes_location = Path::new(ANKAIOS_CONTROL_INTERFACE_BASE_PATH);
    let input_pipe = pipes_location.join("input");
    let output_pipe = pipes_location.join("output");
    if !input_pipe.exists() || !output_pipe.exists() {
        logging::log("Error: Control interface FIFO files do not exist. Exiting..");
        exit(1);
    }

    // Flags to check if it's connected or the connection has been closed
    let connected = std::sync::Arc::new(std::sync::Mutex::new(false));
    let connection_closed = std::sync::Arc::new(std::sync::Mutex::new(false));

    // Spawn a thread to read from the control interface
    let connected_clone = connected.clone();
    let connection_closed_clone = connection_closed.clone();
    let read_handle = std::thread::spawn(move || {
        read_from_control_interface(input_pipe, connected_clone, connection_closed_clone);
    });

    // Open file for writing to the control interface
    let mut output_file = File::create(&output_pipe).unwrap_or_else(|err| {
        logging::log(&format!(
            "Error: cannot create '{}': '{}'",
            output_pipe.to_str().unwrap(),
            err
        ));
        exit(1);
    });

    // Send hello message to establish the connection
    let hello_message = create_hello_message();
    logging::log("Sending initial Hello message to establish connection...");
    write_to_control_interface(&mut output_file, hello_message);
    std::thread::sleep(Duration::from_secs(1)); // Give some time for the connection to be established
    assert!(
        *connected.lock().unwrap(),
        "Connection to Ankaios not established."
    );

    // Send the request to add the dynamic_nginx workload
    let update_workload_request = create_request_to_add_new_workload();
    write_to_control_interface(&mut output_file, update_workload_request);
    logging::log("Requesting to add the dynamic_nginx workload...");
    std::thread::sleep(Duration::from_secs(1)); // Give some time for the request to be processed

    while *connected.lock().unwrap() {
        // Send the request for the complete state
        let complete_state_request = create_request_for_complete_state();
        logging::log("Requesting complete state of the dynamic_nginx workload...");
        write_to_control_interface(&mut output_file, complete_state_request);
        std::thread::sleep(Duration::from_secs(WAITING_TIME_IN_SEC));
    }

    // Wait for the read thread to finish
    read_handle.join().unwrap();
}

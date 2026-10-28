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

use ankaios_api::ank_base::{
    request::RequestContent, response::ResponseContent, CompleteState, CompleteStateRequest,
    Dependencies, Request, RestartPolicy, State, Tags, UpdateStateRequest, Workload, WorkloadMap,
};
use ankaios_api::control_api::{
    from_ankaios::FromAnkaiosEnum, to_ankaios::ToAnkaiosEnum, FromAnkaios, Hello, ToAnkaios,
};

use prost::Message;
use std::{
    collections::HashMap,
    fs::File,
    io,
    io::{Read, Write},
    path::Path,
    process::exit,
    time::Duration,
};

const ANKAIOS_CONTROL_INTERFACE_BASE_PATH: &str = "/run/ankaios/control_interface";
const MAX_VARINT_SIZE: usize = 19;
const WAITING_TIME_IN_SEC: u64 = 5;
const UPDATE_STATE_REQUEST_ID: &str = "RWNsaXBzZSBBbmthaW9z";
const COMPLETE_STATE_REQUEST_ID: &str = "QW5rYWlvcyBpcyB0aGUgYmVzdA==";
const PROTOCOL_VERSION: &str = env!("ANKAIOS_VERSION");

mod logging {
    pub fn log(msg: &str) {
        println!(
            "[{}] {}",
            chrono::offset::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            msg
        );
    }
}

fn create_hello_message() -> ToAnkaios {
    /* Create hello message for connection */
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Hello(Hello {
            protocol_version: PROTOCOL_VERSION.to_string(),
        })),
    }
}

fn create_request_to_add_new_workload() -> ToAnkaios {
    /* Return request for adding a new workload. */
    let new_workloads = Some(WorkloadMap {
        workloads: HashMap::from([(
            "dynamic_nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                restart_policy: Some(RestartPolicy::Never.into()),
                tags: Some(Tags {
                    tags: HashMap::from([("owner".to_string(), "Ankaios team".to_string())]),
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
                            api_version: "v1".into(),
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

fn create_request_for_workload_state() -> ToAnkaios {
    /* Return request for getting the complete state */
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: COMPLETE_STATE_REQUEST_ID.to_string(),
            request_content: Some(RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![String::from("workloadStates.agent_A.dynamic_nginx")],
                ..Default::default()
            })),
        })),
    }
}

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
    /* Read protobuf data from the fifo pipe. */
    let varint_data = read_varint_data(file)?;
    let mut varint_data = Box::new(&varint_data[..]);

    // determine the exact size for exact reading of the bytes later by decoding the varint data
    let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

    let mut buf = vec![0; size];
    file.read_exact(&mut buf[..])?; // read exact bytes from file
    Ok(buf.into_boxed_slice())
}

fn read_from_control_interface(pipe_handle: &mut File) -> Result<FromAnkaios, ()> {
    /* Read and return one message from the input pipe. */
    match read_protobuf_data(pipe_handle) {
        Ok(binary) => match FromAnkaios::decode(&mut Box::new(binary.as_ref())) {
            Ok(from_ankaios) => Ok(from_ankaios),
            Err(err) => {
                logging::log(&format!("Invalid response, parsing error: '{}'", err));
                Err(())
            }
        },
        Err(err) => {
            logging::log(&format!("Error reading protobuf data: '{}'", err));
            Err(())
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

fn main() {
    // Check if the control interface fifo files exist
    let pipes_location = Path::new(ANKAIOS_CONTROL_INTERFACE_BASE_PATH);
    let input_pipe = pipes_location.join("input");
    let output_pipe = pipes_location.join("output");
    if !input_pipe.exists() || !output_pipe.exists() {
        logging::log("Error: Control interface FIFO files do not exist. Exiting..");
        exit(1);
    }

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
    logging::log("Sending initial Hello message to establish connection...");
    let hello_message = create_hello_message();
    write_to_control_interface(&mut output_file, hello_message);

    // Open file for reading from the control interface
    let mut input_file = File::open(&input_pipe).unwrap_or_else(|err| {
        logging::log(&format!(
            "Error: cannot open '{}': '{}'",
            input_pipe.to_str().unwrap(),
            err
        ));
        exit(1);
    });

    // Read the response for the hello message
    let response = read_from_control_interface(&mut input_file);
    assert!(response.is_ok());
    let Some(FromAnkaiosEnum::ControlInterfaceAccepted(response)) =
        response.unwrap().from_ankaios_enum
    else {
        panic!("No ControlInterfaceAccepted received.")
    };
    logging::log(&format!(
        "Receiving answer to the initial Hello:\n{:#?}",
        response
    ));

    logging::log("Requesting to add the dynamic_nginx workload...");
    let update_workload_request = create_request_to_add_new_workload();
    write_to_control_interface(&mut output_file, update_workload_request);

    let response = read_from_control_interface(&mut input_file);
    assert!(response.is_ok());
    let Some(FromAnkaiosEnum::Response(response)) = response.unwrap().from_ankaios_enum else {
        panic!("No response received.")
    };
    assert!(
        matches!(
            response.response_content,
            Some(ResponseContent::UpdateStateSuccess(_))
        ),
        "No UpdateStateSuccess received"
    );
    logging::log(&format!(
        "Receiving response for the UpdateStateRequest:\n{:#?}",
        response
    ));

    loop {
        logging::log("Requesting workload state of the dynamic_nginx workload...");
        let complete_state_request = create_request_for_workload_state();
        write_to_control_interface(&mut output_file, complete_state_request);

        let response = read_from_control_interface(&mut input_file);
        assert!(response.is_ok());
        let Some(FromAnkaiosEnum::Response(response)) = response.unwrap().from_ankaios_enum else {
            panic!("No response received.")
        };
        assert!(
            matches!(
                response.response_content,
                Some(ResponseContent::CompleteStateResponse(_))
            ),
            "No CompleteStateResponse received"
        );
        logging::log(&format!(
            "Receiving response for the CompleteStateRequest with filter 'workloadStates.agent_A.dynamic_nginx':\n{:#?}",
            response
        ));
        std::thread::sleep(Duration::from_secs(WAITING_TIME_IN_SEC));
    }
}


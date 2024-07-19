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

use api::ank_base::{
    request::RequestContent, CompleteState, CompleteStateRequest, Dependencies, Request,
    RestartPolicy, State, Tag, Tags, UpdateStateRequest, Workload, WorkloadMap,
};

use api::control_api::{
    from_ankaios::FromAnkaiosEnum, to_ankaios::ToAnkaiosEnum, FromAnkaios, ToAnkaios,
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
const REQUEST_ID: &str = "dynamic_nginx@rust_control_interface";

mod logging {
    pub fn log(msg: &str) {
        println!(
            "[{}] {}",
            chrono::offset::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            msg
        );
    }
}

/// Create the Request containing an UpdateStateRequest
/// that contains the details for adding the new workload and
/// the update mask to add only the new workload.
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
                control_interface_access: None,
            },
        )]),
    });

    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: REQUEST_ID.to_string(),
            request_content: Some(RequestContent::UpdateStateRequest(UpdateStateRequest {
                new_state: Some(CompleteState {
                    desired_state: Some(State {
                        api_version: "v0.1".into(),
                        workloads: new_workloads,
                    }),
                    ..Default::default()
                }),
                update_mask: vec!["desiredState.workloads.dynamic_nginx".to_string()],
            })),
        })),
    }
}

/// Create a Request to request the CompleteState
/// for querying the workload states.
fn create_request_for_complete_state() -> ToAnkaios {
    ToAnkaios {
        to_ankaios_enum: Some(ToAnkaiosEnum::Request(Request {
            request_id: REQUEST_ID.to_string(),
            request_content: Some(RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![String::from("workloadStates.agent_A.dynamic_nginx")],
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
    let varint_data = read_varint_data(file)?;
    let mut varint_data = Box::new(&varint_data[..]);

    // determine the exact size for exact reading of the bytes later by decoding the varint data
    let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

    let mut buf = vec![0; size];
    file.read_exact(&mut buf[..])?; // read exact bytes from file
    Ok(buf.into_boxed_slice())
}

/// Reads from the control interface input fifo and prints the workload states.
fn read_from_control_interface() {
    let pipes_location = Path::new(ANKAIOS_CONTROL_INTERFACE_BASE_PATH);
    let ex_req_fifo = pipes_location.join("input");

    let mut ex_req = File::open(&ex_req_fifo).unwrap_or_else(|err| {
        logging::log(&format!(
            "Error: cannot open '{}': '{}'",
            ex_req_fifo.to_str().unwrap(),
            err
        ));
        exit(1);
    });

    loop {
        if let Ok(binary) = read_protobuf_data(&mut ex_req) {
            match FromAnkaios::decode(&mut Box::new(binary.as_ref())) {
                Ok(from_ankaios) => {
                    let Some(FromAnkaiosEnum::Response(response)) = &from_ankaios.from_ankaios_enum
                    else {
                        logging::log("No response. Continue.");
                        continue;
                    };

                    let request_id: &String = &response.request_id;
                    if request_id == REQUEST_ID {
                        logging::log(&format!(
                            "Receiving Response containing the workload states of the current state:\n{:#?}",
                            from_ankaios
                        ));
                    } else {
                        logging::log(&format!(
                            "RequestId does not match. Skipping messages from requestId: {}",
                            request_id
                        ));
                    }
                }
                Err(err) => logging::log(&format!("Invalid response, parsing error: '{}'", err)),
            }
        }
    }
}

/// Writes a Request into the control interface output fifo
// to add the new workload dynamically and every x sec according to WAITING_TIME_IN_SEC
// another Request to request the workload states.
fn write_to_control_interface() {
    let pipes_location = Path::new(ANKAIOS_CONTROL_INTERFACE_BASE_PATH);
    let sc_req_fifo = pipes_location.join("output");

    let mut sc_req = File::create(&sc_req_fifo).unwrap_or_else(|err| {
        logging::log(&format!(
            "Error: cannot create '{}': '{}'",
            sc_req_fifo.to_str().unwrap(),
            err
        ));
        exit(1);
    });

    let protobuf_update_workload_request = create_request_to_add_new_workload();

    logging::log(format!("Sending Request containing details for adding the dynamic workload \"dynamic_nginx\":\n{:#?}", protobuf_update_workload_request).as_str());

    sc_req
        .write_all(&protobuf_update_workload_request.encode_length_delimited_to_vec())
        .unwrap();

    let protobuf_request_complete_state_request = create_request_for_complete_state();
    loop {
        logging::log(
            format!(
                "Sending Request containing details for requesting all workload states:\n{:#?}",
                protobuf_request_complete_state_request
            )
            .as_str(),
        );
        sc_req
            .write_all(&protobuf_request_complete_state_request.encode_length_delimited_to_vec())
            .unwrap();

        std::thread::sleep(Duration::from_secs(WAITING_TIME_IN_SEC));
    }
}

fn main() {
    let handle = std::thread::spawn(read_from_control_interface);
    write_to_control_interface();
    handle.join().unwrap();
}

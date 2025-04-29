// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use api::ank_base::response::ResponseContent;
use api::ank_base::{State, UpdateStateRequest};

use api::control_api::{from_ankaios::FromAnkaiosEnum, FromAnkaios};

use prost::Message;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::env::args;
use std::{
    fs::File,
    io,
    io::{Read, Write},
    path::Path,
    process::exit,
};

const ANKAIOS_CONTROL_INTERFACE_BASE_PATH: &str = "/run/ankaios/control_interface";
const MAX_VARINT_SIZE: usize = 19;

mod logging {
    pub fn log(msg: &str) {
        eprintln!(
            "[{}] {}",
            chrono::offset::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            msg
        );
    }
}

#[derive(Deserialize)]
struct Command {
    command: CommandEnum,
}

enum CommandError {
    ConnectionClosed(String),
    GenericError(String),
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum CommandEnum {
    UpdateState(UpdateState),
    GetState(GetState),
    SendHello(Version),
}

#[derive(Deserialize)]
struct Version {
    version: String,
}

#[derive(Deserialize)]
struct UpdateState {
    manifest_file: String,
    update_mask: Vec<String>,
}

#[derive(Deserialize)]
struct GetState {
    field_mask: Vec<String>,
}

#[derive(Serialize)]
struct TestResult {
    result: TestResultEnum,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum TestResultEnum {
    UpdateStateResult(TagSerializedResult<UpdateStateResult>),
    GetStateResult(TagSerializedResult<Option<State>>),
    NoApi,
    SendHelloResult(TagSerializedResult<()>),
    ConnectionClosed,
}

#[derive(Serialize)]
struct UpdateStateResult {
    added_workloads: Vec<String>,
    deleted_workloads: Vec<String>,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value")]
enum TagSerializedResult<T> {
    Ok(T),
    Err(String),
}

impl<T> From<Result<T, String>> for TagSerializedResult<T> {
    fn from(value: Result<T, String>) -> Self {
        match value {
            Ok(res) => Self::Ok(res),
            Err(err) => Self::Err(err),
        }
    }
}

fn main() {
    let mut args = args();
    args.next();
    let Some(input_path) = args.next() else {
        logging::log("Input file argument is missing");
        exit(1)
    };
    let Some(output_path) = args.next() else {
        logging::log("Output file argument is missing");
        exit(1)
    };

    let commands_json = File::open(input_path).unwrap_or_else(|err| {
        logging::log(&format!("Could not open input file: '{}'", err));
        exit(1);
    });

    let commands: Vec<Command> = serde_yaml::from_reader(commands_json).unwrap_or_else(|err| {
        logging::log(&format!("Could not parse commands argument: '{}'", err));
        exit(1)
    });

    let result = if let Ok(mut connection) = Connection::new() {
        commands
            .into_iter()
            .map(|x| connection.handle_command(x))
            .collect::<Result<Vec<_>, _>>()
    } else {
        Ok(vec![TestResult {
            result: TestResultEnum::NoApi,
        }])
    };

    match result {
        Ok(result) => {
            write_result(output_path, result);
        }
        Err(CommandError::ConnectionClosed(err)) => {
            logging::log(&format!(
                "Connection to Ankaios server was closed: '{}'",
                err
            ));
            write_result(
                output_path,
                vec![TestResult {
                    result: TestResultEnum::ConnectionClosed,
                }],
            );
        }
        Err(CommandError::GenericError(err)) => {
            logging::log(&format!("Failed during test execution: {}", err));
            exit(3);
        }
    }
}

fn write_result(output_path: String, result: Vec<TestResult>) {
    let output_file = File::create(output_path).unwrap_or_else(|err| {
        logging::log(&format!("Could not open output file: '{}'", err));
        exit(4);
    });
    serde_json::to_writer(output_file, &result).unwrap_or_else(|err| {
        logging::log(&format!("Could not write to open output file: '{}'", err));
        exit(5);
    });
}

struct Connection {
    id_counter: i32,
    output: File,
    input: File,
}

impl Connection {
    pub fn new() -> Result<Self, String> {
        let pipes_location = Path::new(ANKAIOS_CONTROL_INTERFACE_BASE_PATH);
        let output_fifo = pipes_location.join("output");

        let output = File::create(&output_fifo).map_err(|err| {
            format!(
                "Error: cannot create '{}': '{}'",
                output_fifo.to_str().unwrap(),
                err
            )
        })?;

        let input_fifo = pipes_location.join("input");

        let input = File::open(&input_fifo).map_err(|err| {
            format!(
                "Error: cannot open '{}': '{}'",
                input_fifo.to_str().unwrap(),
                err
            )
        })?;

        Ok(Connection {
            id_counter: 0,
            output,
            input,
        })
    }

    fn handle_command(&mut self, command: Command) -> Result<TestResult, CommandError> {
        Ok(TestResult {
            result: match command.command {
                CommandEnum::UpdateState(update_state_command) => {
                    self.handle_update_state_command(update_state_command)?
                }
                CommandEnum::GetState(get_state_command) => {
                    self.handle_get_state_command(get_state_command)?
                }
                CommandEnum::SendHello(Version { version }) => self.send_hello(version)?,
            },
        })
    }

    fn send_hello(&mut self, protocol_version: String) -> Result<TestResultEnum, CommandError> {
        let proto = api::control_api::ToAnkaios {
            to_ankaios_enum: Some(api::control_api::to_ankaios::ToAnkaiosEnum::Hello(
                api::control_api::Hello { protocol_version },
            )),
        };

        Ok(TestResultEnum::SendHelloResult(TagSerializedResult::Ok(
            self.output
                .write_all(&proto.encode_length_delimited_to_vec())
                .map_err(|err| CommandError::GenericError(err.to_string()))?,
        )))
    }

    pub fn handle_update_state_command(
        &mut self,
        update_state_command: UpdateState,
    ) -> Result<TestResultEnum, CommandError> {
        let request_id = self.get_next_id();

        let state: common::objects::CompleteState =
            read_yaml_file(Path::new(&update_state_command.manifest_file))
                .map_err(CommandError::GenericError)?;

        let request = common::commands::Request {
            request_id: request_id.clone(),
            request_content: common::commands::RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    new_state: Some(state.into()),
                    update_mask: update_state_command.update_mask,
                }
                .try_into()
                .map_err(CommandError::GenericError)?,
            )),
        };

        let proto = api::control_api::ToAnkaios {
            to_ankaios_enum: Some(api::control_api::to_ankaios::ToAnkaiosEnum::Request(
                request.into(),
            )),
        };

        self.output
            .write_all(&proto.encode_length_delimited_to_vec())
            .unwrap();

        let response = self.wait_for_response(request_id)?;

        Ok(TestResultEnum::UpdateStateResult(match response {
            ResponseContent::UpdateStateSuccess(response) => {
                TagSerializedResult::Ok(UpdateStateResult {
                    added_workloads: response.added_workloads,
                    deleted_workloads: response.deleted_workloads,
                })
            }
            response_content => TagSerializedResult::Err(format!(
                "Received wrong response type. Expected UpdateStateSuccess, received: '{:?}'",
                response_content
            )),
        }))
    }

    pub fn handle_get_state_command(
        &mut self,
        get_state_command: GetState,
    ) -> Result<TestResultEnum, CommandError> {
        let request_id = self.get_next_id();

        let request = common::commands::Request {
            request_id: request_id.clone(),
            request_content: common::commands::RequestContent::CompleteStateRequest(
                common::commands::CompleteStateRequest {
                    field_mask: get_state_command.field_mask,
                },
            ),
        };

        let proto = api::control_api::ToAnkaios {
            to_ankaios_enum: Some(api::control_api::to_ankaios::ToAnkaiosEnum::Request(
                request.into(),
            )),
        };

        self.output
            .write_all(&proto.encode_length_delimited_to_vec())
            .unwrap();

        let response = self.wait_for_response(request_id)?;

        Ok(TestResultEnum::GetStateResult(match response {
            ResponseContent::CompleteState(complete_state) => {
                TagSerializedResult::Ok(complete_state.desired_state)
            }
            response_content => TagSerializedResult::Err(format!(
                "Received wrong response type. Expected CompleteState, received: '{:?}'",
                response_content
            )),
        }))
    }

    fn wait_for_response(
        &mut self,
        target_request_id: String,
    ) -> Result<ResponseContent, CommandError> {
        loop {
            let message = self.read_message().map_err(CommandError::GenericError)?;

            match message {
                FromAnkaiosEnum::Response(response) => {
                    if response.request_id.eq(&target_request_id) {
                        if let Some(response_content) = response.response_content {
                            return Ok(response_content);
                        } else {
                            return Err(CommandError::GenericError(format!(
                                "Received Response with correct request_id, but without content. Request Id: '{:?}'",
                                response.request_id
                            )));
                        }
                    } else {
                        logging::log(&format!(
                            "Received unexpected response for request {:}",
                            response.request_id
                        ));
                    }
                }
                FromAnkaiosEnum::ConnectionClosed(_) => {
                    return Err(CommandError::ConnectionClosed(
                        "Control Interface connection closed by Ankaios.".into(),
                    ))
                }
            }
        }
    }

    fn read_message(&mut self) -> Result<FromAnkaiosEnum, String> {
        let binary = self
            .read_protobuf_data()
            .map_err(|err| format!("Failed to read message from input stream: '{}'", err))?;
        FromAnkaios::decode(&mut Box::new(binary.as_ref()))
            .map_err(|err| format!("Could not decode proto received from input: '{}'", err))?
            .from_ankaios_enum
            .ok_or_else(|| "The field FromAnkaiosEnum is not set".to_string())
    }

    fn read_protobuf_data(&mut self) -> Result<Box<[u8]>, io::Error> {
        let varint_data = self.read_varint_data()?;
        let mut varint_data = Box::new(&varint_data[..]);

        // determine the exact size for exact reading of the bytes later by decoding the varint data
        let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

        let mut buf = vec![0; size];
        self.input.read_exact(&mut buf[..])?; // read exact bytes from file
        Ok(buf.into_boxed_slice())
    }

    fn read_varint_data(&mut self) -> Result<[u8; MAX_VARINT_SIZE], io::Error> {
        let mut res = [0u8; MAX_VARINT_SIZE];
        let mut one_byte_buffer = [0u8; 1];
        for item in res.iter_mut() {
            self.input.read_exact(&mut one_byte_buffer)?;
            *item = one_byte_buffer[0];
            // check if most significant bit is set to 0 if so it is the last byte to be readxxxxxxfff
            if *item & 0b10000000 == 0 {
                break;
            }
        }
        Ok(res)
    }

    pub fn get_next_id(&mut self) -> String {
        self.id_counter += 1;
        format!("id-{}", self.id_counter)
    }
}

fn read_yaml_file<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = File::open(path)
        .map_err(|err| format!("Error: cannot open '{}': '{}'", path.to_str().unwrap(), err))?;

    serde_yaml::from_reader(file)
        .map_err(|err| format!("Could not parse '{}': {}", path.to_str().unwrap(), err))
}

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
use api::ank_base::{
    CompleteStateResponse, LogEntriesResponse, LogsCancelAccepted, LogsRequestAccepted, State,
    UpdateStateRequest,
};

use api::control_api::{FromAnkaios, from_ankaios::FromAnkaiosEnum};

use prost::Message;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::env::args;
use std::path::PathBuf;
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
    RequestLogs(RequestLogs),
    GetLogs(GetLogs),
    CancelLogs(CancelLogs),
}

#[derive(Deserialize, Debug)]
struct Version {
    version: String,
}

#[derive(Deserialize, Debug)]
struct UpdateState {
    manifest_file: String,
    update_mask: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct GetState {
    field_mask: Vec<String>,
}

#[derive(Deserialize)]
struct RequestLogs {
    workload_names: Vec<String>,
    agent_names: Vec<String>,
    request_id: String,
}

#[derive(Deserialize)]
struct GetLogs {
    request_id: String,
}

#[derive(Deserialize)]
struct CancelLogs {
    request_id: String,
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
    LogRequestResponse(TagSerializedResult<LogsRequestAccepted>),
    LogEntriesResponse(TagSerializedResult<LogEntriesResponse>),
    LogCancelResponse(TagSerializedResult<LogsCancelAccepted>),
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
        logging::log(&format!("Could not open input file: '{err}'"));
        exit(1);
    });

    let commands: Vec<Command> = serde_yaml::from_reader(commands_json).unwrap_or_else(|err| {
        logging::log(&format!("Could not parse commands argument: '{err}'"));
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
            logging::log(&format!("Connection to Ankaios server was closed: '{err}'"));
            write_result(
                output_path,
                vec![TestResult {
                    result: TestResultEnum::ConnectionClosed,
                }],
            );
        }
        Err(CommandError::GenericError(err)) => {
            logging::log(&format!("Failed during test execution: {err}"));
            exit(3);
        }
    }
}

fn write_result(output_path: String, result: Vec<TestResult>) {
    let output_file = File::create(output_path).unwrap_or_else(|err| {
        logging::log(&format!("Could not open output file: '{err}'"));
        exit(4);
    });
    serde_json::to_writer(output_file, &result).unwrap_or_else(|err| {
        logging::log(&format!("Could not write to open output file: '{err}'"));
        exit(5);
    });
}

struct Connection {
    id_counter: i32,
    output: File,
    input: InputPipe,
}

enum InputPipe {
    Path(PathBuf),
    File(File),
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

        Ok(Connection {
            id_counter: 0,
            output,
            input: InputPipe::Path(input_fifo),
        })
    }

    fn handle_command(&mut self, command: Command) -> Result<TestResult, CommandError> {
        Ok(TestResult {
            result: match command.command {
                CommandEnum::UpdateState(update_state_command) => {
                    logging::log("Executing command: UpdateState");
                    self.handle_update_state_command(update_state_command)?
                }
                CommandEnum::GetState(get_state_command) => {
                    logging::log("Executing command: GetState");
                    self.handle_get_state_command(get_state_command)?
                }
                CommandEnum::SendHello(Version { version }) => self.send_hello(version)?,
                CommandEnum::RequestLogs(RequestLogs {
                    workload_names,
                    agent_names,
                    request_id,
                }) => self.handle_request_logs_command(workload_names, agent_names, request_id)?,
                CommandEnum::GetLogs(GetLogs { request_id }) => {
                    logging::log("Executing command: GetLogs");

                    self.handle_get_logs_command(request_id)?
                }
                CommandEnum::CancelLogs(CancelLogs { request_id }) => {
                    logging::log("Executing command: CancelLogs");

                    self.handle_cancel_logs_command(request_id)?
                }
            },
        })
    }

    fn send_hello(&mut self, protocol_version: String) -> Result<TestResultEnum, CommandError> {
        logging::log("Executing command: SendHello");

        let proto = api::control_api::ToAnkaios {
            to_ankaios_enum: Some(api::control_api::to_ankaios::ToAnkaiosEnum::Hello(
                api::control_api::Hello { protocol_version },
            )),
        };

        self.output
            .write_all(&proto.encode_length_delimited_to_vec())
            .map_err(|err| CommandError::GenericError(err.to_string()))?;

        match self.read_message().map_err(CommandError::GenericError)? {
            FromAnkaiosEnum::ControlInterfaceAccepted(_) => {
                logging::log("Received ControlInterfaceAccepted message from Ankaios.");
                Ok(TestResultEnum::SendHelloResult(TagSerializedResult::Ok(())))
            }
            _ => Err(CommandError::GenericError(
                "Expected ControlInterfaceAccepted message, but received something else.".into(),
            )),
        }
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
                "Received wrong response type. Expected UpdateStateSuccess, received: '{response_content:?}'"
            )),
        }))
    }

    pub fn get_complete_state(
        &mut self,
        field_mask: Vec<String>,
    ) -> Result<ResponseContent, CommandError> {
        let request_id = self.get_next_id();

        let request = common::commands::Request {
            request_id: request_id.clone(),
            request_content: common::commands::RequestContent::CompleteStateRequest(
                common::commands::CompleteStateRequest {
                    field_mask,
                    subscribe: false,
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

        self.wait_for_response(request_id)
    }

    pub fn handle_get_state_command(
        &mut self,
        get_state_command: GetState,
    ) -> Result<TestResultEnum, CommandError> {
        let response = self.get_complete_state(get_state_command.field_mask)?;

        Ok(TestResultEnum::GetStateResult(match response {
            ResponseContent::CompleteState(CompleteStateResponse {
                complete_state: Some(complete_state),
                ..
            }) => TagSerializedResult::Ok(complete_state.desired_state),
            response_content => TagSerializedResult::Err(format!(
                "Received wrong response type. Expected CompleteState, received: '{response_content:?}'"
            )),
        }))
    }

    fn wait_for_response(
        &mut self,
        target_request_id: String,
    ) -> Result<ResponseContent, CommandError> {
        loop {
            let message = self.read_message().map_err(CommandError::GenericError)?;
            logging::log(&format!("Received message: {message:?}"));

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
                FromAnkaiosEnum::ControlInterfaceAccepted(_) => {
                    logging::log(
                        "Received ControlInterfaceAccepted message from Ankaios. Ignoring..",
                    );
                }
                FromAnkaiosEnum::ConnectionClosed(_) => {
                    return Err(CommandError::ConnectionClosed(
                        "Control Interface connection closed by Ankaios.".into(),
                    ));
                }
            }
        }
    }

    fn handle_request_logs_command(
        &mut self,
        workload_names: Vec<String>,
        agent_names: Vec<String>,
        request_id: String,
    ) -> Result<TestResultEnum, CommandError> {
        logging::log("Executing command: RequestLogs");

        if workload_names.is_empty() || agent_names.is_empty() {
            return Err(CommandError::GenericError(
                "Workload names and agent names cannot be empty".into(),
            ));
        }
        if workload_names.len() != agent_names.len() {
            return Err(CommandError::GenericError(
                "Workload names and agent names must have the same length".into(),
            ));
        }

        // Get the workload states to extract the workload instance names
        let workload_states_response = self.get_complete_state(vec!["workloadStates".into()])?;
        let workload_states = match workload_states_response {
            ResponseContent::CompleteState(CompleteStateResponse{complete_state: Some(complete_state), ..}) => complete_state.workload_states,
            response_content => {
                return Err(CommandError::GenericError(format!(
                    "Received wrong response type. Expected CompleteState, received: '{response_content:?}'"
                )));
            }
        }
        .expect("Expected workload states to be present in the response");

        let mut workload_instance_names = Vec::new();
        for (workload_name, agent_name) in workload_names.iter().zip(agent_names.iter()) {
            let workload_id = workload_states
                .agent_state_map
                .get(agent_name)
                .ok_or_else(|| {
                    CommandError::GenericError(format!(
                        "Agent '{agent_name}' not found in workload states"
                    ))
                })?
                .wl_name_state_map
                .get(workload_name)
                .ok_or_else(|| {
                    CommandError::GenericError(format!(
                        "Workload '{workload_name}' not found in agent '{agent_name}' workload states"
                    ))
                })?
                .id_state_map
                .keys()
                .next()
                .cloned()
                .ok_or_else(|| {
                    CommandError::GenericError(format!(
                        "No workload instance found for workload '{workload_name}' in agent '{agent_name}'"
                    ))
                })?;

            workload_instance_names.push(common::objects::WorkloadInstanceName::new(
                agent_name.clone(),
                workload_name.clone(),
                workload_id.clone(),
            ));
        }

        let request = common::commands::Request {
            request_id: request_id.clone(),
            request_content: common::commands::RequestContent::LogsRequest(
                common::commands::LogsRequest {
                    workload_names: workload_instance_names,
                    follow: true,
                    tail: -1,
                    since: None,
                    until: None,
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
        match response {
            ResponseContent::LogsRequestAccepted(logs_response) => Ok(
                TestResultEnum::LogRequestResponse(TagSerializedResult::Ok(logs_response)),
            ),
            ResponseContent::Error(error) => Ok(TestResultEnum::LogEntriesResponse(
                TagSerializedResult::Err(error.message),
            )),
            response_content => Err(CommandError::GenericError(format!(
                "Received wrong response type. Expected LogsRequestAccepted, received: '{response_content:?}'"
            ))),
        }
    }

    fn handle_get_logs_command(
        &mut self,
        request_id: String,
    ) -> Result<TestResultEnum, CommandError> {
        let response = self.wait_for_response(request_id)?;

        match response {
            ResponseContent::LogEntriesResponse(logs_response) => Ok(
                TestResultEnum::LogEntriesResponse(TagSerializedResult::Ok(logs_response)),
            ),
            response_content => Err(CommandError::GenericError(format!(
                "Received wrong response type. Expected LogsResponse, received: '{response_content:?}'"
            ))),
        }
    }

    fn handle_cancel_logs_command(
        &mut self,
        request_id: String,
    ) -> Result<TestResultEnum, CommandError> {
        let request = common::commands::Request {
            request_id: request_id.clone(),
            request_content: common::commands::RequestContent::LogsCancelRequest,
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
        match response {
            ResponseContent::LogsCancelAccepted(logs_response) => Ok(
                TestResultEnum::LogCancelResponse(TagSerializedResult::Ok(logs_response)),
            ),
            ResponseContent::Error(error) => Ok(TestResultEnum::LogEntriesResponse(
                TagSerializedResult::Err(error.message),
            )),
            response_content => Err(CommandError::GenericError(format!(
                "Received wrong response type. Expected LogsCancelAccepted, received: '{response_content:?}'"
            ))),
        }
    }

    fn read_message(&mut self) -> Result<FromAnkaiosEnum, String> {
        let binary = self
            .read_protobuf_data()
            .map_err(|err| format!("Failed to read message from input stream: '{err}'"))?;
        FromAnkaios::decode(&mut Box::new(binary.as_ref()))
            .map_err(|err| format!("Could not decode proto received from input: '{err}'"))?
            .from_ankaios_enum
            .ok_or_else(|| "The field FromAnkaiosEnum is not set".to_string())
    }

    fn read_protobuf_data(&mut self) -> Result<Box<[u8]>, io::Error> {
        let varint_data = self.read_varint_data()?;
        let mut varint_data = Box::new(&varint_data[..]);

        // determine the exact size for exact reading of the bytes later by decoding the varint data
        let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

        let mut buf = vec![0; size];
        self.read_exact(&mut buf[..])?; // read exact bytes from file
        Ok(buf.into_boxed_slice())
    }

    fn read_varint_data(&mut self) -> Result<[u8; MAX_VARINT_SIZE], io::Error> {
        let mut res = [0u8; MAX_VARINT_SIZE];
        let mut one_byte_buffer = [0u8; 1];
        for item in res.iter_mut() {
            self.read_exact(&mut one_byte_buffer)?;
            *item = one_byte_buffer[0];
            // check if most significant bit is set to 0 if so it is the last byte to be readxxxxxxfff
            if *item & 0b10000000 == 0 {
                break;
            }
        }
        Ok(res)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), io::Error> {
        if let InputPipe::Path(path) = &mut self.input {
            let file = File::open(path)?;
            self.input = InputPipe::File(file);
        }

        if let InputPipe::File(file) = &mut self.input {
            file.read_exact(buf)
        } else {
            unreachable!()
        }
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

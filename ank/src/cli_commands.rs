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

use std::{fmt, time::Duration};

#[cfg(not(test))]
async fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

use common::{
    commands::{CompleteState, RequestCompleteState},
    execution_interface::ExecutionCommand,
    objects::{Tag, WorkloadSpec},
    state_change_interface::{StateChangeCommand, StateChangeInterface},
};

#[cfg(not(test))]
use common::communications_client::CommunicationsClient;
#[cfg(not(test))]
use grpc::client::GRPCCommunicationsClient;

#[cfg(test)]
use tests::MockGRPCCommunicationsClient as GRPCCommunicationsClient;

use tabled::{settings::Style, Table, Tabled};
use tokio::time::timeout;
use url::Url;

use crate::{cli::OutputFormat, output_and_error, output_debug};

const BUFFER_SIZE: usize = 20;

#[derive(Debug, Clone)]
pub enum CliError {
    YamlSerialization(String),
    JsonSerialization(String),
    ExecutionError(String),
    ConnectionTimeout(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CliError::YamlSerialization(message) => {
                write!(f, "Could not serialize YAML object: '{message}'")
            }
            CliError::JsonSerialization(message) => {
                write!(f, "Could not serialize JSON object: '{message}'")
            }
            CliError::ExecutionError(message) => {
                write!(f, "Command failed: '{}'", message)
            }
            CliError::ConnectionTimeout(message) => {
                write!(f, "ConnectionTimeout: '{}'", message)
            }
        }
    }
}

impl From<serde_yaml::Error> for CliError {
    fn from(value: serde_yaml::Error) -> Self {
        CliError::YamlSerialization(format!("{value}"))
    }
}

impl From<serde_json::Error> for CliError {
    fn from(value: serde_json::Error) -> Self {
        CliError::JsonSerialization(format!("{value}"))
    }
}

fn generate_compact_state_output(
    state: &CompleteState,
    object_field_mask: Vec<String>,
    output_format: OutputFormat,
) -> Result<String, CliError> {
    let convert_to_output = |map: serde_yaml::Value| -> Result<String, CliError> {
        match output_format {
            // [impl -> swdd~cli-shall-support-current-state-yaml~1]
            OutputFormat::Yaml => Ok(serde_yaml::to_string(&map)?),
            // [impl -> swdd~cli-shall-support-current-state-json~1]
            OutputFormat::Json => Ok(serde_json::to_string_pretty(&map)?),
        }
    };

    let deserialized_state: serde_yaml::Value = serde_yaml::to_value(state)?;

    if object_field_mask.is_empty() {
        return convert_to_output(deserialized_state);
    }

    let mut compact_state = serde_yaml::Value::Mapping(Default::default());
    for mask in object_field_mask {
        let splitted_masks: Vec<&str> = mask.split('.').collect();
        if let Some(filtered_mapping) = get_filtered_value(&deserialized_state, &splitted_masks) {
            update_compact_state(
                &mut compact_state,
                &splitted_masks,
                filtered_mapping.to_owned(),
            );
        }
    }

    convert_to_output(compact_state)
}

fn get_filtered_value<'a>(
    map: &'a serde_yaml::Value,
    mask: &[&str],
) -> Option<&'a serde_yaml::Value> {
    mask.iter().fold(Some(map), |current_level, mask_part| {
        current_level?.get(mask_part)
    })
}

fn update_compact_state(
    new_compact_state: &mut serde_yaml::Value,
    mask: &[&str],
    new_mapping: serde_yaml::Value,
) -> Option<()> {
    if mask.is_empty() {
        return Some(());
    }

    let mut current_level = new_compact_state;

    for mask_part in mask {
        if current_level.get(mask_part).is_some() {
            current_level = current_level.get_mut(mask_part)?;
            continue;
        }

        if let serde_yaml::Value::Mapping(current_mapping) = current_level {
            current_mapping.insert(
                (*mask_part).into(),
                serde_yaml::Value::Mapping(Default::default()),
            );

            current_level = current_mapping.get_mut(mask_part)?;
        } else {
            return None;
        }
    }

    *current_level = new_mapping;
    Some(())
}

// [impl->swdd~server-handle-cli-communication~1]
// [impl->swdd~cli-communication-over-middleware~1]
fn setup_cli_communication(
    cli_name: &str,
    server_url: Url,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::Sender<StateChangeCommand>,
    tokio::sync::mpsc::Receiver<ExecutionCommand>,
) // (task,sender,receiver)
{
    let mut grpc_communications_client =
        GRPCCommunicationsClient::new_cli_communication(cli_name.to_owned(), server_url);

    let (to_cli, cli_receiver) = tokio::sync::mpsc::channel::<ExecutionCommand>(BUFFER_SIZE);
    let (to_server, server_receiver) =
        tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    let communications_task = tokio::spawn(async move {
        if let Err(err) = grpc_communications_client
            .run(server_receiver, to_cli.clone())
            .await
        {
            output_and_error!("{err}");
        }
    });
    (communications_task, to_server, cli_receiver)
}

#[derive(Debug, Tabled)]
#[tabled(rename_all = "UPPERCASE")]
struct WorkloadInfo {
    #[tabled(rename = "WORKLOAD NAME")]
    name: String,
    agent: String,
    runtime: String,
    #[tabled(rename = "EXECUTION STATE")]
    execution_state: String,
}

pub struct CliCommands {
    response_timeout_ms: u64,
    cli_name: String,
    task: tokio::task::JoinHandle<()>,
    to_server: tokio::sync::mpsc::Sender<StateChangeCommand>,
    from_server: tokio::sync::mpsc::Receiver<ExecutionCommand>,
}

impl Drop for CliCommands {
    fn drop(&mut self) {
        self.task.abort(); // abort task to signalize the server to close cli connection
    }
}

impl CliCommands {
    pub fn init(response_timeout_ms: u64, cli_name: String, server_url: Url) -> Self {
        let (task, to_server, from_server) =
            setup_cli_communication(cli_name.as_str(), server_url.clone());
        Self {
            response_timeout_ms,
            cli_name,
            task,
            to_server,
            from_server,
        }
    }

    pub async fn get_state(
        &mut self,
        object_field_mask: Vec<String>,
        output_format: OutputFormat,
    ) -> Option<String> {
        let mut out_command_text: Option<String> = None;
        output_debug!(
            "Got: object_field_mask={:?} output_format={:?}",
            object_field_mask,
            output_format
        );

        // send request
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: object_field_mask.clone(),
            })
            .await
            .ok()?;

        if let Some(ExecutionCommand::CompleteState(res)) = self.from_server.recv().await {
            out_command_text =
                // [impl->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
                match generate_compact_state_output(&res, object_field_mask, output_format) {
                    Ok(res) => Some(res),
                    Err(err) => {
                        output_and_error!(
                            "Error occurred during processing response from server.\nError: {err}"
                        );
                        None
                    }
                }
        }

        out_command_text
    }

    pub async fn set_state(
        &mut self,
        object_field_mask: Vec<String>,
        state_object_file: Option<String>,
        response_timeout_ms: u64,
    ) {
        output_debug!(
            "Got: object_field_mask={:?} state_object_file={:?}",
            object_field_mask,
            state_object_file
        );
        let mut complete_state_input = CompleteState::default();
        if state_object_file.is_some() {
            let state_object_data = read_file_to_string(state_object_file.unwrap())
                .await
                .unwrap_or_else(|error| {
                    panic!("Could not read the state object file.\nError: {error}")
                });
            // [impl -> swdd~cli-supports-yaml-to-set-current-state~1]
            complete_state_input =
                serde_yaml::from_str(&state_object_data).unwrap_or_else(|error| {
                    panic!("Error while parsing the state object data.\nError: {error}")
                });
        }

        output_debug!("Send UpdateState request ...");
        // send update request
        self.to_server
            .update_state(complete_state_input, object_field_mask)
            .await
            .unwrap_or_else(|err| {
                output_and_error!("Update state failed: '{}'", err);
            });
        if (timeout(
            Duration::from_millis(response_timeout_ms),
            self.from_server.recv(),
        )
        .await)
            .is_err()
        {
            output_debug!("Ok");
        }
    }

    // [impl->swdd~cli-provides-list-of-workloads~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1]
    // [impl->swdd~cli-shall-print-empty-table~1]
    pub async fn get_workloads(
        &mut self,
        agent_name: Option<String>,
        state: Option<String>,
        workload_name: Vec<String>,
    ) -> Result<String, CliError> {
        // send request
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: Vec::new(),
            })
            .await
            .map_err(|err| CliError::ExecutionError(err.to_string()))?;

        if let Some(ExecutionCommand::CompleteState(res)) = self.from_server.recv().await {
            let mut workload_infos: Vec<WorkloadInfo> = res
                .current_state
                .workloads
                .values()
                .cloned()
                .map(|w| WorkloadInfo {
                    name: w.name,
                    agent: w.agent,
                    runtime: w.runtime,
                    execution_state: String::new(),
                })
                .collect();

            // [impl->swdd~cli-shall-filter-list-of-workloads~1]
            for wi in &mut workload_infos {
                if let Some(ws) = res
                    .workload_states
                    .iter()
                    .find(|ws| ws.agent_name == wi.agent && ws.workload_name == wi.name)
                {
                    wi.execution_state = ws.execution_state.to_string();
                }
            }
            output_debug!("The table before filtering:\n{:?}", workload_infos);

            // [impl->swdd~cli-shall-filter-list-of-workloads~1]
            if agent_name.is_some() {
                workload_infos.retain(|wi| &wi.agent == agent_name.as_ref().unwrap());
            }

            // [impl->swdd~cli-shall-filter-list-of-workloads~1]
            if state.is_some() {
                workload_infos.retain(|wi| {
                    wi.execution_state.to_lowercase() == state.as_ref().unwrap().to_lowercase()
                });
            }

            // [impl->swdd~cli-shall-filter-list-of-workloads~1]
            if !workload_name.is_empty() {
                workload_infos.retain(|wi| workload_name.iter().any(|wn| wn == &wi.name));
            }

            // The order of workloads in RequestCompleteState is not sable -> make sure that the user sees always the same order.
            // [impl->swdd~cli-shall-sort-list-of-workloads~1]
            workload_infos.sort_by_key(|wi| wi.name.clone());

            output_debug!("The table after filtering:\n{:?}", workload_infos);

            // [impl->swdd~cli-shall-present-list-workloads-as-table~1]
            return Ok(Table::new(workload_infos).with(Style::blank()).to_string());
        }
        // [impl->swdd~cli-returns-list-of-workloads-from-server~1]
        Err(CliError::ExecutionError(
            "Failed to get complete state of server.".to_string(),
        ))
    }

    // [impl->swdd~cli-provides-delete-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~1]
    pub async fn delete_workloads(&mut self, workload_names: Vec<String>) -> Result<(), CliError> {
        // get current state
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: Vec::new(),
            })
            .await
            .map_err(|err| CliError::ExecutionError(err.to_string()))?;

        let res = self
            .from_server
            .recv()
            .await
            .ok_or(CliError::ExecutionError(
                "Failed to get execution command from server".to_string(),
            ))?;

        let complete_state = if let ExecutionCommand::CompleteState(res) = res {
            res
        } else {
            return Err(CliError::ExecutionError(
                "Expected complete state".to_string(),
            ));
        };

        output_debug!("Got current state: {:?}", complete_state);
        let mut new_state = *complete_state.clone();
        // Filter out workloads to be deleted.
        new_state
            .current_state
            .workloads
            .retain(|k, _v| !workload_names.clone().into_iter().any(|wn| &wn == k));

        // Filter out workload statuses of the workloads to be deleted.
        // Only a nice-to-have, but it could be better to avoid sending misleading information
        new_state.workload_states.retain(|ws| {
            !workload_names
                .clone()
                .into_iter()
                .any(|wn| wn == ws.workload_name)
        });

        let update_mask = vec!["currentState".to_string()];
        if new_state.current_state != complete_state.current_state {
            output_debug!("Sending the new state {:?}", new_state);
            self.to_server
                .update_state(new_state, update_mask)
                .await
                .map_err(|err| CliError::ExecutionError(err.to_string()))?;

            timeout(
                Duration::from_millis(self.response_timeout_ms),
                self.from_server.recv(),
            )
            .await
            .map_err(|_| CliError::ConnectionTimeout("No response from the server".to_string()))?;
        } else {
            // [impl->swdd~no-delete-workloads-when-not-found~1]
            output_debug!("Current and new states are identical -> nothing to do");
        }

        Ok(())
    }

    // [impl->swdd~cli-provides-run-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-run-workload~1]
    pub async fn run_workload(
        &mut self,
        workload_name: String,
        runtime_name: String,
        runtime_config: String,
        agent_name: String,
        tags_strings: Vec<(String, String)>,
    ) -> Result<(), CliError> {
        let tags: Vec<Tag> = tags_strings
            .into_iter()
            .map(|(k, v)| Tag { key: k, value: v })
            .collect();
        let new_workload = WorkloadSpec {
            runtime: runtime_name,
            name: workload_name.clone(),
            agent: agent_name,
            tags,
            runtime_config,
            ..Default::default()
        };
        output_debug!("Request to run new workload: {:?}", new_workload);

        // get current state
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: Vec::new(),
            })
            .await
            .map_err(|err| CliError::ExecutionError(err.to_string()))?;

        if let Some(ExecutionCommand::CompleteState(res)) = self.from_server.recv().await {
            output_debug!("Got current state: {:?}", res);
            let mut new_state = *res.clone();
            new_state
                .current_state
                .workloads
                .insert(workload_name, new_workload);

            let update_mask = vec!["currentState".to_string()];
            output_debug!("Sending the new state {:?}", new_state);
            self.to_server
                .update_state(new_state, update_mask)
                .await
                .map_err(|err| CliError::ExecutionError(err.to_string()))?;

            timeout(
                Duration::from_millis(self.response_timeout_ms),
                self.from_server.recv(),
            )
            .await
            .map_err(|_| CliError::ConnectionTimeout("No response from the server".to_string()))?;

            return Ok(());
        }

        Err(CliError::ExecutionError(
            "Failed to get complete state from server".to_string(),
        ))
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use std::{io, thread};

    use common::{
        commands,
        execution_interface::ExecutionCommand,
        objects::{Tag, WorkloadSpec},
        state_change_interface::{StateChangeCommand, StateChangeReceiver},
        test_utils::{self, generate_test_complete_state},
    };
    use tabled::{settings::Style, Table};
    use tokio::sync::mpsc::Sender;

    use crate::{
        cli::OutputFormat,
        cli_commands::{
            generate_compact_state_output, get_filtered_value, update_compact_state, WorkloadInfo,
        },
    };

    use super::CliCommands;

    use url::Url;

    const BUFFER_SIZE: usize = 20;
    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    const EXAMPLE_STATE_INPUT: &str = r#"{
        "currentState": {
            "workloads": {
                "nginx": {
                    "restart": true,
                    "agent": "agent_A"
                },
                "hello1": {
                    "agent": "agent_B"
                }
            }
        }
    }"#;

    mockall::lazy_static! {
        pub static ref FAKE_READ_TO_STRING_MOCK_RESULT_LIST: tokio::sync::Mutex<std::collections::VecDeque<io::Result<String>>>  =
        tokio::sync::Mutex::new(std::collections::VecDeque::new());
    }

    pub async fn read_to_string_mock(_file: String) -> io::Result<String> {
        FAKE_READ_TO_STRING_MOCK_RESULT_LIST
            .lock()
            .await
            .pop_front()
            .unwrap()
    }

    mockall::mock! {
        pub GRPCCommunicationsClient {
            pub fn new_cli_communication(name: String, server_address: Url) -> Self;
            pub async fn run(
                &mut self,
                mut server_rx: StateChangeReceiver,
                agent_tx: Sender<ExecutionCommand>,
            ) -> Result<(), String>;
        }
    }

    fn prepare_server_response(
        complete_states: Vec<ExecutionCommand>,
        to_cli: Sender<ExecutionCommand>,
    ) -> Result<(), String> {
        let sync_code = thread::spawn(move || {
            complete_states.into_iter().for_each(|cs| {
                to_cli.blocking_send(cs).unwrap();
            });
        });
        sync_code.join().unwrap();
        Ok(())
    }

    // [utest->swdd~cli-shall-print-empty-table~1]
    #[tokio::test]
    async fn get_workloads_empty_table() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let empty_complete_state = vec![ExecutionCommand::CompleteState(Box::new(
            test_utils::generate_test_complete_state("request_id".to_owned(), Vec::new()),
        ))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(empty_complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd.get_workloads(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_empty_table: Vec<WorkloadInfo> = Vec::new();
        let expected_table_text = Table::new(expected_empty_table)
            .with(Style::blank())
            .to_string();

        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-provides-list-of-workloads~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1]
    // [utest->swdd~cli-returns-list-of-workloads-from-server~1]
    // [utest->swdd~cli-shall-present-list-workloads-as-table~1]
    // [utest->swdd~cli-shall-sort-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_no_filtering() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(
            test_utils::generate_test_complete_state(
                "request_id".to_owned(),
                vec![
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ],
            ),
        ))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd.get_workloads(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<WorkloadInfo> = vec![
            WorkloadInfo {
                name: String::from("name1"),
                agent: String::from("agent_A"),
                runtime: String::from("runtime"),
                execution_state: String::from("Running"),
            },
            WorkloadInfo {
                name: String::from("name2"),
                agent: String::from("agent_B"),
                runtime: String::from("runtime"),
                execution_state: String::from("Running"),
            },
            WorkloadInfo {
                name: String::from("name3"),
                agent: String::from("agent_B"),
                runtime: String::from("runtime"),
                execution_state: String::from("Running"),
            },
        ];
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_workload_name() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(
            test_utils::generate_test_complete_state(
                "request_id".to_owned(),
                vec![
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ],
            ),
        ))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd
            .get_workloads(None, None, vec!["name1".to_string()])
            .await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<WorkloadInfo> = vec![WorkloadInfo {
            name: String::from("name1"),
            agent: String::from("agent_A"),
            runtime: String::from("runtime"),
            execution_state: String::from("Running"),
        }];
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_agent() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(
            test_utils::generate_test_complete_state(
                "request_id".to_owned(),
                vec![
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ],
            ),
        ))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd
            .get_workloads(Some("agent_B".to_string()), None, Vec::new())
            .await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<WorkloadInfo> = vec![
            WorkloadInfo {
                name: String::from("name2"),
                agent: String::from("agent_B"),
                runtime: String::from("runtime"),
                execution_state: String::from("Running"),
            },
            WorkloadInfo {
                name: String::from("name3"),
                agent: String::from("agent_B"),
                runtime: String::from("runtime"),
                execution_state: String::from("Running"),
            },
        ];
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(
            test_utils::generate_test_complete_state(
                "request_id".to_owned(),
                vec![
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_A".to_string(),
                        "name1".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name2".to_string(),
                        "runtime".to_string(),
                    ),
                    test_utils::generate_test_workload_spec_with_param(
                        "agent_B".to_string(),
                        "name3".to_string(),
                        "runtime".to_string(),
                    ),
                ],
            ),
        ))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd
            .get_workloads(None, Some("Failed".to_string()), Vec::new())
            .await;
        assert!(cmd_text.is_ok());

        let expected_table: Vec<WorkloadInfo> = Vec::new();
        let expected_table_text = Table::new(expected_table).with(Style::blank()).to_string();
        assert_eq!(cmd_text.unwrap(), expected_table_text);
    }

    // [utest->swdd~cli-provides-delete-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~1]
    #[tokio::test]
    async fn delete_workloads_two_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let startup_state = test_utils::generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "runtime".to_string(),
                ),
            ],
        );
        let updated_state = test_utils::generate_test_complete_state(
            "request_id".to_owned(),
            vec![test_utils::generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            )],
        );
        let complete_states = vec![
            ExecutionCommand::CompleteState(Box::new(startup_state)),
            ExecutionCommand::CompleteState(Box::new(updated_state.clone())),
        ];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_states, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );

        // replace the connection to the server with our own
        let (test_to_server, mut test_server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);
        cmd.to_server = test_to_server;

        let delete_result = cmd
            .delete_workloads(vec!["name1".to_string(), "name2".to_string()])
            .await;
        assert!(delete_result.is_ok());

        // The request to get workloads
        let message_to_server = test_server_receiver.try_recv();
        assert!(message_to_server.is_ok());

        // The request to update_state
        let message_to_server = test_server_receiver.try_recv();
        assert!(message_to_server.is_ok());
        assert_eq!(
            message_to_server.unwrap(),
            StateChangeCommand::UpdateState(commands::UpdateStateRequest {
                state: updated_state,
                update_mask: vec!["currentState".to_string()]
            },)
        );

        // Make sure that we have read all commands from the channel.
        assert!(test_server_receiver.try_recv().is_err());
    }

    // [utest->swdd~no-delete-workloads-when-not-found~1]
    #[tokio::test]
    async fn delete_workloads_unknown_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let startup_state = test_utils::generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "runtime".to_string(),
                ),
            ],
        );
        let updated_state = startup_state.clone();
        let complete_states = vec![
            ExecutionCommand::CompleteState(Box::new(startup_state)),
            ExecutionCommand::CompleteState(Box::new(updated_state.clone())),
        ];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_states, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );

        // replace the connection to the server with our own
        let (test_to_server, mut test_server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);
        cmd.to_server = test_to_server;

        let delete_result = cmd
            .delete_workloads(vec!["unknown_workload".to_string()])
            .await;
        assert!(delete_result.is_ok());

        // The request to get workloads
        let message_to_server = test_server_receiver.try_recv();
        assert!(message_to_server.is_ok());

        // Make sure that we have read all commands from the channel.
        assert!(test_server_receiver.try_recv().is_err());
    }

    // [utest -> swdd~cli-returns-current-state-from-server~1]
    // [utest -> swdd~cli-shall-support-current-state-yaml~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-get-current-state~1]
    // [utest->swdd~cli-provides-get-current-state~1]
    #[tokio::test]
    async fn get_state_complete_current_state_yaml() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "runtime".to_string(),
                ),
            ],
        );

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(test_data.clone()))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd.get_state(vec![], crate::cli::OutputFormat::Yaml).await;
        assert!(cmd_text.is_some());

        let expected_text = Some(serde_yaml::to_string(&test_data).unwrap());
        assert_eq!(cmd_text, expected_text);
    }

    // [utest -> swdd~cli-shall-support-current-state-json~1]
    #[tokio::test]
    async fn get_state_complete_current_state_json() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "runtime".to_string(),
                ),
            ],
        );

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(test_data.clone()))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd.get_state(vec![], crate::cli::OutputFormat::Json).await;
        assert!(cmd_text.is_some());

        let expected_text = Some(serde_json::to_string_pretty(&test_data).unwrap());
        assert_eq!(cmd_text, expected_text);
    }

    // [utest -> swdd~cli-returns-current-state-from-server~1]
    #[tokio::test]
    async fn get_state_single_field_of_current_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(
            "requestId".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "runtime".to_string(),
                ),
            ],
        );

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(test_data.clone()))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );
        let cmd_text = cmd
            .get_state(
                vec!["currentState.workloads.name3.runtime".to_owned()],
                crate::cli::OutputFormat::Yaml,
            )
            .await;
        assert!(cmd_text.is_some());

        let expected_single_field_result_text = Some(
            serde_yaml::to_string(&serde_json::json!(
                {"currentState": {"workloads": {"name3": { "runtime": "runtime"}}}}
            ))
            .unwrap(),
        );

        assert_eq!(cmd_text, expected_single_field_result_text);
    }

    // [utest->swdd~cli-provides-object-field-mask-arg-to-get-partial-current-state~1]
    // [utest->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
    #[tokio::test]
    async fn get_state_multiple_fields_of_current_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let test_data = test_utils::generate_test_complete_state(
            "requestId".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "runtime".to_string(),
                ),
            ],
        );

        let complete_state = vec![ExecutionCommand::CompleteState(Box::new(test_data.clone()))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_state, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );

        let cmd_text = cmd
            .get_state(
                vec![
                    "currentState.workloads.name1.runtime".to_owned(),
                    "currentState.workloads.name2.runtime".to_owned(),
                ],
                crate::cli::OutputFormat::Yaml,
            )
            .await;
        assert!(cmd_text.is_some());
        assert!(matches!(cmd_text, 
            Some(txt) if txt == *"currentState:\n  workloads:\n    name1:\n      runtime: runtime\n    name2:\n      runtime: runtime\n" || 
            txt == *"currentState:\n  workloads:\n    name2:\n      runtime: runtime\n    name1:\n      runtime: runtime\n"));
    }

    // [utest -> swdd~cli-provides-set-current-state~1]
    // [utest -> swdd~cli-supports-yaml-to-set-current-state~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-set-current-state~1]
    #[tokio::test]
    async fn set_state_update_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut updated_state = commands::CompleteState::default();
        updated_state.current_state.workloads.insert(
            "name3".to_owned(),
            WorkloadSpec {
                runtime: "new_runtime".to_owned(),
                ..Default::default()
            },
        );

        let complete_states = vec![ExecutionCommand::CompleteState(Box::new(
            updated_state.clone(),
        ))];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_states, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            3000,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );

        // replace the connection to the server with our own
        let (test_to_server, mut test_server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);
        cmd.to_server = test_to_server;

        FAKE_READ_TO_STRING_MOCK_RESULT_LIST
            .lock()
            .await
            .push_back(Ok(r#"
            currentState:
               workloads:
                  name3:
                    runtime: new_runtime
            "#
            .to_owned()));

        let update_mask = vec![
            "currentState".to_owned(),
            "workloads".to_owned(),
            "name3".to_owned(),
            "runtime".to_owned(),
        ];
        cmd.set_state(update_mask.clone(), Some("my_file".to_owned()), 30000)
            .await;

        // check update_state request generated by set_state command
        let message_to_server = test_server_receiver.try_recv();
        assert!(message_to_server.is_ok());
        assert_eq!(
            message_to_server.unwrap(),
            StateChangeCommand::UpdateState(commands::UpdateStateRequest {
                state: updated_state,
                update_mask
            },)
        );

        // Make sure that we have read all commands from the channel.
        assert!(test_server_receiver.try_recv().is_err());
    }

    // [utest->swdd~cli-provides-run-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-run-workload~1]
    #[tokio::test]
    async fn run_workload_one_new_workload() {
        let test_workload_name = "name4".to_string();
        let test_workload_agent = "agent_B".to_string();
        let test_workload_runtime_name = "runtime2".to_string();
        let test_workload_runtime_cfg = "some config".to_string();

        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let startup_state = test_utils::generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "runtime".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "runtime".to_string(),
                ),
            ],
        );

        // The "run workload" command shall add one new workload to the startup state.
        let new_workload = WorkloadSpec {
            runtime: test_workload_runtime_name.clone(),
            name: test_workload_name.clone(),
            agent: test_workload_agent.clone(),
            tags: vec![Tag {
                key: "key".to_string(),
                value: "value".to_string(),
            }],
            runtime_config: test_workload_runtime_cfg.clone(),
            ..Default::default()
        };
        let mut updated_state = startup_state.clone();
        updated_state
            .current_state
            .workloads
            .insert(test_workload_name.clone(), new_workload);
        let complete_states = vec![
            ExecutionCommand::CompleteState(Box::new(startup_state)),
            ExecutionCommand::CompleteState(Box::new(updated_state.clone())),
        ];

        let mut mock_client = MockGRPCCommunicationsClient::default();
        mock_client
            .expect_run()
            .return_once(|_r, to_cli| prepare_server_response(complete_states, to_cli));

        let mock_new = MockGRPCCommunicationsClient::new_cli_communication_context();
        mock_new
            .expect()
            .return_once(move |_name, _server_address| mock_client);

        let mut cmd = CliCommands::init(
            RESPONSE_TIMEOUT_MS,
            "TestCli".to_string(),
            Url::parse("http://localhost").unwrap(),
        );

        // replace the connection to the server with our own
        let (test_to_server, mut test_server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);
        cmd.to_server = test_to_server;

        let run_workload_result = cmd
            .run_workload(
                test_workload_name,
                test_workload_runtime_name,
                test_workload_runtime_cfg,
                test_workload_agent,
                vec![("key".to_string(), "value".to_string())],
            )
            .await;
        assert!(run_workload_result.is_ok());

        // request to get workloads
        let message_to_server = test_server_receiver.try_recv();
        assert!(message_to_server.is_ok());

        // request to update the current state
        let message_to_server = test_server_receiver.try_recv();
        assert!(message_to_server.is_ok());

        assert_eq!(
            message_to_server.unwrap(),
            StateChangeCommand::UpdateState(commands::UpdateStateRequest {
                state: updated_state,
                update_mask: vec!["currentState".to_string()]
            },)
        );

        // Make sure that we have read all commands from the channel.
        assert!(test_server_receiver.try_recv().is_err());
    }

    #[test]
    fn utest_generate_compact_state_output_empty_filter_masks() {
        let input_state = generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "podman".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "podman".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "podman".to_string(),
                ),
            ],
        );

        let cli_output =
            generate_compact_state_output(&input_state, vec![], OutputFormat::Yaml).unwrap();

        // state shall remain unchanged
        assert_eq!(cli_output, serde_yaml::to_string(&input_state).unwrap());
    }

    #[test]
    fn utest_generate_compact_state_output_single_filter_mask() {
        let input_state = generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "podman".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "podman".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "podman".to_string(),
                ),
            ],
        );

        let expected_state = r#"{
            "currentState": {
                "workloads": {
                    "name1": {
                    "agent": "agent_A",
                    "name": "name1",
                    "tags": [
                        {
                        "key": "key",
                        "value": "value"
                        }
                    ],
                    "dependencies": {
                        "workload A": "RUNNING",
                        "workload C": "STOPPED"
                    },
                    "updateStrategy": "UNSPECIFIED",
                    "restart": true,
                    "accessRights": {
                        "allow": [],
                        "deny": []
                    },
                    "runtime": "podman",
                    "runtimeConfig": "image: alpine:latest\ncommand: [\"echo\"]\nargs: [\"Hello Ankaios\"]"
                    }
                }
            }
        }"#;

        let cli_output = generate_compact_state_output(
            &input_state,
            vec!["currentState.workloads.name1".to_string()],
            OutputFormat::Yaml,
        )
        .unwrap();

        let expected_value: serde_yaml::Value = serde_yaml::from_str(expected_state).unwrap();

        assert_eq!(cli_output, serde_yaml::to_string(&expected_value).unwrap());
    }

    #[test]
    fn utest_generate_compact_state_output_multiple_filter_masks() {
        let input_state = generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                test_utils::generate_test_workload_spec_with_param(
                    "agent_A".to_string(),
                    "name1".to_string(),
                    "podman".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name2".to_string(),
                    "podman".to_string(),
                ),
                test_utils::generate_test_workload_spec_with_param(
                    "agent_B".to_string(),
                    "name3".to_string(),
                    "podman".to_string(),
                ),
            ],
        );

        let expected_state = r#"{
            "currentState": {
                "workloads": {
                    "name1": {
                        "agent": "agent_A",
                        "name": "name1",
                        "tags": [
                            {
                            "key": "key",
                            "value": "value"
                            }
                        ],
                        "dependencies": {
                            "workload A": "RUNNING",
                            "workload C": "STOPPED"
                        },
                        "updateStrategy": "UNSPECIFIED",
                        "restart": true,
                        "accessRights": {
                            "allow": [],
                            "deny": []
                        },
                        "runtime": "podman",
                        "runtimeConfig": "image: alpine:latest\ncommand: [\"echo\"]\nargs: [\"Hello Ankaios\"]"
                    },
                    "name2": {
                        "agent": "agent_B"
                    }
                }
            }
        }"#;

        let cli_output = generate_compact_state_output(
            &input_state,
            vec![
                "currentState.workloads.name1".to_string(),
                "currentState.workloads.name2.agent".to_string(),
            ],
            OutputFormat::Yaml,
        )
        .unwrap();

        let expected_value: serde_yaml::Value = serde_yaml::from_str(expected_state).unwrap();

        assert_eq!(cli_output, serde_yaml::to_string(&expected_value).unwrap());
    }

    #[test]
    fn utest_get_filtered_value_filter_key_with_mapping() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        let result =
            get_filtered_value(&deserialized_map, &["currentState", "workloads", "nginx"]).unwrap();
        assert_eq!(
            result.get("restart").unwrap(),
            &serde_yaml::Value::Bool(true)
        );
    }

    #[test]
    fn utest_get_filtered_value_filter_key_without_mapping() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        let result = get_filtered_value(
            &deserialized_map,
            &["currentState", "workloads", "nginx", "agent"],
        )
        .unwrap();
        let expected = serde_yaml::Value::String("agent_A".to_string());
        assert_eq!(result, &expected);
    }

    #[test]
    fn utest_get_filtered_value_empty_mask() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        let result = get_filtered_value(&deserialized_map, &[]).unwrap();
        assert!(result.get("currentState").is_some());
    }

    #[test]
    fn utest_get_filtered_value_not_existing_keys() {
        let deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();

        let result = get_filtered_value(
            &deserialized_map,
            &["currentState", "workloads", "notExistingWorkload", "nginx"],
        );
        assert!(result.is_none());

        let result = get_filtered_value(
            &deserialized_map,
            &[
                "currentState",
                "workloads",
                "notExistingWorkload",
                "notExistingField",
            ],
        );
        assert!(result.is_none());

        let result = get_filtered_value(
            &deserialized_map,
            &[
                "currentState",
                "workloads",
                "nginx",
                "agent",
                "notExistingField",
            ],
        );
        assert!(result.is_none());
    }

    #[test]
    fn utest_update_compact_state_create_two_keys() {
        let mut deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();

        // update by inserting two new nested keys and a new empty mapping as value
        update_compact_state(
            &mut deserialized_map,
            &[
                "currentState",
                "workloads",
                "createThisKey",
                "createThisKey",
            ],
            serde_yaml::Value::Mapping(Default::default()),
        );

        assert!(deserialized_map
            .get("currentState")
            .and_then(|next| next.get("workloads").and_then(|next| next
                .get("createThisKey")
                .and_then(|next| next.get("createThisKey"))))
            .is_some());
    }

    #[test]
    fn utest_update_compact_state_keep_value_of_existing_key() {
        let mut deserialized_map: serde_yaml::Value =
            serde_yaml::from_str(EXAMPLE_STATE_INPUT).unwrap();
        // do not update value of existing key
        update_compact_state(
            &mut deserialized_map,
            &[
                "currentState",
                "workloads",
                "nginx",
                "restart",
                "createThisKey",
            ],
            serde_yaml::Value::Mapping(Default::default()),
        );

        assert_eq!(
            deserialized_map
                .get("currentState")
                .and_then(|next| next
                    .get("workloads")
                    .and_then(|next| next.get("nginx").and_then(|next| next.get("restart"))))
                .unwrap(),
            &serde_yaml::Value::Bool(true)
        );
    }

    #[test]
    fn utest_update_compact_state_insert_into_empty_map() {
        // insert keys nested into empty map and add empty mapping as value
        let mut empty_map = serde_yaml::Value::Mapping(Default::default());
        update_compact_state(
            &mut empty_map,
            &["currentState", "workloads", "nginx"],
            serde_yaml::Value::Mapping(Default::default()),
        );

        assert!(empty_map
            .get("currentState")
            .and_then(|next| next.get("workloads").and_then(|next| next.get("nginx")))
            .is_some());
    }

    #[test]
    fn utest_update_compact_state_do_not_update_on_empty_mask() {
        let mut empty_map = serde_yaml::Value::Mapping(Default::default());
        empty_map.as_mapping_mut().unwrap().insert(
            "currentState".into(),
            serde_yaml::Value::Mapping(Default::default()),
        );
        let expected_map = empty_map.clone();

        // do not update map if no masks are provided
        update_compact_state(
            &mut empty_map,
            &[],
            serde_yaml::Value::Mapping(Default::default()),
        );
        assert_eq!(empty_map, expected_map);
    }
}

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

use std::{collections::HashMap, fmt, time::Duration};

#[cfg(not(test))]
async fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

use common::{
    commands::{CompleteState, RequestCompleteState},
    execution_interface::ExecutionCommand,
    objects::{RuntimeWorkload, Tag, WorkloadSpec},
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

use crate::cli::OutputFormat;

const BUFFER_SIZE: usize = 20;

#[derive(Debug, Clone)]
pub enum CliError {
    InvalidObjectFieldMask(String),
    YamlSerialization(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CliError::InvalidObjectFieldMask(message) => {
                write!(f, "Invalid object field mask: '{message}'")
            }
            CliError::YamlSerialization(message) => {
                write!(f, "Could not serialize YAML object: '{message}'")
            }
        }
    }
}

impl From<serde_yaml::Error> for CliError {
    fn from(value: serde_yaml::Error) -> Self {
        CliError::YamlSerialization(format!("{value}"))
    }
}

fn generate_compact_state_output(
    state: &CompleteState,
    object_field_mask: Vec<String>,
    output_format: OutputFormat,
) -> Result<String, CliError> {
    let mut top_level_map: serde_yaml::Value = serde_yaml::to_value(state)?;
    if !object_field_mask.is_empty() {
        let state_value = serde_yaml::to_value(state)?;
        let mut obj_map: HashMap<String, serde_yaml::Value> = HashMap::new();

        // build compact output for each object field mask
        for mask in object_field_mask.iter() {
            obj_map.insert(
                mask.to_string(),
                serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            );
            let mut cur_level_map = obj_map.get_mut(mask).unwrap();
            let mask_vec: Vec<&str> = mask.split('.').collect();
            let mask_vec_len = mask_vec.len();
            let mut cur_state_value: &serde_yaml::Value = &state_value;
            for (field_index, cur_field) in mask_vec.iter().enumerate() {
                if let Some(obj) = cur_state_value.get(cur_field) {
                    if field_index == mask_vec_len - 1 {
                        cur_level_map.as_mapping_mut().unwrap().insert(
                            serde_yaml::Value::String(cur_field.to_string()),
                            obj.clone(),
                        );
                    } else {
                        if cur_level_map.get(cur_field).is_none() {
                            cur_level_map.as_mapping_mut().unwrap().insert(
                                serde_yaml::Value::String(cur_field.to_string()),
                                serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
                            );
                        }

                        cur_level_map = cur_level_map.get_mut(cur_field).unwrap();
                        cur_state_value = cur_state_value.get(cur_field).unwrap()
                    }
                } else {
                    return Err(CliError::InvalidObjectFieldMask(mask.to_string()));
                }
            }
        }

        // merge the compact output for each object field mask to form single compact output
        top_level_map = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
        for (mask, obj) in obj_map {
            let mask_vec: Vec<&str> = mask.split('.').collect();
            let mut cur_obj = &obj;
            let mut cur_level_map = &mut top_level_map;
            for cur_field in mask_vec.iter() {
                let cur_level = cur_level_map.as_mapping_mut().unwrap();
                if cur_level.get(cur_field).is_none() {
                    cur_level.insert(
                        serde_yaml::Value::String(cur_field.to_string()),
                        cur_obj.get(cur_field).unwrap().to_owned(),
                    );
                    break;
                } else {
                    cur_level_map = cur_level_map.get_mut(cur_field).unwrap();
                    cur_obj = cur_obj.get(cur_field).unwrap();
                }
            }
        }
    }

    match output_format {
        // [impl -> swdd~cli-shall-support-current-state-yaml~1]
        OutputFormat::Yaml => Ok(serde_yaml::to_string(&top_level_map).unwrap()),
        // [impl -> swdd~cli-shall-support-current-state-json~1]
        OutputFormat::Json => Ok(serde_json::to_string_pretty(&top_level_map).unwrap()),
    }
}

// [impl->swdd~server-handle-cli-communication~1]
// [impl->swdd~cli-communication-over-middleware~1]
async fn setup_cli_communication(
    cli_name: &str,
    server_url: Url,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::Sender<StateChangeCommand>,
    tokio::sync::mpsc::Receiver<ExecutionCommand>,
) // (task,sender,receiver)
{
    let mut grps_communications_client =
        GRPCCommunicationsClient::new_cli_communication(cli_name.to_owned(), server_url);

    let (to_cli, cli_receiver) = tokio::sync::mpsc::channel::<ExecutionCommand>(BUFFER_SIZE);
    let (to_server, server_receiver) =
        tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    let communications_task = tokio::spawn(async move {
        grps_communications_client
            .run(server_receiver, to_cli.clone())
            .await
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
    pub async fn init(response_timeout_ms: u64, cli_name: String, server_url: Url) -> Self {
        let (task, to_server, from_server) =
            setup_cli_communication(cli_name.as_str(), server_url.clone()).await;
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
        log::info!(
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
            .await;

        if let Some(ExecutionCommand::CompleteState(res)) = self.from_server.recv().await {
            out_command_text =
                // [impl->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
                match generate_compact_state_output(&res, object_field_mask, output_format) {
                    Ok(res) => Some(res),
                    Err(err) => {
                        log::error!(
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
        log::info!(
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

        log::info!("Send UpdateState request ...");
        // send update request
        self.to_server
            .update_state(complete_state_input, object_field_mask)
            .await;
        if (timeout(
            Duration::from_millis(response_timeout_ms),
            self.from_server.recv(),
        )
        .await)
            .is_err()
        {
            log::info!("Ok");
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
    ) -> Option<String> {
        let mut out_command_text: Option<String> = None;

        // send request
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: Vec::new(),
            })
            .await;

        if let Some(ExecutionCommand::CompleteState(res)) = self.from_server.recv().await {
            let mut workload_infos: Vec<WorkloadInfo> = res
                .current_state
                .workloads
                .values()
                .cloned()
                .map(|w| WorkloadInfo {
                    name: w.workload.name,
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
            log::debug!("The table before filtering:\n{:?}", workload_infos);

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

            log::debug!("The table after filtering:\n{:?}", workload_infos);

            // [impl->swdd~cli-shall-present-list-workloads-as-table~1]
            out_command_text = Some(Table::new(workload_infos).with(Style::blank()).to_string());
        }
        // [impl->swdd~cli-returns-list-of-workloads-from-server~1]
        out_command_text
    }

    // [impl->swdd~cli-provides-delete-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~1]
    pub async fn delete_workloads(&mut self, workload_names: Vec<String>) {
        // get current state
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: Vec::new(),
            })
            .await;

        if let Some(ExecutionCommand::CompleteState(res)) = self.from_server.recv().await {
            log::debug!("Got current state: {:?}", res);
            let mut new_state = *res.clone();
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
            if new_state.current_state != res.current_state {
                log::debug!("Sending the new state {:?}", new_state);
                self.to_server.update_state(new_state, update_mask).await;

                if (timeout(
                    Duration::from_millis(self.response_timeout_ms),
                    self.from_server.recv(),
                )
                .await)
                    .is_err()
                {
                    log::warn!("No response from the server");
                } else {
                    log::info!("OK");
                }
            } else {
                // [impl->swdd~no-delete-workloads-when-not-found~1]
                log::debug!("Current and new states are identical -> nothing to do");
            }
        }
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
    ) {
        let tags: Vec<Tag> = tags_strings
            .into_iter()
            .map(|(k, v)| Tag { key: k, value: v })
            .collect();
        let new_workload = WorkloadSpec {
            agent: agent_name,
            runtime: runtime_name,
            workload: RuntimeWorkload {
                name: workload_name.clone(),
                tags,
                runtime_config,
                ..Default::default()
            },
            ..Default::default()
        };
        log::debug!("Request to run new workload: {:?}", new_workload);

        // get current state
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: Vec::new(),
            })
            .await;

        if let Some(ExecutionCommand::CompleteState(res)) = self.from_server.recv().await {
            log::debug!("Got current state: {:?}", res);
            let mut new_state = *res.clone();
            new_state
                .current_state
                .workloads
                .insert(workload_name, new_workload);

            let update_mask = vec!["currentState".to_string()];
            log::debug!("Sending the new state {:?}", new_state);
            self.to_server.update_state(new_state, update_mask).await;

            if (timeout(
                Duration::from_millis(self.response_timeout_ms),
                self.from_server.recv(),
            )
            .await)
                .is_err()
            {
                log::warn!("No response from the server");
            } else {
                log::info!("OK");
            }
        }
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
        objects::{RuntimeWorkload, Tag, WorkloadSpec},
        state_change_interface::{StateChangeCommand, StateChangeReceiver},
        test_utils,
    };
    use tabled::{settings::Style, Table};
    use tokio::sync::mpsc::Sender;

    use crate::cli_commands::WorkloadInfo;

    use super::CliCommands;

    use url::Url;

    const BUFFER_SIZE: usize = 20;
    const RESPONSE_TIMEOUT_MS: u64 = 3000;

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
            );
        }
    }

    fn prepare_server_response(
        complete_states: Vec<ExecutionCommand>,
        to_cli: Sender<ExecutionCommand>,
    ) {
        let sync_code = thread::spawn(move || {
            complete_states.into_iter().for_each(|cs| {
                to_cli.blocking_send(cs).unwrap();
            });
        });
        sync_code.join().unwrap()
    }

    // [utest->swdd~cli-shall-print-empty-table~1]
    #[tokio::test]
    async fn get_workloads_empty_table() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;
        let cmd_text = cmd.get_workloads(None, None, Vec::new()).await;
        assert!(cmd_text.is_some());

        let expected_empty_table: Vec<WorkloadInfo> = Vec::new();
        let expected_table_text = Some(
            Table::new(expected_empty_table)
                .with(Style::blank())
                .to_string(),
        );
        assert_eq!(cmd_text, expected_table_text);
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
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;
        let cmd_text = cmd.get_workloads(None, None, Vec::new()).await;
        assert!(cmd_text.is_some());

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
        let expected_table_text = Some(Table::new(expected_table).with(Style::blank()).to_string());
        assert_eq!(cmd_text, expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_workload_name() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;
        let cmd_text = cmd
            .get_workloads(None, None, vec!["name1".to_string()])
            .await;
        assert!(cmd_text.is_some());

        let expected_table: Vec<WorkloadInfo> = vec![WorkloadInfo {
            name: String::from("name1"),
            agent: String::from("agent_A"),
            runtime: String::from("runtime"),
            execution_state: String::from("Running"),
        }];
        let expected_table_text = Some(Table::new(expected_table).with(Style::blank()).to_string());
        assert_eq!(cmd_text, expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_agent() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;
        let cmd_text = cmd
            .get_workloads(Some("agent_B".to_string()), None, Vec::new())
            .await;
        assert!(cmd_text.is_some());

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
        let expected_table_text = Some(Table::new(expected_table).with(Style::blank()).to_string());
        assert_eq!(cmd_text, expected_table_text);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn get_workloads_filter_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;
        let cmd_text = cmd
            .get_workloads(None, Some("Failed".to_string()), Vec::new())
            .await;
        assert!(cmd_text.is_some());

        let expected_table: Vec<WorkloadInfo> = Vec::new();
        let expected_table_text = Some(Table::new(expected_table).with(Style::blank()).to_string());
        assert_eq!(cmd_text, expected_table_text);
    }

    // [utest->swdd~cli-provides-delete-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~1]
    #[tokio::test]
    async fn delete_workloads_two_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;

        // replace the connection to the server with our own
        let (test_to_server, mut test_server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);
        cmd.to_server = test_to_server;

        cmd.delete_workloads(vec!["name1".to_string(), "name2".to_string()])
            .await;

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
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;

        // replace the connection to the server with our own
        let (test_to_server, mut test_server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);
        cmd.to_server = test_to_server;

        cmd.delete_workloads(vec!["unknown_workload".to_string()])
            .await;

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
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;
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
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;
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
        let _ = env_logger::builder().is_test(true).try_init();

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

        let complete_state = vec![
            ExecutionCommand::CompleteState(Box::new(test_data.clone())),
        ];

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
        )
        .await;
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
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;

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
        let _ = env_logger::builder().is_test(true).try_init();

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
        )
        .await;

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
        let _ = env_logger::builder().is_test(true).try_init();

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
            agent: test_workload_agent.clone(),
            runtime: test_workload_runtime_name.clone(),
            workload: RuntimeWorkload {
                name: test_workload_name.clone(),
                tags: vec![Tag {
                    key: "key".to_string(),
                    value: "value".to_string(),
                }],
                runtime_config: test_workload_runtime_cfg.clone(),
                ..Default::default()
            },
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
        )
        .await;

        // replace the connection to the server with our own
        let (test_to_server, mut test_server_receiver) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);
        cmd.to_server = test_to_server;

        cmd.run_workload(
            test_workload_name,
            test_workload_runtime_name,
            test_workload_runtime_cfg,
            test_workload_agent,
            vec![("key".to_string(), "value".to_string())],
        )
        .await;

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
}

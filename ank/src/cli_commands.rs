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

use std::{
    collections::HashSet,
    default, fmt, fs, io,
    ops::{Add, Deref},
    time::Duration,
};

#[cfg(not(test))]
async fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

use common::{
    commands::{CompleteState, RequestCompleteState},
    execution_interface::ExecutionCommand,
    objects::{State, Tag, WorkloadSpec},
    state_change_interface::{StateChangeCommand, StateChangeInterface},
    state_manipulation::Object,
};

#[cfg(not(test))]
use common::communications_client::CommunicationsClient;
#[cfg(not(test))]
use grpc::client::GRPCCommunicationsClient;

#[cfg(test)]
use tests::MockGRPCCommunicationsClient as GRPCCommunicationsClient;

use tabled::{settings::Style, Table, Tabled};
use url::Url;

use crate::{
    cli::{ApplyArgs, OutputFormat},
    output_and_error, output_and_exit, output_debug,
};

const BUFFER_SIZE: usize = 20;
const WAIT_TIME_MS: Duration = Duration::from_millis(3000);

#[derive(Debug, Clone)]
pub enum CliError {
    YamlSerialization(String),
    JsonSerialization(String),
    ExecutionError(String),
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
    mask.iter()
        .try_fold(map, |current_level, mask_part| current_level.get(mask_part))
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

type InputSources = Result<Vec<(String, Box<dyn io::Read>)>, io::Error>;
impl ApplyArgs {
    pub fn get_input_sources(self) -> InputSources {
        if let Some(first_arg) = self.manifest_files.first() {
            match first_arg.as_str() {
                "-" => Ok(vec![("stdin".to_owned(), Box::new(io::stdin()))]),
                _ => {
                    let mut res: InputSources = Ok(vec![]);
                    for file_path in self.manifest_files {
                        match fs::File::open(&file_path) {
                            Ok(open_file) => {
                                res.as_mut().unwrap().push((file_path, Box::new(open_file)))
                            }
                            Err(err) => {
                                res = Err(err);
                                break;
                            }
                        }
                    }
                    res
                }
            }
        } else {
            Ok(vec![])
        }
    }
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
    // Left here for the future use.
    _response_timeout_ms: u64,
    cli_name: String,
    task: tokio::task::JoinHandle<()>,
    to_server: tokio::sync::mpsc::Sender<StateChangeCommand>,
    from_server: tokio::sync::mpsc::Receiver<ExecutionCommand>,
}

impl CliCommands {
    pub fn init(response_timeout_ms: u64, cli_name: String, server_url: Url) -> Self {
        let (task, to_server, from_server) =
            setup_cli_communication(cli_name.as_str(), server_url.clone());
        Self {
            _response_timeout_ms: response_timeout_ms,
            cli_name,
            task,
            to_server,
            from_server,
        }
    }

    pub async fn shut_down(self) {
        drop(self.to_server);

        let _ = self.task.await;
    }

    async fn get_complete_state(
        &mut self,
        object_field_mask: &Vec<String>,
    ) -> Result<Box<CompleteState>, CliError> {
        output_debug!(
            "get_complete_state: object_field_mask={:?} ",
            object_field_mask
        );

        // send complete state request to server
        self.to_server
            .request_complete_state(RequestCompleteState {
                request_id: self.cli_name.to_owned(),
                field_mask: object_field_mask.clone(),
            })
            .await
            .map_err(|err| CliError::ExecutionError(err.to_string()))?;

        let poll_complete_state_response = async {
            loop {
                match self.from_server.recv().await {
                    Some(ExecutionCommand::CompleteState(res)) => return Ok(res),
                    None => return Err("Channel preliminary closed."),
                    Some(_) => (),
                }
            }
        };
        match tokio::time::timeout(WAIT_TIME_MS, poll_complete_state_response).await {
            Ok(Ok(res)) => Ok(res),
            Ok(Err(err)) => Err(CliError::ExecutionError(format!(
                "Failed to get complete state.\nError: {err}"
            ))),
            Err(_) => Err(CliError::ExecutionError(format!(
                "Failed to get complete state in time (timeout={WAIT_TIME_MS:?})."
            ))),
        }
    }

    pub async fn get_state(
        &mut self,
        object_field_mask: Vec<String>,
        output_format: OutputFormat,
    ) -> Result<String, CliError> {
        output_debug!(
            "Got: object_field_mask={:?} output_format={:?}",
            object_field_mask,
            output_format
        );

        let res_complete_state = self.get_complete_state(&object_field_mask).await?;
        // [impl->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
        match generate_compact_state_output(&res_complete_state, object_field_mask, output_format) {
            Ok(res) => Ok(res),
            Err(err) => {
                output_and_error!(
                    "Error occurred during processing response from server.\nError: {err}"
                );
                Err(err)
            }
        }
    }

    pub async fn set_state(
        &mut self,
        object_field_mask: Vec<String>,
        state_object_file: Option<String>,
    ) {
        output_debug!(
            "Got: object_field_mask={:?} state_object_file={:?}",
            object_field_mask,
            state_object_file
        );
        let mut complete_state_input = CompleteState::default();
        if let Some(state_object_file) = state_object_file {
            let state_object_data =
                read_file_to_string(state_object_file)
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
        // [impl->swdd~cli-returns-list-of-workloads-from-server~1]
        let res_complete_state = self.get_complete_state(&Vec::new()).await?;

        let mut workload_infos: Vec<WorkloadInfo> = res_complete_state
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
            if let Some(ws) = res_complete_state
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
        Ok(Table::new(workload_infos).with(Style::blank()).to_string())
    }

    // [impl->swdd~cli-provides-delete-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~1]
    pub async fn delete_workloads(&mut self, workload_names: Vec<String>) -> Result<(), CliError> {
        let complete_state = self.get_complete_state(&Vec::new()).await?;

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

        let res_complete_state = self.get_complete_state(&Vec::new()).await?;
        output_debug!("Got current state: {:?}", res_complete_state);
        let mut new_state = *res_complete_state.clone();
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
        Ok(())
    }

    // [impl->swdd~cli-provides-apply-multiple-ankaios-manifests~1]
    pub async fn apply_manifests(&mut self, apply_args: ApplyArgs) -> Result<(), CliError> {
        let mut req_obj: Object = State::default().try_into().unwrap();
        let mut req_paths: Vec<common::state_manipulation::Path> = Vec::new();
        let mut console_output = String::default();
        let line_len = apply_args.manifest_files.iter().max().unwrap().len() + 2;
        let plot_line = |console_output: &mut String, end_str: &str| {
            for _ in 1..line_len {
                console_output.push('-');
            }
            console_output.push_str(end_str);
        };
        console_output.push_str("Applying manifest files overview:\n");
        plot_line(&mut console_output, "\n");
        apply_args.manifest_files.iter().for_each(|file| {
            console_output.push_str(&format!("{}\n", if "-" != file { file } else { "stdin" }))
        });

        plot_line(&mut console_output, "\n");
        let delete_mode = apply_args.delete_mode;
        match apply_args.get_input_sources() {
            Ok(mut manifests) => {
                for manifest in manifests.iter_mut() {
                    let mut data = "".to_owned();
                    let _ = manifest.1.read_to_string(&mut data);
                    if data.is_empty() {
                        output_and_exit!("Empty manifest provided -> nothing to do!");
                    }
                    let yaml_nodes: serde_yaml::Value =
                        serde_yaml::from_str(&data).unwrap_or_else(|error| {
                            let error_out = format!(
                                "Error while parsing the state object data.\nError: {error}"
                            );
                            output_and_error!("{}", error_out);
                            panic!("{}", error_out)
                        });

                    let cur_obj: Object = Object::try_from(&yaml_nodes).unwrap();
                    let paths =
                        common::state_manipulation::get_paths_from_yaml_node(&yaml_nodes, false);

                    let mut workload_paths: HashSet<common::state_manipulation::Path> =
                        HashSet::new();
                    for path in paths {
                        let parts = path.parts();
                        let _ = &mut workload_paths.insert(common::state_manipulation::Path::from(
                            format!("{}.{}", parts[0], parts[1]),
                        ));

                        req_paths.push(path);
                    }

                    let info_collecting = if delete_mode {
                        format!(
                            "Collecting manifest for deletion: '{}' - contained workloads: {{",
                            manifest.0
                        )
                    } else {
                        format!(
                            "Collecting manifest: '{}' - contained workloads: {{",
                            manifest.0
                        )
                    };

                    console_output.push_str(&info_collecting);

                    let workload_path_len = workload_paths.len();
                    for (index, workload_path) in workload_paths.iter().enumerate() {
                        let workload_name = &workload_path.parts()[1];
                        if req_obj.get(workload_path).is_none() {
                            if index == workload_path_len - 1 {
                                console_output.push_str(&format!(" '{}'", workload_name));
                            } else {
                                console_output.push_str(&format!(" '{}',", workload_name));
                            }
                            let _ = req_obj
                                .set(workload_path, cur_obj.get(workload_path).unwrap().clone());
                        } else {
                            let error_str = format!("Error: Multiple workloads with the same name '{}' found!! }} - NOK",
                            workload_name);
                            // out.push_str(&format!("{}\n", &error_str));
                            output_and_error!("\n{}\n{}", console_output, error_str);
                            return Err(CliError::ExecutionError(error_str));
                        }
                    }
                    console_output.push_str(" }} - OK\n");
                }

                // println!("\nreq_obj: {:?}\n", req_obj);
                let update_state_req_obj: State = req_obj.try_into().unwrap();
                let complete_state_req_obj = common::commands::CompleteState {
                    request_id: self.cli_name.to_owned(),
                    current_state: update_state_req_obj,
                    ..Default::default()
                };
                let mut filter_masks = req_paths
                    .into_iter()
                    .map(|path| format!("currentState.{}.{}", path.parts()[0], path.parts()[1]))
                    .collect::<Vec<String>>();
                filter_masks.sort();
                filter_masks.dedup();

                output_debug!("\n{:?}\n{:?}", complete_state_req_obj, filter_masks);

                console_output.push_str("Applying collected manifests...");
                if delete_mode {
                    self.to_server
                        .update_state(CompleteState::default(), filter_masks)
                        .await
                        .map_err(|err| CliError::ExecutionError(err.to_string()))?;
                } else {
                    self.to_server
                        .update_state(complete_state_req_obj, filter_masks)
                        .await
                        .map_err(|err| CliError::ExecutionError(err.to_string()))?;
                }
                console_output.push_str("Done.");
                output_and_exit!("{}", console_output);
                Ok(())
            }
            Err(err) => Err(CliError::ExecutionError(err.to_string())),
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
    use std::{collections::HashMap, io, thread};

    use common::{
        commands,
        execution_interface::ExecutionCommand,
        objects::{Tag, UpdateStrategy, WorkloadSpec},
        state_change_interface::{StateChangeCommand, StateChangeReceiver},
        test_utils::{self, generate_test_complete_state},
    };
    use tabled::{settings::Style, Table};
    use tokio::sync::mpsc::Sender;

    use crate::{
        cli::OutputFormat,
        cli_commands::{
            generate_compact_state_output, get_filtered_value, merge_state_objects,
            update_compact_state, WorkloadInfo,
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
        let cmd_text = cmd
            .get_state(vec![], crate::cli::OutputFormat::Yaml)
            .await
            .unwrap();
        let expected_text = serde_yaml::to_string(&test_data).unwrap();
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
        let cmd_text = cmd
            .get_state(vec![], crate::cli::OutputFormat::Json)
            .await
            .unwrap();

        let expected_text = serde_json::to_string_pretty(&test_data).unwrap();
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
            .await
            .unwrap();

        let expected_single_field_result_text = serde_yaml::to_string(&serde_json::json!(
            {"currentState": {"workloads": {"name3": { "runtime": "runtime"}}}}
        ))
        .unwrap();

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
            .await
            .unwrap();
        assert!(matches!(cmd_text,
            txt if txt == *"currentState:\n  workloads:\n    name1:\n      runtime: runtime\n    name2:\n      runtime: runtime\n" ||
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
        cmd.set_state(update_mask.clone(), Some("my_file".to_owned()))
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
                        "workload A": "ADD_COND_RUNNING",
                        "workload C": "ADD_COND_SUCCEEDED"
                    },
                    "updateStrategy": "UNSPECIFIED",
                    "restart": true,
                    "accessRights": {
                        "allow": [],
                        "deny": []
                    },
                    "runtime": "podman",
                    "runtimeConfig": "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
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
                            "workload A": "ADD_COND_RUNNING",
                            "workload C": "ADD_COND_SUCCEEDED"
                        },
                        "updateStrategy": "UNSPECIFIED",
                        "restart": true,
                        "accessRights": {
                            "allow": [],
                            "deny": []
                        },
                        "runtime": "podman",
                        "runtimeConfig": "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
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

    #[test]
    fn utest_merge_state_objects_ok() {
        use common::objects::State;
        let mut test_state = State::default();
        let test_state_clone = test_state.clone();

        let mut workload_spec1: HashMap<String, WorkloadSpec> = HashMap::new();
        workload_spec1.insert(
            "hello".to_string(),
            WorkloadSpec {
                name: "wl1".to_string(),
                agent: "agent1".to_string(),
                tags: vec![],
                dependencies: HashMap::default(),
                update_strategy: UpdateStrategy::Unspecified,
                restart: true,
                access_rights: common::objects::AccessRights::default(),
                runtime: "".to_string(),
                runtime_config: "".to_string(),
            },
        );
        let other_states = vec![
            common::objects::State {
                workloads: workload_spec1,
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            State::default(),
        ];
        let _ = merge_state_objects(&other_states, &mut test_state);
        assert_ne!(test_state_clone, test_state);
    }
}

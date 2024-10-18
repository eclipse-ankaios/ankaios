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
    collections::{BTreeMap, HashMap, HashSet},
    time::Duration,
};
pub mod server_connection;
mod wait_list;
use grpc::security::TLSConfig;
mod cli_table;
use tokio::time::interval;
use wait_list::WaitList;
mod workload_table_row;
use workload_table_row::WorkloadTableRow;
mod agent_table_row;
mod wait_list_display;

// CLI commands implemented in another files
mod apply_manifests;
mod delete_workloads;
mod get_agents;
mod get_state;
mod get_workloads;
mod run_workload;
mod set_state;

use common::{
    communications_error::CommunicationMiddlewareError,
    from_server_interface::FromServer,
    objects::{CompleteState, State, WorkloadInstanceName, WorkloadState, WorkloadStatesMap},
};

use wait_list_display::WaitListDisplay;

#[cfg_attr(test, mockall_double::double)]
use self::server_connection::ServerConnection;
use crate::{
    cli_commands::wait_list::ParsedUpdateStateSuccess,
    cli_error::CliError,
    filtered_complete_state::{FilteredCompleteState, FilteredWorkloadSpec},
    output, output_debug,
};

#[cfg(test)]
use self::tests::open_manifest_mock as open_manifest;

#[cfg(not(test))]
fn open_manifest(
    file_path: &str,
) -> std::io::Result<(String, Box<dyn std::io::Read + Send + Sync + 'static>)> {
    use std::fs::File;
    match File::open(file_path) {
        Ok(open_file) => Ok((file_path.to_owned(), Box::new(open_file))),
        Err(err) => Err(err),
    }
}

pub fn get_input_sources(manifest_files: &[String]) -> Result<Vec<InputSourcePair>, String> {
    if let Some(first_arg) = manifest_files.first() {
        match first_arg.as_str() {
            // [impl->swdd~cli-apply-accepts-ankaios-manifest-content-from-stdin~1]
            "-" => Ok(vec![("stdin".to_owned(), Box::new(std::io::stdin()))]),
            // [impl->swdd~cli-apply-accepts-list-of-ankaios-manifests~1]
            _ => {
                let mut res: Vec<InputSourcePair> = vec![];
                for file_path in manifest_files.iter() {
                    match open_manifest(file_path) {
                        Ok(open_file) => res.push(open_file),
                        Err(err) => {
                            return Err(match err.kind() {
                                std::io::ErrorKind::NotFound => {
                                    format!("File '{}' not found!", file_path)
                                }
                                _ => err.to_string(),
                            });
                        }
                    }
                }
                Ok(res)
            }
        }
    } else {
        Ok(vec![])
    }
}

pub type InputSourcePair = (String, Box<dyn std::io::Read + Send + Sync + 'static>);

#[derive(Debug)]
pub struct WorkloadInfos(Vec<(WorkloadInstanceName, WorkloadTableRow)>);

impl WorkloadInfos {
    pub fn get_mut(&mut self) -> &mut Vec<(WorkloadInstanceName, WorkloadTableRow)> {
        &mut self.0
    }
}

impl IntoIterator for WorkloadInfos {
    type Item = (WorkloadInstanceName, WorkloadTableRow);
    type IntoIter = <Vec<(WorkloadInstanceName, WorkloadTableRow)> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<WorkloadStatesMap> for WorkloadInfos {
    fn from(workload_states_map: WorkloadStatesMap) -> Self {
        WorkloadInfos(
            // invoking this from is cheaper then repeating the code to flatten the wl state map
            Vec::<WorkloadState>::from(workload_states_map)
                .into_iter()
                .map(|wl_state| {
                    (
                        wl_state.instance_name.clone(),
                        WorkloadTableRow::new(
                            wl_state.instance_name.workload_name(),
                            wl_state.instance_name.agent_name(),
                            String::default(),
                            wl_state.execution_state.state.to_string(),
                            wl_state.execution_state.additional_info.to_string(),
                        ),
                    )
                })
                .collect(),
        )
    }
}

// The CLI commands are implemented in the modules included above. The rest are the common function.
pub struct CliCommands {
    // Left here for the future use.
    _response_timeout_ms: u64,
    no_wait: bool,
    server_connection: ServerConnection,
}

impl CliCommands {
    pub fn init(
        response_timeout_ms: u64,
        cli_name: String,
        server_url: String,
        no_wait: bool,
        tls_config: Option<TLSConfig>,
    ) -> Result<Self, CommunicationMiddlewareError> {
        Ok(Self {
            _response_timeout_ms: response_timeout_ms,
            no_wait,
            server_connection: ServerConnection::new(
                cli_name.as_str(),
                server_url.clone(),
                tls_config,
            )?,
        })
    }

    pub async fn shut_down(self) {
        self.server_connection.shut_down().await
    }

    // [impl->swdd~processes-complete-state-to-list-workloads~1]
    async fn get_workloads(&mut self) -> Result<WorkloadInfos, CliError> {
        let res_complete_state = self
            .server_connection
            .get_complete_state(&Vec::new())
            .await?;

        Ok(self.transform_into_workload_infos(res_complete_state))
    }

    // [impl->swdd~processes-complete-state-to-list-workloads~1]
    fn transform_into_workload_infos(
        &self,
        complete_state: FilteredCompleteState,
    ) -> WorkloadInfos {
        let workload_states_map = complete_state.workload_states.unwrap_or_default();
        let workload_infos = WorkloadInfos::from(workload_states_map);

        let desired_state_workloads = complete_state
            .desired_state
            .and_then(|desired_state| desired_state.workloads)
            .unwrap_or_default();

        self.add_runtime_name_to_workload_infos(workload_infos, desired_state_workloads)
    }

    // [impl->swdd~processes-complete-state-to-list-workloads~1]
    fn add_runtime_name_to_workload_infos(
        &self,
        mut workload_infos: WorkloadInfos,
        workloads: HashMap<String, FilteredWorkloadSpec>,
    ) -> WorkloadInfos {
        for (_, table_row) in workload_infos.get_mut() {
            let runtime_name = workloads
                .iter()
                .find(|&(wl_name, wl_spec)| {
                    *wl_name == table_row.name
                        && wl_spec
                            .agent
                            .as_deref()
                            .map_or(false, |x| x == table_row.agent)
                        && wl_spec.runtime.as_ref().is_some()
                })
                // runtime is valid because the filter above has found one
                .map(|(_, found_wl_spec)| found_wl_spec.runtime.as_ref().unwrap());

            if let Some(runtime) = runtime_name {
                table_row.runtime.clone_from(runtime);
            }
        }
        workload_infos
    }

    // [impl->swdd~cli-requests-update-state-with-watch~2]
    async fn update_state_and_wait_for_complete(
        &mut self,
        new_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<(), CliError> {
        /* to keep track of deleted not initially started workloads in the wait mode
        the current workloads before the update must be stored in an ordered map. Affects only user output.
        The updated state is created directly, independent of fetching the current workloads. */
        let current_workload_infos: BTreeMap<WorkloadInstanceName, WorkloadTableRow> =
            self.get_workloads().await?.into_iter().collect();

        let update_state_success = self
            .server_connection
            .update_state(new_state, update_mask)
            .await?;

        output_debug!("Got update success: {:?}", update_state_success);

        // [impl->swdd~cli-requests-update-state-with-watch-error~1]
        let update_state_success = ParsedUpdateStateSuccess::try_from(update_state_success)
            .map_err(|error| {
                CliError::ExecutionError(format!(
                    "Could not parse UpdateStateSuccess message: {error}"
                ))
            })?;

        if self.no_wait {
            Ok(())
        } else {
            // [impl->swdd~cli-requests-update-state-with-watch-success~1]
            self.wait_for_complete(update_state_success, current_workload_infos)
                .await
        }
    }

    // [impl->swdd~cli-watches-workloads~1]
    async fn wait_for_complete(
        &mut self,
        update_state_success: ParsedUpdateStateSuccess,
        mut previous_workload_infos: BTreeMap<WorkloadInstanceName, WorkloadTableRow>,
    ) -> Result<(), CliError> {
        output_debug!("updated state success: {:?}", update_state_success);

        let mut changed_workloads =
            HashSet::from_iter(update_state_success.added_workloads.iter().cloned());
        changed_workloads.extend(update_state_success.deleted_workloads.iter().cloned());

        if changed_workloads.is_empty() {
            output!("No workloads to update");
            return Ok(());
        } else {
            output!("Successfully applied the manifest(s).\nWaiting for workload(s) to reach desired states (press Ctrl+C to interrupt).\n");
        }

        let field_mask_whole_complete_state = Vec::new();
        let mut new_complete_state = self
            .server_connection
            .get_complete_state(&field_mask_whole_complete_state)
            .await?;

        let connected_agents: HashSet<String> = new_complete_state
            .agents
            .take()
            .and_then(|agents| agents.agents)
            .unwrap_or_default()
            .into_keys()
            .collect();

        let new_workload_infos = self.transform_into_workload_infos(new_complete_state);

        // new workloads were added through an updated state or the previous ones might already have reached the next state
        previous_workload_infos.extend(new_workload_infos.into_iter());

        let changed_workload_infos = previous_workload_infos
            .into_iter()
            .filter(|(instance_name, _)| changed_workloads.contains(instance_name))
            .collect::<Vec<_>>();

        let mut wait_list = WaitList::new(
            update_state_success,
            connected_agents,
            WaitListDisplay {
                data: changed_workload_infos.into_iter().collect(),
                spinner: Default::default(),
                not_completed: changed_workloads,
            },
        );

        let missed_workload_states = self
            .server_connection
            .take_missed_from_server_messages()
            .into_iter()
            .filter_map(|m| {
                if let FromServer::UpdateWorkloadState(u) = m {
                    Some(u)
                } else {
                    None
                }
            })
            .flat_map(|u| u.workload_states);

        output_debug!(
            "Got update workload state before waiting: {:?}",
            missed_workload_states
        );

        wait_list.update(missed_workload_states);

        let mut spinner_interval = interval(Duration::from_millis(100));

        while !wait_list.is_empty() {
            tokio::select! {
                update_workload_state = self.server_connection.read_next_update_workload_state() => {
                    let update_workload_state = update_workload_state?;
                    output_debug!("Got update workload state: {:?}", update_workload_state);
                    wait_list.update(update_workload_state.workload_states);
                }
                _ = spinner_interval.tick() => {
                    wait_list.step_spinner();
                }
            }
        }
        Ok(())
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
    use common::{from_server_interface::FromServerSender, to_server_interface::ToServerReceiver};
    use grpc::security::TLSConfig;

    use std::io;

    use super::{get_input_sources, InputSourcePair};

    mockall::lazy_static! {
        pub static ref FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST: std::sync::Mutex<std::collections::VecDeque<io::Result<InputSourcePair>>>  =
        std::sync::Mutex::new(std::collections::VecDeque::new());
    }

    mockall::mock! {
        pub GRPCCommunicationsClient {
            pub fn new_cli_communication(name: String, server_address: String, tls_config: Option<TLSConfig>) -> Self;
            pub async fn run(
                &mut self,
                mut server_rx: ToServerReceiver,
                agent_tx: FromServerSender,
            ) -> Result<(), String>;
        }
    }

    pub fn open_manifest_mock(
        _file_path: &str,
    ) -> io::Result<(String, Box<dyn io::Read + Send + Sync + 'static>)> {
        FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .pop_front()
            .unwrap()
    }

    #[tokio::test]
    async fn utest_apply_args_get_input_sources_manifest_files_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let _dummy_content = io::Cursor::new(b"manifest content");
        FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
            .lock()
            .unwrap()
            .push_back(Err(io::Error::other(
                "Some error occurred during open the manifest file!",
            )));

        assert!(
            get_input_sources(&["manifest1.yml".to_owned()]).is_err(),
            "Expected an error"
        );
    }

    // [utest->swdd~cli-apply-accepts-ankaios-manifest-content-from-stdin~1]
    #[test]
    fn utest_apply_args_get_input_sources_valid_manifest_stdin() {
        let expected = vec!["stdin".to_owned()];
        let actual = get_input_sources(&["-".to_owned()]).unwrap();

        let get_file_name = |item: &InputSourcePair| -> String { item.0.to_owned() };
        assert_eq!(
            expected,
            actual.iter().map(get_file_name).collect::<Vec<String>>()
        )
    }

    // [utest->swdd~cli-apply-accepts-list-of-ankaios-manifests~1]
    #[tokio::test]
    async fn utest_apply_args_get_input_sources_manifest_files_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let _dummy_content = io::Cursor::new(b"manifest content");
        for i in 1..3 {
            FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST
                .lock()
                .unwrap()
                .push_back(Ok((
                    format!("manifest{i}.yml"),
                    Box::new(_dummy_content.clone()),
                )));
        }

        let expected = vec!["manifest1.yml".to_owned(), "manifest2.yml".to_owned()];
        let actual = get_input_sources(&expected).unwrap();

        let get_file_name = |item: &InputSourcePair| -> String { item.0.to_owned() };
        assert_eq!(
            expected,
            actual.iter().map(get_file_name).collect::<Vec<String>>()
        )
    }
}

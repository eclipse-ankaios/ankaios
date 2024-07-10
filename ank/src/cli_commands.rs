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

use std::{collections::HashSet, time::Duration};
pub mod server_connection;
mod wait_list;
mod workload_table;
use tokio::time::interval;
use wait_list::WaitList;
mod workload_table_row;
use workload_table_row::WorkloadTableRow;
mod wait_list_display;

// CLI commands implemented in another files
mod apply_manifests;
mod delete_workloads;
mod get_state;
mod get_workloads;
mod run_workload;
mod set_state;

use common::{
    communications_error::CommunicationMiddlewareError,
    from_server_interface::FromServer,
    objects::{CompleteState, State, WorkloadInstanceName},
};

use wait_list_display::WaitListDisplay;

#[cfg_attr(test, mockall_double::double)]
use self::server_connection::ServerConnection;
use crate::{
    cli_commands::wait_list::ParsedUpdateStateSuccess, cli_error::CliError, output, output_debug,
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
    ) -> Result<Self, CommunicationMiddlewareError> {
        Ok(Self {
            _response_timeout_ms: response_timeout_ms,
            no_wait,
            server_connection: ServerConnection::new(cli_name.as_str(), server_url.clone())?,
        })
    }

    pub async fn shut_down(self) {
        self.server_connection.shut_down().await
    }

    async fn get_workloads(
        &mut self,
    ) -> Result<Vec<(WorkloadInstanceName, WorkloadTableRow)>, CliError> {
        let res_complete_state = self
            .server_connection
            .get_complete_state(&Vec::new())
            .await?;

        let mut workload_infos: Vec<(WorkloadInstanceName, WorkloadTableRow)> = res_complete_state
            .workload_states
            .into_iter()
            .map(|wl_state| {
                (
                    wl_state.instance_name.clone(),
                    WorkloadTableRow::new(
                        wl_state.instance_name.workload_name(),
                        wl_state.instance_name.agent_name(),
                        Default::default(),
                        &wl_state.execution_state.state.to_string(),
                        &wl_state.execution_state.additional_info.to_string(),
                    ),
                )
            })
            .collect();

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        for wi in &mut workload_infos {
            if let Some((_found_wl_name, found_wl_spec)) = res_complete_state
                .desired_state
                .workloads
                .iter()
                .find(|&(wl_name, wl_spec)| *wl_name == wi.1.name && wl_spec.agent == wi.1.agent)
            {
                wi.1.runtime = found_wl_spec.runtime.clone();
            }
        }

        Ok(workload_infos)
    }

    // [impl->swdd~cli-requests-update-state-with-watch~1]
    async fn update_state_and_wait_for_complete(
        &mut self,
        new_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<(), CliError> {
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
            self.wait_for_complete(update_state_success).await
        }
    }

    // [impl->swdd~cli-watches-workloads~1]
    async fn wait_for_complete(
        &mut self,
        update_state_success: ParsedUpdateStateSuccess,
    ) -> Result<(), CliError> {
        let mut changed_workloads =
            HashSet::from_iter(update_state_success.added_workloads.iter().cloned());
        changed_workloads.extend(update_state_success.deleted_workloads.iter().cloned());

        if changed_workloads.is_empty() {
            output!("No workloads to update");
            return Ok(());
        } else {
            output!("Successfully applied the manifest(s).\nWaiting for workload(s) to reach desired states (press Ctrl+C to interrupt).\n");
        }

        let states_of_all_workloads = self.get_workloads().await.unwrap();
        let states_of_changed_workloads = states_of_all_workloads
            .into_iter()
            .filter(|x| changed_workloads.contains(&x.0))
            .collect::<Vec<_>>();

        let mut wait_list = WaitList::new(
            update_state_success,
            WaitListDisplay {
                data: states_of_changed_workloads.into_iter().collect(),
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
    use super::{get_input_sources, InputSourcePair};
    use common::{
        commands::{UpdateStateSuccess, UpdateWorkloadState},
        from_server_interface::FromServer,
        objects::{
            generate_test_workload_spec_with_param, CompleteState, ExecutionState, WorkloadState,
        },
        test_utils::generate_test_complete_state,
    };
    use std::io;

    use crate::{
        cli_commands::server_connection::{MockServerConnection, ServerConnectionError},
        cli_error::CliError,
    };

    mockall::lazy_static! {
        pub static ref FAKE_OPEN_MANIFEST_MOCK_RESULT_LIST: std::sync::Mutex<std::collections::VecDeque<io::Result<InputSourcePair>>>  =
        std::sync::Mutex::new(std::collections::VecDeque::new());
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

    // [utest->swdd~cli-requests-update-state-with-watch~1]
    // [utest->swdd~cli-requests-update-state-with-watch-success~1]
    #[tokio::test]
    async fn utest_update_state_and_wait_for_complete_success() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_workload = generate_test_workload_spec_with_param(
            "agent_A".to_string(),
            "name1".to_string(),
            "runtime".to_string(),
        );
        let new_workload_instance_name = new_workload.instance_name.clone();
        let starting_workload_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                instance_name: new_workload_instance_name.clone(),
                execution_state: ExecutionState::starting_triggered(),
            }],
        };

        let running_workload_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                instance_name: new_workload_instance_name.clone(),
                execution_state: ExecutionState::running(),
            }],
        };

        let new_complete_state = generate_test_complete_state(vec![new_workload]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .once()
            .returning(move |_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![new_workload_instance_name.to_string()],
                    ..Default::default()
                })
            });

        let server_reply_complete_state = new_complete_state.clone();
        mock_server_connection
            .expect_get_complete_state()
            .once()
            .return_once(|_| Ok(Box::new(server_reply_complete_state)));

        mock_server_connection
            .expect_take_missed_from_server_messages()
            .once()
            .return_once(move || vec![FromServer::UpdateWorkloadState(starting_workload_state)]);

        mock_server_connection
            .expect_read_next_update_workload_state()
            .once()
            .return_once(move || Ok(running_workload_state));

        let mut cli_commands = super::CliCommands {
            _response_timeout_ms: 100,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let update_mask = vec!["desiredWorkloads.workloads".to_string()];

        assert!(cli_commands
            .update_state_and_wait_for_complete(new_complete_state, update_mask)
            .await
            .is_ok());
    }

    // [utest->swdd~cli-requests-update-state-with-watch~1]
    // [utest->swdd~cli-requests-update-state-with-watch-error~1]
    #[tokio::test]
    async fn utest_update_state_and_wait_for_complete_error() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let server_connection_error_msg = "server connection error";

        let mut server_connection_mock = MockServerConnection::default();
        server_connection_mock
            .expect_update_state()
            .once()
            .returning(|_, _| {
                Err(ServerConnectionError::ExecutionError(
                    server_connection_error_msg.to_string(),
                ))
            });

        let mut cli_commands = super::CliCommands {
            _response_timeout_ms: 100,
            no_wait: false,
            server_connection: server_connection_mock,
        };

        let empty_update_mask = vec![];
        assert_eq!(
            cli_commands
                .update_state_and_wait_for_complete(CompleteState::default(), empty_update_mask)
                .await,
            Err(CliError::ExecutionError(
                server_connection_error_msg.to_string(),
            ))
        );
    }
}

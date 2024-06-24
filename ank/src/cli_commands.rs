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
use tokio::time::interval;
use wait_list::WaitList;
mod get_workload_table_display;
use get_workload_table_display::GetWorkloadTableDisplay;
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
    ) -> Result<Vec<(WorkloadInstanceName, GetWorkloadTableDisplay)>, CliError> {
        let res_complete_state = self
            .server_connection
            .get_complete_state(&Vec::new())
            .await?;

        let mut workload_infos: Vec<(WorkloadInstanceName, GetWorkloadTableDisplay)> =
            res_complete_state
                .workload_states
                .into_iter()
                .map(|wl_state| {
                    (
                        wl_state.instance_name.clone(),
                        GetWorkloadTableDisplay::new(
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

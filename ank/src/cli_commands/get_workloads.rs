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
use crate::{cli_error::CliError, output_debug, output_update};

use super::cli_table::CliTable;
use super::workload_table_row::WorkloadTableRow;
use super::{CliCommands, WorkloadInfos};
use common::commands::UpdateWorkloadState;

use std::collections::BTreeMap;

impl CliCommands {
    // [impl->swdd~cli-provides-list-of-workloads~1]
    pub async fn get_workloads_table(
        &mut self,
        agent_name: Option<String>,
        state: Option<String>,
        workload_name: Vec<String>,
    ) -> Result<String, CliError> {
        let workload_infos = self.fetch_filtered_workloads(&agent_name, &state, &workload_name).await?;

        // [impl->swdd~cli-shall-present-list-of-workloads~1]
        let table_rows: Vec<WorkloadTableRow> = workload_infos.into_iter().map(|x| x.1).collect();

        // [impl->swdd~cli-shall-present-workloads-as-table~1]
        Ok(CliTable::new(&table_rows)
            .table_with_wrapped_column_to_remaining_terminal_width(
                WorkloadTableRow::ADDITIONAL_INFO_POS,
            )
            .unwrap_or_else(|_err| CliTable::new(&table_rows).create_default_table()))


    }

    // [impl->swdd~cli-get-workloads-with-watch~1]
    pub async fn watch_workloads(
        &mut self,
        agent_name: Option<String>,
        state: Option<String>,
        workload_name: Vec<String>,
    ) -> Result<(), CliError> {

        let workload_infos = self.fetch_filtered_workloads(&agent_name, &state, &workload_name).await?;

        let mut workloads_table_data: BTreeMap<String, WorkloadTableRow> = workload_infos
            .into_iter()
            .map(|(i_name, row)| (i_name.to_string(), row))
            .collect();

        update_table(&workloads_table_data);

        loop {
            let update = self.server_connection.read_next_update_workload_state().await?;

            // [impl->swdd~cli-process-workload-updates~1]
            workloads_table_data = self.process_workload_updates(
                update,
                &agent_name,
                &state,
                &workload_name,
                workloads_table_data,
            ).await?;

            update_table(&workloads_table_data);
        }
    }

    async fn fetch_filtered_workloads(
        &mut self,
        agent_name: &Option<String>,
        state: &Option<String>,
        workload_names: &[String],
    ) -> Result<WorkloadInfos, CliError> {
        // [impl->swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1]
        let mut workload_infos = self.get_workloads().await?;

        output_debug!("The table before filtering:\n{:?}", workload_infos);

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        workload_infos.get_mut().retain(|wi| check_workload_filters(&wi.1, agent_name, state, workload_names));

        // The order of workloads in RequestCompleteState is not sable -> make sure that the user sees always the same order.
        // [impl->swdd~cli-shall-sort-list-of-workloads~1]
        workload_infos.get_mut().sort_by_key(|wi| wi.1.name.clone());

        output_debug!("The table after filtering:\n{:?}", workload_infos);

        Ok(workload_infos)
    }

    // [impl->swdd~cli-process-workload-updates~1]
    async fn process_workload_updates(
        &mut self,
        update: UpdateWorkloadState,
        agent_name: &Option<String>,
        state: &Option<String>,
        workload_name: &[String],
        mut workloads_table_data: BTreeMap<String, WorkloadTableRow>,
    ) -> Result<BTreeMap<String, WorkloadTableRow>, CliError> {
        for wl_state in update.workload_states {
            let instance_name = wl_state.instance_name.to_string();
            let new_state = wl_state.execution_state.state.to_string();

            if !workloads_table_data.contains_key(&instance_name) {
                let mut updated_workloads = self.get_workloads().await?;

                updated_workloads.get_mut().retain(|wi| {
                    check_workload_filters(&wi.1, agent_name, state, workload_name)
                });

                workloads_table_data = updated_workloads
                    .into_iter()
                    .map(|(i_name, row)| (i_name.to_string(), row))
                    .collect();
            } else {
                // Update existing entry
                if new_state == "Removed" {
                    workloads_table_data.remove(&instance_name);
                } else if let Some(row) = workloads_table_data.get_mut(&instance_name) {
                    row.execution_state = new_state;
                    row.set_additional_info(&wl_state.execution_state.additional_info);
                }

                workloads_table_data.retain(|_k, row| {
                    check_workload_filters(row, agent_name, state, workload_name)
                });
            }
        }
        Ok(workloads_table_data)
    }
}

fn update_table(table_data: &BTreeMap<String, WorkloadTableRow>) {
    let rows: Vec<&WorkloadTableRow> = table_data.values().collect();
    let table = CliTable::new(&rows)
        .table_with_wrapped_column_to_remaining_terminal_width(WorkloadTableRow::ADDITIONAL_INFO_POS)
        .unwrap_or_else(|_| CliTable::new(&rows).create_default_table());
    output_update!("{}", table);
}

// [impl->swdd~cli-shall-filter-list-of-workloads~1]
fn check_workload_filters(
    row: &WorkloadTableRow,
    agent_name: &Option<String>,
    state: &Option<String>,
    workload_names: &[String],
) -> bool {

    if let Some(agent) = agent_name {
        if row.agent != *agent {
            return false;
        }
    }

    if let Some(state) = state {
        if row.execution_state.to_lowercase() != state.to_lowercase() {
            return false;
        }
    }

    if !workload_names.is_empty() && !workload_names.iter().any(|wn| wn == &row.name){
            return false;

    }

    true
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
    use api::ank_base;
    use common::{
        objects::{
            self, generate_test_workload_spec_with_param,
            generate_test_workload_states_map_with_data, ExecutionState,
        },
        test_utils,
    };
    use mockall::predicate::eq;

    use crate::cli_commands::{server_connection::MockServerConnection, CliCommands};

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[tokio::test]
    async fn utest_get_workloads_empty_table() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                Ok(
                    (ank_base::CompleteState::from(test_utils::generate_test_complete_state(
                        vec![],
                    )))
                    .into(),
                )
            });
        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd.get_workloads_table(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_table_output =
            "WORKLOAD NAME   AGENT   RUNTIME   EXECUTION STATE   ADDITIONAL INFO";

        assert_eq!(cmd_text.unwrap(), expected_table_output);
    }

    // [utest->swdd~cli-provides-list-of-workloads~1]
    // [utest->swdd~processes-complete-state-to-list-workloads~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1]
    // [utest->swdd~cli-shall-present-list-of-workloads~1]
    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    // [utest->swdd~cli-shall-sort-list-of-workloads~1]
    #[tokio::test]
    async fn utest_get_workloads_no_filtering() {
        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((ank_base::CompleteState::from(test_data)).into()));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd.get_workloads_table(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_table_output = [
            "WORKLOAD NAME   AGENT     RUNTIME   EXECUTION STATE   ADDITIONAL INFO",
            "name1           agent_A   runtime   Running(Ok)                      ",
            "name2           agent_B   runtime   Running(Ok)                      ",
            "name3           agent_B   runtime   Running(Ok)                      ",
        ]
        .join("\n");

        assert_eq!(cmd_text.unwrap(), expected_table_output);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn utest_get_workloads_filter_workload_name() {
        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((ank_base::CompleteState::from(test_data)).into()));
        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd
            .get_workloads_table(None, None, vec!["name1".to_string()])
            .await;
        assert!(cmd_text.is_ok());

        let expected_table_output = [
            "WORKLOAD NAME   AGENT     RUNTIME   EXECUTION STATE   ADDITIONAL INFO",
            "name1           agent_A   runtime   Running(Ok)                      ",
        ]
        .join("\n");

        assert_eq!(cmd_text.unwrap(), expected_table_output);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn utest_get_workloads_filter_agent() {
        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((ank_base::CompleteState::from(test_data)).into()));
        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };
        let cmd_text = cmd
            .get_workloads_table(Some("agent_B".to_string()), None, Vec::new())
            .await;
        assert!(cmd_text.is_ok());

        let expected_table_output = [
            "WORKLOAD NAME   AGENT     RUNTIME   EXECUTION STATE   ADDITIONAL INFO",
            "name2           agent_B   runtime   Running(Ok)                      ",
            "name3           agent_B   runtime   Running(Ok)                      ",
        ]
        .join("\n");

        assert_eq!(cmd_text.unwrap(), expected_table_output);
    }

    // [utest->swdd~cli-shall-filter-list-of-workloads~1]
    #[tokio::test]
    async fn utest_get_workloads_filter_state() {
        let test_data = test_utils::generate_test_complete_state(vec![
            generate_test_workload_spec_with_param(
                "agent_A".to_string(),
                "name1".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name2".to_string(),
                "runtime".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent_B".to_string(),
                "name3".to_string(),
                "runtime".to_string(),
            ),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((ank_base::CompleteState::from(test_data)).into()));
        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };
        let cmd_text = cmd
            .get_workloads_table(None, Some("Failed".to_string()), Vec::new())
            .await;
        assert!(cmd_text.is_ok());

        let expected_table_output =
            "WORKLOAD NAME   AGENT   RUNTIME   EXECUTION STATE   ADDITIONAL INFO";

        assert_eq!(cmd_text.unwrap(), expected_table_output);
    }

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[tokio::test]
    async fn utest_get_workloads_deleted_workload() {
        let test_data = objects::CompleteState {
            workload_states: generate_test_workload_states_map_with_data(
                "agent_A",
                "Workload_1",
                "ID_X",
                ExecutionState::removed(),
            ),
            ..Default::default()
        };

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((ank_base::CompleteState::from(test_data)).into()));
        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd.get_workloads_table(None, None, Vec::new()).await;
        assert!(cmd_text.is_ok());

        let expected_table_output = [
            "WORKLOAD NAME   AGENT     RUNTIME   EXECUTION STATE   ADDITIONAL INFO",
            "Workload_1      agent_A             Removed                          ",
        ]
        .join("\n");

        assert_eq!(cmd_text.unwrap(), expected_table_output);
    }
}

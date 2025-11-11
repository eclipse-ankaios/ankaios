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
use crate::{cli_error::CliError, output_debug};

#[cfg(not(test))]
use crate::output_update;

use super::cli_table::CliTable;
use super::workload_table_row::WorkloadTableRow;
use super::{CliCommands, WorkloadInfos};
use common::commands::UpdateWorkloadState;

use std::collections::BTreeMap;
#[cfg(test)]
lazy_static! {
    pub static ref TEST_TABLE_OUTPUT_DATA: Arc<Mutex<BTreeMap<String, WorkloadTableRow>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
}

impl CliCommands {
    // [impl->swdd~cli-provides-list-of-workloads~1]
    pub async fn get_workloads_table(
        &mut self,
        agent_name: Option<String>,
        state: Option<String>,
        workload_name: Vec<String>,
    ) -> Result<String, CliError> {
        let workload_infos = self
            .fetch_filtered_workloads(&agent_name, &state, &workload_name)
            .await?;

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
        let workload_infos = self
            .fetch_filtered_workloads(&agent_name, &state, &workload_name)
            .await?;

        let mut workloads_table_data: BTreeMap<String, WorkloadTableRow> = workload_infos
            .into_iter()
            .map(|(i_name, row)| (i_name.to_string(), row))
            .collect();

        update_table(&workloads_table_data);

        loop {
            let update = self
                .server_connection
                .read_next_update_workload_state()
                .await?;

            // [impl->swdd~cli-process-workload-updates~1]
            workloads_table_data = self
                .process_workload_updates(
                    update,
                    &agent_name,
                    &state,
                    &workload_name,
                    workloads_table_data,
                )
                .await?;

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
        workload_infos
            .get_mut()
            .retain(|wi| check_workload_filters(&wi.1, agent_name, state, workload_names));

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
            let new_state = wl_state.execution_state.state().to_string();

            if !workloads_table_data.contains_key(&instance_name) {
                let mut updated_workloads = self.get_workloads().await?;

                updated_workloads
                    .get_mut()
                    .retain(|wi| check_workload_filters(&wi.1, agent_name, state, workload_name));

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

#[cfg(test)]
use {
    mockall::lazy_static,
    std::sync::{Arc, Mutex},
};

#[cfg(test)]
fn update_table(table_data: &BTreeMap<String, WorkloadTableRow>) {
    let mut test_out = TEST_TABLE_OUTPUT_DATA.lock().unwrap();
    *test_out = table_data.clone();
}

#[cfg(not(test))]
fn update_table(table_data: &BTreeMap<String, WorkloadTableRow>) {
    let rows: Vec<&WorkloadTableRow> = table_data.values().collect();
    let table = CliTable::new(&rows)
        .table_with_wrapped_column_to_remaining_terminal_width(
            WorkloadTableRow::ADDITIONAL_INFO_POS,
        )
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

    if !workload_names.is_empty() && !workload_names.iter().any(|wn| wn == &row.name) {
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
    use crate::cli_commands::{
        CliCommands,
        get_workloads::TEST_TABLE_OUTPUT_DATA,
        server_connection::{MockServerConnection, ServerConnectionError},
        workload_table_row::WorkloadTableRow,
    };

    use api::ank_base::{
        CompleteState, CompleteStateInternal, ExecutionStateInternal, WorkloadNamed,
        WorkloadStateInternal,
    };
    use api::test_utils::{
        generate_test_complete_state, generate_test_workload_states_map_with_data,
        generate_test_workload_with_param,
    };
    use common::commands::UpdateWorkloadState;

    use mockall::{Sequence, predicate::eq};
    use std::collections::BTreeMap;

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    // [utest->swdd~cli-shall-present-workloads-as-table~1]
    #[tokio::test]
    async fn utest_get_workloads_empty_table() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(
                |_| Ok((CompleteState::from(generate_test_complete_state(vec![]))).into()),
            );
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
        let test_data = generate_test_complete_state(vec![
            generate_test_workload_with_param::<WorkloadNamed>("agent_A", "runtime").name("name1"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name2"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name3"),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((CompleteState::from(test_data)).into()));

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
        let test_data = generate_test_complete_state(vec![
            generate_test_workload_with_param::<WorkloadNamed>("agent_A", "runtime").name("name1"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name2"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name3"),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((CompleteState::from(test_data)).into()));
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
        let test_data = generate_test_complete_state(vec![
            generate_test_workload_with_param::<WorkloadNamed>("agent_A", "runtime").name("name1"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name2"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name3"),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((CompleteState::from(test_data)).into()));
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
        let test_data = generate_test_complete_state(vec![
            generate_test_workload_with_param::<WorkloadNamed>("agent_A", "runtime").name("name1"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name2"),
            generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime").name("name3"),
        ]);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((CompleteState::from(test_data)).into()));
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
        let test_data = CompleteStateInternal {
            workload_states: generate_test_workload_states_map_with_data(
                "agent_A",
                "Workload_1",
                "ID_X",
                ExecutionStateInternal::removed(),
            ),
            ..Default::default()
        };

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((CompleteState::from(test_data)).into()));
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

    // [utest->swdd~cli-get-workloads-with-watch~1]
    #[tokio::test]
    async fn utest_watch_workloads_initial_fetch_fails() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .times(1)
            .return_once(|_| {
                Err(ServerConnectionError::ExecutionError(
                    "Simulated Error".to_string(),
                ))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.watch_workloads(None, None, Vec::new()).await;

        assert!(result.is_err());
    }

    // [utest->swdd~cli-get-workloads-with-watch~1]
    #[tokio::test]
    async fn utest_watch_workloads_addition() {
        let initial_wl =
            generate_test_workload_with_param::<WorkloadNamed>("agent_A", "runtime").name("name1");
        let new_wl =
            generate_test_workload_with_param::<WorkloadNamed>("agent_A", "runtime").name("name2");
        let initial_key = initial_wl.instance_name.to_string();

        let initial_state = generate_test_complete_state(vec![initial_wl.clone()]);
        let updated_state = generate_test_complete_state(vec![initial_wl.clone(), new_wl.clone()]);

        let new_workload = WorkloadStateInternal {
            instance_name: new_wl.clone().instance_name,
            execution_state: ExecutionStateInternal::running(),
        };
        let update_event = UpdateWorkloadState {
            workload_states: vec![new_workload],
        };

        let mut seq = Sequence::new();
        let mut mock_server = MockServerConnection::default();

        mock_server
            .expect_get_complete_state()
            .with(eq(vec![]))
            .times(1)
            .in_sequence(&mut seq)
            .return_once(move |_| Ok((CompleteState::from(initial_state)).into()));

        mock_server
            .expect_read_next_update_workload_state()
            .times(1)
            .in_sequence(&mut seq)
            .return_once(move || {
                let table = TEST_TABLE_OUTPUT_DATA.lock().unwrap();
                assert_eq!(table.len(), 1);
                assert!(table.contains_key(&initial_key));
                Ok(update_event)
            });

        mock_server
            .expect_get_complete_state()
            .with(eq(vec![]))
            .times(1)
            .in_sequence(&mut seq)
            .return_once(move |_| Ok((CompleteState::from(updated_state)).into()));

        mock_server
            .expect_read_next_update_workload_state()
            .times(1)
            .in_sequence(&mut seq)
            .return_once(|| Err(ServerConnectionError::ExecutionError("Stop".into())));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server,
        };

        let result = cmd
            .watch_workloads(Some("agent_A".to_string()), None, vec![])
            .await;
        assert!(result.is_err());

        let table = TEST_TABLE_OUTPUT_DATA.lock().unwrap().clone();

        let mut expected_table = BTreeMap::new();
        expected_table.insert(
            initial_wl.instance_name.to_string(),
            WorkloadTableRow::new(
                initial_wl.instance_name.workload_name(),
                initial_wl.instance_name.agent_name(),
                initial_wl.workload.runtime.clone(),
                ExecutionStateInternal::running().to_string(),
                String::new(),
            ),
        );
        expected_table.insert(
            new_wl.instance_name.to_string(),
            WorkloadTableRow::new(
                new_wl.instance_name.workload_name(),
                new_wl.instance_name.agent_name(),
                new_wl.workload.runtime.clone(),
                ExecutionStateInternal::running().to_string(),
                String::new(),
            ),
        );
        assert_eq!(table, expected_table);
    }

    // [utest->swdd~cli-process-workload-updates~1]
    #[tokio::test]
    async fn utest_process_workload_updates_removal() {
        let wl1 =
            generate_test_workload_with_param::<WorkloadNamed>("agent_A", "runtime").name("name1");

        let key_wl1 = wl1.instance_name.to_string();

        let mut initial_table_data = BTreeMap::new();

        initial_table_data.insert(
            key_wl1.clone(),
            WorkloadTableRow::new(
                wl1.instance_name.workload_name().to_string(),
                wl1.instance_name.agent_name().to_string(),
                wl1.workload.runtime.clone(),
                ExecutionStateInternal::running().to_string(),
                String::new(),
            ),
        );

        let removed_workload_state = WorkloadStateInternal {
            instance_name: wl1.instance_name.clone(),
            execution_state: ExecutionStateInternal::removed(),
        };
        let update_event = UpdateWorkloadState {
            workload_states: vec![removed_workload_state],
        };

        let mock_server_connection = MockServerConnection::default();
        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result_table_data = cmd
            .process_workload_updates(
                update_event,
                &None,
                &None,
                &Vec::new(),
                initial_table_data.clone(),
            )
            .await;

        assert!(
            result_table_data.is_ok(),
            "process_workload_updates failed: {:?}",
            result_table_data.err()
        );
        let final_table_data = result_table_data.unwrap();

        assert!(
            final_table_data.is_empty(),
            "The final table data should be empty after removal."
        );
    }

    // [utest->swdd~cli-process-workload-updates~1]
    #[tokio::test]
    async fn utest_process_workload_updates_state_change() {
        let workload_1 = generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime_1")
            .name("workload_alpha");
        let workload_2 = generate_test_workload_with_param::<WorkloadNamed>("agent_B", "runtime_2")
            .name("workload_beta");

        let key_wl1 = workload_1.instance_name.to_string();
        let key_wl2 = workload_2.instance_name.to_string();

        let mut initial_table_data = BTreeMap::new();

        let wl1_initial_row = WorkloadTableRow::new(
            workload_1.instance_name.workload_name().to_string(),
            workload_1.instance_name.agent_name().to_string(),
            workload_1.workload.runtime.clone(),
            ExecutionStateInternal::running().to_string(),
            "Initial state".to_string(),
        );
        initial_table_data.insert(key_wl1.clone(), wl1_initial_row.clone());

        let wl2_initial_row = WorkloadTableRow::new(
            workload_2.instance_name.workload_name().to_string(),
            workload_2.instance_name.agent_name().to_string(),
            workload_2.workload.runtime.clone(),
            ExecutionStateInternal::running().to_string(),
            "Stable".to_string(),
        );
        initial_table_data.insert(key_wl2.clone(), wl2_initial_row);

        let new_additional_info_for_wl1 = "Critical error occurred".to_string();
        let wl1_updated_execution_state_obj =
            ExecutionStateInternal::failed(new_additional_info_for_wl1.clone()); // The actual ExecutionStateInternal object

        let update_event = UpdateWorkloadState {
            workload_states: vec![WorkloadStateInternal {
                instance_name: workload_1.instance_name.clone(),
                execution_state: wl1_updated_execution_state_obj.clone(),
            }],
        };

        let mock_server_connection = MockServerConnection::default();
        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let agent_name_filter: Option<String> = Some("agent_B".to_string());
        let state_filter: Option<String> =
            Some(wl1_updated_execution_state_obj.state().to_string());
        let workload_name_filter: Vec<String> = Vec::new();

        let result_table_data = cmd
            .process_workload_updates(
                update_event,
                &agent_name_filter,
                &state_filter,
                &workload_name_filter,
                initial_table_data.clone(),
            )
            .await;

        assert!(
            result_table_data.is_ok(),
            "process_workload_updates failed: {:?}",
            result_table_data.err()
        );
        let final_table_data = result_table_data.unwrap();

        let mut expected_table_data = BTreeMap::new();
        expected_table_data.insert(
            key_wl1.clone(),
            WorkloadTableRow::new(
                workload_1.instance_name.workload_name().to_string(),
                workload_1.instance_name.agent_name().to_string(),
                workload_1.workload.runtime.clone(),
                wl1_updated_execution_state_obj.state().to_string(),
                new_additional_info_for_wl1,
            ),
        );

        assert_eq!(
            final_table_data, expected_table_data,
            "The final table data after state update did not match the expected table data."
        );
    }
}

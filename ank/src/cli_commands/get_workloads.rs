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

use super::workload_table::WorkloadTable;
use super::workload_table_row::WorkloadTableRow;
use super::CliCommands;

impl CliCommands {
    // [impl->swdd~cli-provides-list-of-workloads~1]
    pub async fn get_workloads_table(
        &mut self,
        agent_name: Option<String>,
        state: Option<String>,
        workload_name: Vec<String>,
    ) -> Result<String, CliError> {
        // [impl->swdd~cli-blocks-until-ankaios-server-responds-list-workloads~1]
        let mut workload_infos = self.get_workloads().await?;
        output_debug!("The table before filtering:\n{:?}", workload_infos);

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        if let Some(agent_name) = agent_name {
            workload_infos.retain(|wi| wi.1.agent == agent_name);
        }

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        if let Some(state) = state {
            workload_infos.retain(|wi| wi.1.execution_state.to_lowercase() == state.to_lowercase());
        }

        // [impl->swdd~cli-shall-filter-list-of-workloads~1]
        if !workload_name.is_empty() {
            workload_infos.retain(|wi| workload_name.iter().any(|wn| wn == &wi.1.name));
        }

        // The order of workloads in RequestCompleteState is not sable -> make sure that the user sees always the same order.
        // [impl->swdd~cli-shall-sort-list-of-workloads~1]
        workload_infos.sort_by_key(|wi| wi.1.name.clone());

        output_debug!("The table after filtering:\n{:?}", workload_infos);

        // [impl->swdd~cli-shall-present-list-of-workloads~1]
        let table_rows: Vec<WorkloadTableRow> = workload_infos.into_iter().map(|x| x.1).collect();

        // [impl->swdd~cli-shall-present-workloads-as-table~1]
        let workload_table_infos = WorkloadTable::new(table_rows);

        Ok(workload_table_infos
            .create_table_wrapped_additional_info()
            .unwrap_or_else(|| {
                output_debug!(
                    "Failed to create wrapped table output. Continue with default table layout."
                );
                workload_table_infos.create_default_table()
            }))
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

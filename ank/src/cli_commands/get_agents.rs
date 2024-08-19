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
use super::CliCommands;
use crate::{
    cli_commands::{agent_table_row::AgentTableRow, ank_table::AnkTable},
    cli_error::CliError,
    filtered_complete_state::FilteredWorkloadSpec,
    output_debug,
};
use std::collections::HashMap;

const DEFAULT_WORKLOAD_COUNT: u32 = 0;

impl CliCommands {
    // [impl->swdd~cli-provides-list-of-agents~1]
    // [impl->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    pub async fn get_agents(&mut self) -> Result<String, CliError> {
        let empty_filter_mask = [];

        let filtered_complete_state = self
            .server_connection
            .get_complete_state(&empty_filter_mask)
            .await?;

        let workloads = filtered_complete_state
            .desired_state
            .and_then(|desired_state| desired_state.workloads)
            .unwrap_or_default()
            .into_values();

        let workload_count_per_agent = count_workloads_per_agent(workloads);

        let connected_agents = filtered_complete_state
            .agents
            .and_then(|agents| agents.agents)
            .unwrap_or_default()
            .into_keys();

        let agent_table_rows =
            transform_into_table_rows(connected_agents, workload_count_per_agent);

        output_debug!("Got agents of complete state: {:?}", agent_table_rows);

        // [impl->swdd~cli-presents-connected-agents-as-table~1]
        Ok(AnkTable::new(&agent_table_rows).create_default_table())
    }
}

fn count_workloads_per_agent(
    workload_specs: impl Iterator<Item = FilteredWorkloadSpec>,
) -> HashMap<String, u32> {
    workload_specs.fold(HashMap::new(), |mut init, workload| {
        if let Some(agent) = workload.agent {
            let count = init.entry(agent).or_insert(DEFAULT_WORKLOAD_COUNT);
            *count += 1;
        }
        init
    })
}

fn transform_into_table_rows(
    agents_map: impl Iterator<Item = String>,
    mut workload_count_per_agent: HashMap<String, u32>,
) -> Vec<AgentTableRow> {
    let mut agent_table_rows: Vec<AgentTableRow> = agents_map
        .map(|agent_name| {
            let workload_count = workload_count_per_agent
                .remove(&agent_name)
                .unwrap_or(DEFAULT_WORKLOAD_COUNT);
            AgentTableRow {
                agent_name,
                workloads: workload_count,
            }
        })
        .collect();

    // sort to ensure consistent output
    agent_table_rows.sort_by(|a, b| a.agent_name.cmp(&b.agent_name));
    agent_table_rows
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
        server_connection::{MockServerConnection, ServerConnectionError},
        CliCommands,
    };
    use api::ank_base;
    use common::{
        objects::{generate_test_agent_map, generate_test_workload_spec_with_param, AgentMap},
        test_utils,
    };
    use mockall::predicate::eq;

    const RESPONSE_TIMEOUT_MS: u64 = 3000;
    const AGENT_A_NAME: &str = "agent_A";
    const AGENT_B_NAME: &str = "agent_B";
    const AGENT_UNCONNECTED_NAME: &str = "agent_not_connected";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const RUNTIME_NAME: &str = "runtime";

    // [utest->swdd~cli-provides-list-of-agents~1]
    // [utest->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    #[tokio::test]
    async fn test_get_agents() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                Ok(
                    ank_base::CompleteState::from(test_utils::generate_test_complete_state(vec![
                        generate_test_workload_spec_with_param(
                            AGENT_A_NAME.to_string(),
                            WORKLOAD_NAME_1.to_string(),
                            RUNTIME_NAME.to_string(),
                        ),
                        generate_test_workload_spec_with_param(
                            AGENT_B_NAME.to_string(),
                            WORKLOAD_NAME_2.to_string(),
                            RUNTIME_NAME.to_string(),
                        ),
                    ]))
                    .into(),
                )
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = [
            "NAME      WORKLOADS",
            "agent_A   1        ",
            "agent_B   1        ",
        ]
        .join("\n");

        assert_eq!(Ok(expected_table_output), table_output_result);
    }

    // [utest->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    #[tokio::test]
    async fn test_get_agents_agent_not_inside_complete_state_not_listed() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                let mut complete_state = test_utils::generate_test_complete_state(vec![
                    generate_test_workload_spec_with_param(
                        AGENT_UNCONNECTED_NAME.to_string(),
                        WORKLOAD_NAME_2.to_string(),
                        RUNTIME_NAME.to_string(),
                    ),
                ]);

                complete_state.agents = AgentMap::default();
                Ok(ank_base::CompleteState::from(complete_state).into())
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = "NAME   WORKLOADS".to_string();

        assert_eq!(Ok(expected_table_output), table_output_result);
    }

    // [utest->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    #[tokio::test]
    async fn test_get_agents_empty_workloads_in_complete_state() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                let mut complete_state = test_utils::generate_test_complete_state(vec![]);

                complete_state.agents = generate_test_agent_map(AGENT_A_NAME);
                Ok(ank_base::CompleteState::from(complete_state).into())
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = ["NAME      WORKLOADS", "agent_A   0        "].join("\n");

        assert_eq!(Ok(expected_table_output), table_output_result);
    }

    // [utest->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    #[tokio::test]
    async fn test_get_agents_failed_to_get_complete_state() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                Err(ServerConnectionError::ExecutionError(
                    "connection error".to_string(),
                ))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;
        assert!(table_output_result.is_err());
    }
}

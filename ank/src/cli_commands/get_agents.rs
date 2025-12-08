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
    cli_commands::{agent_table_row::AgentTableRow, cli_table::CliTable},
    cli_error::CliError,
    output_debug,
};

use ankaios_api::ank_base::{AgentAttributes, AgentStatus, WorkloadStatesMapSpec};

const EMPTY_FILTER_MASK: [String; 0] = [];

impl CliCommands {
    // [impl->swdd~cli-provides-list-of-agents~1]
    // [impl->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    pub async fn get_agents(&mut self) -> Result<String, CliError> {
        let filtered_complete_state = self
            .server_connection
            .get_complete_state(&EMPTY_FILTER_MASK)
            .await?;

        let workload_states_map = filtered_complete_state
            .workload_states
            .unwrap_or_default()
            .try_into()
            .map_err(|err| {
                CliError::ExecutionError(format!("Failed to convert workload states map: {err}"))
            })?;

        let connected_agents = filtered_complete_state
            .agents
            .and_then(|agents| {
                if !agents.agents.is_empty() {
                    Some(agents.agents)
                } else {
                    None
                }
            })
            .unwrap_or_default()
            .into_iter();

        let agent_table_rows = transform_into_table_rows(connected_agents, workload_states_map);
        output_debug!("Got agents of complete state: {:?}", agent_table_rows);

        // [impl->swdd~cli-presents-connected-agents-as-table~2]
        Ok(CliTable::new(&agent_table_rows).create_default_table())
    }
}

pub fn get_cpu_usage_as_string(agent_attributes: &AgentAttributes) -> String {
    if let Some(AgentStatus {
        cpu_usage: Some(cpu_usage),
        ..
    }) = &agent_attributes.status
    {
        format!("{}%", cpu_usage.cpu_usage)
    } else {
        "".to_string()
    }
}

pub fn get_free_memory_as_string(agent_attributes: &AgentAttributes) -> String {
    if let Some(AgentStatus {
        free_memory: Some(free_memory),
        ..
    }) = &agent_attributes.status
    {
        format!("{}B", free_memory.free_memory)
    } else {
        "".to_string()
    }
}

fn transform_into_table_rows(
    agents_map: impl Iterator<Item = (String, AgentAttributes)>,
    workload_states_map: WorkloadStatesMapSpec,
) -> Vec<AgentTableRow> {
    let mut agent_table_rows: Vec<AgentTableRow> = agents_map
        .map(|(agent_name, agent_attributes)| {
            let workload_states_count = workload_states_map
                .get_workload_state_for_agent(&agent_name)
                .len() as u32;

            AgentTableRow {
                agent_name,
                workloads: workload_states_count,
                cpu_usage: get_cpu_usage_as_string(&agent_attributes),
                free_memory: get_free_memory_as_string(&agent_attributes),
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
        CliCommands,
        server_connection::{MockServerConnection, ServerConnectionError},
    };
    use ankaios_api::ank_base::{AgentMapSpec, CompleteState, ExecutionStateSpec};
    use ankaios_api::test_utils::{
        generate_test_agent_map, generate_test_agent_map_from_workloads,
        generate_test_complete_state, generate_test_workload_named_with_params,
        generate_test_workload_states_map_with_data, fixtures,
    };
    use mockall::predicate::eq;

    const AGENT_UNCONNECTED_NAME: &str = "agent_not_connected";

    // [utest->swdd~cli-presents-connected-agents-as-table~2]
    // [utest->swdd~cli-provides-list-of-agents~1]
    // [utest->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    #[tokio::test]
    async fn test_get_agents() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                Ok(CompleteState::from(generate_test_complete_state(vec![
                    generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[0],
                        "agent_A",
                        fixtures::RUNTIME_NAMES[0],
                    ),
                    generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[1],
                        "agent_B",
                        fixtures::RUNTIME_NAMES[0],
                    ),
                ])))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = [
            "NAME      WORKLOADS   CPU USAGE   FREE MEMORY",
            "agent_A   1           50%         1024B      ",
            "agent_B   1           50%         1024B      ",
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
                let mut complete_state =
                    generate_test_complete_state(vec![generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[0],
                        AGENT_UNCONNECTED_NAME,
                        fixtures::RUNTIME_NAMES[0],
                    )]);

                complete_state.agents = AgentMapSpec::default();
                Ok(CompleteState::from(complete_state))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = "NAME   WORKLOADS   CPU USAGE   FREE MEMORY".to_string();

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
                let mut complete_state = generate_test_complete_state(vec![]);

                complete_state.agents = generate_test_agent_map("agent_A");
                Ok(CompleteState::from(complete_state))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = [
            "NAME      WORKLOADS   CPU USAGE   FREE MEMORY",
            "agent_A   0           50%         1024B      ",
        ]
        .join("\n");

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
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;
        assert!(table_output_result.is_err());
    }

    // [utest->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    #[tokio::test]
    async fn test_get_agents_no_output_of_empty_agents() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                let workload1 = generate_test_workload_named_with_params(
                    fixtures::WORKLOAD_NAMES[1],
                    "agent_A",
                    fixtures::RUNTIME_NAMES[0],
                );
                let mut complete_state = generate_test_complete_state(vec![
                    workload1.clone(),
                    generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[0],
                        String::default(),
                        fixtures::RUNTIME_NAMES[0],
                    ),
                ]);

                complete_state.agents =
                    generate_test_agent_map_from_workloads(&[workload1.workload]);
                Ok(CompleteState::from(complete_state))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = [
            "NAME      WORKLOADS   CPU USAGE   FREE MEMORY",
            "agent_A   1           50%         1024B      ",
        ]
        .join("\n");

        assert_eq!(Ok(expected_table_output), table_output_result);
    }

    // [utest->swdd~cli-processes-complete-state-to-provide-connected-agents~1]
    #[tokio::test]
    async fn test_get_agents_count_the_workload_states() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| {
                let mut complete_state = generate_test_complete_state(vec![]);
                complete_state.agents = generate_test_agent_map(fixtures::AGENT_NAMES[0]);
                // workload1 is deleted from the complete state already but delete not scheduled yet
                complete_state.workload_states = generate_test_workload_states_map_with_data(
                    "agent_A",
                    fixtures::WORKLOAD_NAMES[0],
                    "some workload id",
                    ExecutionStateSpec::waiting_to_stop(),
                );
                Ok(CompleteState::from(complete_state))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let table_output_result = cmd.get_agents().await;

        let expected_table_output = [
            "NAME      WORKLOADS   CPU USAGE   FREE MEMORY",
            "agent_A   1           50%         1024B      ",
        ]
        .join("\n");

        assert_eq!(Ok(expected_table_output), table_output_result);
    }
}

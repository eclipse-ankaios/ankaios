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
use crate::{cli_commands::DESIRED_STATE_WORKLOADS, cli_error::CliError, output_debug};

use ankaios_api::ank_base::CompleteState;

impl CliCommands {
    // [impl->swdd~cli-provides-delete-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~2]
    pub async fn delete_workloads(&mut self, workload_names: Vec<String>) -> Result<(), CliError> {
        let complete_state_update = CompleteState::default();

        let update_mask = workload_names
            .into_iter()
            .map(|name_of_workload_to_delete| {
                format!("{DESIRED_STATE_WORKLOADS}.{name_of_workload_to_delete}")
            })
            .collect();

        output_debug!(
            "Updating with empty complete state and update mask {:?}",
            update_mask
        );

        self.update_state_and_wait_for_complete(complete_state_update, update_mask)
            .await
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
    use crate::cli_commands::{CliCommands, server_connection::MockServerConnection};

    use ankaios_api::{
        ank_base::{
            CompleteState, ExecutionStateSpec, UpdateStateSuccess,
            WorkloadStateSpec,
        },
        test_utils::fixtures,
    };
    use common::{commands::UpdateWorkloadState, from_server_interface::FromServer};

    use mockall::predicate::eq;

    // [utest->swdd~cli-provides-delete-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~2]
    // [utest->swdd~cli-watches-workloads-on-updates~1]
    #[tokio::test]
    async fn utest_delete_workloads_two_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(CompleteState::default()),
                eq(vec![
                    format!("desiredState.workloads.{}", fixtures::WORKLOAD_NAMES[0]),
                    format!("desiredState.workloads.{}", fixtures::WORKLOAD_NAMES[1]),
                ]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec![
                        format!(
                            "{}.{}.{}",
                            fixtures::WORKLOAD_NAMES[0],
                            fixtures::WORKLOAD_IDS[0],
                            fixtures::AGENT_NAMES[0]
                        ),
                        format!(
                            "{}.{}.{}",
                            fixtures::WORKLOAD_NAMES[1],
                            fixtures::WORKLOAD_IDS[0],
                            fixtures::AGENT_NAMES[0]
                        ),
                    ],
                })
            });

        mock_server_connection
            .expect_get_complete_state()
            .times(2)
            .with(eq(vec![]))
            .returning(move |_| Ok(CompleteState::default()));

        mock_server_connection
            .expect_take_missed_from_server_messages()
            .return_once(|| {
                vec![FromServer::UpdateWorkloadState(UpdateWorkloadState {
                    workload_states: vec![
                        WorkloadStateSpec {
                            instance_name: format!(
                                "{}.{}.{}",
                                fixtures::WORKLOAD_NAMES[0],
                                fixtures::WORKLOAD_IDS[0],
                                fixtures::AGENT_NAMES[0]
                            )
                            .try_into()
                            .unwrap(),
                            execution_state: ExecutionStateSpec::removed(),
                        },
                        WorkloadStateSpec {
                            instance_name: format!(
                                "{}.{}.{}",
                                fixtures::WORKLOAD_NAMES[1],
                                fixtures::WORKLOAD_IDS[0],
                                fixtures::AGENT_NAMES[0]
                            )
                            .try_into()
                            .unwrap(),
                            execution_state: ExecutionStateSpec::removed(),
                        },
                    ],
                })]
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let delete_result = cmd
            .delete_workloads(vec![
                fixtures::WORKLOAD_NAMES[0].to_string(),
                fixtures::WORKLOAD_NAMES[1].to_string(),
            ])
            .await;
        assert!(delete_result.is_ok());
    }

    // [utest->swdd~cli-provides-delete-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~2]
    #[tokio::test]
    async fn utest_delete_workloads_unknown_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .once()
            .returning(|_| Ok(CompleteState::default()));
        mock_server_connection
            .expect_update_state()
            .with(
                eq(CompleteState::default()),
                eq(vec!["desiredState.workloads.unknown_workload".to_string()]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec![],
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let delete_result = cmd
            .delete_workloads(vec!["unknown_workload".to_string()])
            .await;
        assert!(delete_result.is_ok());
    }
}

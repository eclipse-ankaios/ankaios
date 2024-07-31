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

use common::objects::CompleteState;

use crate::{cli_error::CliError, output_debug};

use super::CliCommands;

impl CliCommands {
    // [impl->swdd~cli-provides-delete-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~2]
    pub async fn delete_workloads(&mut self, workload_names: Vec<String>) -> Result<(), CliError> {
        let complete_state_update = CompleteState::default();

        let update_mask = workload_names
            .into_iter()
            .map(|name_of_workload_to_delete| {
                format!("desiredState.workloads.{}", name_of_workload_to_delete)
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
    use api::ank_base::{self, UpdateStateSuccess};
    use common::{
        commands::UpdateWorkloadState,
        from_server_interface::FromServer,
        objects::{self, CompleteState, ExecutionState, WorkloadState},
    };
    use mockall::predicate::eq;

    use crate::cli_commands::{server_connection::MockServerConnection, CliCommands};

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    // [utest->swdd~cli-provides-delete-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-delete-workload~2]
    // [utest->swdd~cli-watches-workloads~1]
    #[tokio::test]
    async fn utest_delete_workloads_two_workloads() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state_update = CompleteState::default();

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(complete_state_update.clone()),
                eq(vec![
                    "desiredState.workloads.name1".to_string(),
                    "desiredState.workloads.name2".to_string(),
                ]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec![
                        "name1.abc.agent_B".to_string(),
                        "name2.abc.agent_B".to_string(),
                    ],
                })
            });
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .return_once(|_| Ok((ank_base::CompleteState::from(complete_state_update)).into()));

        mock_server_connection
            .expect_take_missed_from_server_messages()
            .return_once(|| {
                vec![FromServer::UpdateWorkloadState(UpdateWorkloadState {
                    workload_states: vec![
                        WorkloadState {
                            instance_name: "name1.abc.agent_B".try_into().unwrap(),
                            execution_state: ExecutionState {
                                state: objects::ExecutionStateEnum::Removed,
                                additional_info: "".to_string(),
                            },
                        },
                        WorkloadState {
                            instance_name: "name2.abc.agent_B".try_into().unwrap(),
                            execution_state: ExecutionState {
                                state: objects::ExecutionStateEnum::Removed,
                                additional_info: "".to_string(),
                            },
                        },
                    ],
                })]
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let delete_result = cmd
            .delete_workloads(vec!["name1".to_string(), "name2".to_string()])
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

        let complete_state_update = CompleteState::default();

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(complete_state_update),
                eq(vec!["desiredState.workloads.unknown_workload".to_string()]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec![],
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let delete_result = cmd
            .delete_workloads(vec!["unknown_workload".to_string()])
            .await;
        assert!(delete_result.is_ok());
    }
}

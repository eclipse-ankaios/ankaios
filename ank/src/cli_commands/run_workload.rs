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

use common::objects::{CompleteState, StoredWorkloadSpec, Tag};

use crate::{cli_error::CliError, output_debug};

use super::CliCommands;

impl CliCommands {
    // [impl->swdd~cli-provides-run-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-run-workload~2]
    pub async fn run_workload(
        &mut self,
        workload_name: String,
        runtime_name: String,
        runtime_config: String,
        agent_name: String,
        tags_strings: Vec<(String, String)>,
    ) -> Result<(), CliError> {
        let tags: Vec<Tag> = tags_strings
            .into_iter()
            .map(|(k, v)| Tag { key: k, value: v })
            .collect();

        let new_workload = StoredWorkloadSpec {
            agent: agent_name,
            runtime: runtime_name,
            tags,
            runtime_config,
            ..Default::default()
        };
        output_debug!("Request to run new workload: {:?}", new_workload);

        let update_mask = vec![format!("desiredState.workloads.{}", workload_name)];

        let mut complete_state_update = CompleteState::default();
        complete_state_update
            .desired_state
            .workloads
            .insert(workload_name, new_workload);

        output_debug!(
            "The complete state update: {:?}, update mask {:?}",
            complete_state_update,
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
        objects::{self, CompleteState, ExecutionState, StoredWorkloadSpec, Tag, WorkloadState},
    };
    use mockall::predicate::eq;

    use crate::cli_commands::{server_connection::MockServerConnection, CliCommands};

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    // [utest->swdd~cli-provides-run-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-run-workload~2]
    // [utest->swdd~cli-watches-workloads~1]
    #[tokio::test]
    async fn utest_run_workload_one_new_workload() {
        const TEST_WORKLOAD_NAME: &str = "name4";
        let test_workload_agent = "agent_B".to_string();
        let test_workload_runtime_name = "runtime2".to_string();
        let test_workload_runtime_cfg = "some config".to_string();

        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_workload = StoredWorkloadSpec {
            agent: test_workload_agent.to_owned(),
            runtime: test_workload_runtime_name.clone(),
            tags: vec![Tag {
                key: "key".to_string(),
                value: "value".to_string(),
            }],
            runtime_config: test_workload_runtime_cfg.clone(),
            ..Default::default()
        };
        let mut complete_state_update = CompleteState::default();
        complete_state_update
            .desired_state
            .workloads
            .insert(TEST_WORKLOAD_NAME.into(), new_workload);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(complete_state_update.clone()),
                eq(vec![format!(
                    "desiredState.workloads.{}",
                    TEST_WORKLOAD_NAME
                )]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![format!(
                        "{}.abc.agent_B",
                        TEST_WORKLOAD_NAME.to_string()
                    )],
                    deleted_workloads: vec![],
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
                    workload_states: vec![WorkloadState {
                        instance_name: "name4.abc.agent_B".try_into().unwrap(),
                        execution_state: ExecutionState {
                            state: objects::ExecutionStateEnum::Running(
                                objects::RunningSubstate::Ok,
                            ),
                            additional_info: "".to_string(),
                        },
                    }],
                })]
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let run_workload_result = cmd
            .run_workload(
                TEST_WORKLOAD_NAME.into(),
                test_workload_runtime_name,
                test_workload_runtime_cfg,
                test_workload_agent,
                vec![("key".to_string(), "value".to_string())],
            )
            .await;
        assert!(run_workload_result.is_ok());
    }
}

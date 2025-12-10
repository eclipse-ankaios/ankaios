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
use crate::{cli_error::CliError, output_debug};

use ankaios_api::ank_base::{CompleteStateSpec, TagsSpec, WorkloadSpec};

use std::collections::HashMap;

impl CliCommands {
    // [impl->swdd~cli-provides-run-workload~1]
    // [impl->swdd~cli-blocks-until-ankaios-server-responds-run-workload~2]
    pub async fn run_workload(
        &mut self,
        workload_name: String,
        runtime_name: String,
        runtime_config: String,
        agent_name: String,
        tags: HashMap<String, String>,
    ) -> Result<(), CliError> {
        let new_workload = WorkloadSpec {
            agent: agent_name,
            runtime: runtime_name,
            tags: TagsSpec { tags },
            runtime_config,
            restart_policy: Default::default(),
            dependencies: Default::default(),
            control_interface_access: Default::default(),
            configs: Default::default(),
            files: Default::default(),
        };
        output_debug!("Request to run new workload: {:?}", new_workload);

        let update_mask = vec![format!("desiredState.workloads.{}", workload_name)];

        let mut complete_state_update = CompleteStateSpec::default();
        complete_state_update
            .desired_state
            .workloads
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
    use crate::cli_commands::{CliCommands, server_connection::MockServerConnection};

    use ankaios_api::ank_base::{
        CompleteState, CompleteStateSpec, ExecutionStateSpec, TagsSpec, UpdateStateSuccess,
        WorkloadSpec, WorkloadStateSpec,
    };
    use ankaios_api::test_utils::fixtures;
    use common::{commands::UpdateWorkloadState, from_server_interface::FromServer};

    use mockall::predicate::eq;
    use std::collections::HashMap;

    // [utest->swdd~cli-provides-run-workload~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-run-workload~2]
    // [utest->swdd~cli-watches-workloads-on-updates~1]
    #[tokio::test]
    async fn utest_run_workload_one_new_workload() {
        let test_workload_runtime_cfg = "some config".to_string();

        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_workload = WorkloadSpec {
            agent: fixtures::AGENT_NAMES[0].to_owned(),
            runtime: fixtures::RUNTIME_NAMES[0].to_owned(),
            tags: TagsSpec {
                tags: HashMap::from([("key".to_string(), "value".to_string())]),
            },
            runtime_config: test_workload_runtime_cfg.clone(),
            restart_policy: Default::default(),
            dependencies: Default::default(),
            control_interface_access: Default::default(),
            configs: Default::default(),
            files: Default::default(),
        };
        let mut complete_state_update = CompleteStateSpec::default();
        complete_state_update
            .desired_state
            .workloads
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].into(), new_workload);

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(eq(vec![]))
            .once()
            .return_once(|_| Ok(CompleteState::default()));
        mock_server_connection
            .expect_update_state()
            .with(
                eq(complete_state_update.clone()),
                eq(vec![format!(
                    "desiredState.workloads.{}",
                    fixtures::WORKLOAD_NAMES[0]
                )]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![format!(
                        "{}.{}.{}",
                        fixtures::WORKLOAD_NAMES[0],
                        fixtures::WORKLOAD_IDS[0],
                        fixtures::AGENT_NAMES[0],
                    )],
                    deleted_workloads: vec![],
                })
            });

        mock_server_connection
            .expect_get_complete_state()
            .once()
            .with(eq(vec![]))
            .return_once(|_| Ok(CompleteState::from(complete_state_update)));
        mock_server_connection
            .expect_take_missed_from_server_messages()
            .return_once(|| {
                vec![FromServer::UpdateWorkloadState(UpdateWorkloadState {
                    workload_states: vec![WorkloadStateSpec {
                        instance_name: format!(
                            "{}.{}.{}",
                            fixtures::WORKLOAD_NAMES[0],
                            fixtures::WORKLOAD_IDS[0],
                            fixtures::AGENT_NAMES[0]
                        )
                        .try_into()
                        .unwrap(),
                        execution_state: ExecutionStateSpec::running(),
                    }],
                })]
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let run_workload_result = cmd
            .run_workload(
                fixtures::WORKLOAD_NAMES[0].into(),
                fixtures::RUNTIME_NAMES[0].to_owned(),
                test_workload_runtime_cfg,
                fixtures::AGENT_NAMES[0].to_owned(),
                HashMap::from([("key".to_string(), "value".to_string())]),
            )
            .await;
        assert!(run_workload_result.is_ok());
    }
}

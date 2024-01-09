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

use common::{
    execution_interface::{ExecutionCommand, ExecutionReceiver},
    state_change_interface::StateChangeSender,
};

use crate::parameter_storage::ParameterStorage;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
// [impl->swdd~agent-shall-use-interfaces-to-server~1]
pub struct AgentManager {
    agent_name: String,
    runtime_manager: RuntimeManager,
    // [impl->swdd~communication-to-from-agent-middleware~1]
    receiver: ExecutionReceiver,
    _to_server: StateChangeSender,
    parameter_storage: ParameterStorage,
}

impl AgentManager {
    pub fn new(
        agent_name: String,
        receiver: ExecutionReceiver,
        runtime_manager: RuntimeManager,
        _to_server: StateChangeSender,
    ) -> AgentManager {
        AgentManager {
            agent_name,
            runtime_manager,
            receiver,
            _to_server,
            parameter_storage: ParameterStorage::new(),
        }
    }

    pub async fn start(&mut self) {
        log::info!("Starting ...");
        self.listen_to_server().await
    }

    // [impl->swdd~agent-manager-listens-requests-from-server~1]
    async fn listen_to_server(&mut self) {
        log::debug!("Start listening to server.");
        while let Some(x) = self.receiver.recv().await {
            match x {
                ExecutionCommand::UpdateWorkload(method_obj) => {
                    log::debug!("Agent '{}' received UpdateWorkload:\n\tAdded workloads: {:?}\n\tDeleted workloads: {:?}",
                    self.agent_name,
                    method_obj.added_workloads,
                    method_obj.deleted_workloads);

                    self.runtime_manager
                        .handle_update_workload(
                            method_obj.added_workloads,
                            method_obj.deleted_workloads,
                        )
                        .await;
                }
                ExecutionCommand::UpdateWorkloadState(method_obj) => {
                    log::debug!(
                        "Agent '{}' received UpdateWorkloadState: {:?}",
                        self.agent_name,
                        method_obj
                    );

                    // [impl->swdd~agent-stores-all-workload-states~1]
                    method_obj
                        .workload_states
                        .into_iter()
                        .for_each(|workload_state| {
                            log::info!("The server reports workload state '{:?}' for the workload '{}' in the agent '{}'", workload_state.execution_state,
                            workload_state.workload_name, workload_state.agent_name);
                            self.parameter_storage.update_workload_state(workload_state)
                        });
                }
                ExecutionCommand::CompleteState(method_obj) => {
                    log::debug!(
                        "Agent '{}' received CompleteState: {:?}",
                        self.agent_name,
                        method_obj
                    );

                    // [impl->swdd~agent-forward-responses-to-control-interface-pipe~1]
                    self.runtime_manager
                        .forward_complete_state(*method_obj)
                        .await;
                }
                ExecutionCommand::Stop(_method_obj) => {
                    log::debug!("Agent '{}' received Stop from server", self.agent_name);

                    break;
                }
            }
        }
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
    use super::*;
    use crate::agent_manager::AgentManager;
    use common::{
        commands::CompleteState,
        execution_interface::ExecutionInterface,
        objects::{ExecutionState, WorkloadState},
        test_utils::generate_test_workload_spec_with_param,
    };
    use mockall::predicate::*;
    use tokio::{join, sync::mpsc::channel};

    const BUFFER_SIZE: usize = 20;
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_2_NAME: &str = "workload2";
    const REQUEST_ID: &str = "request_id";
    const RUNTIME_NAME: &str = "runtime_name";

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    #[tokio::test]
    async fn utest_agent_manager_update_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_handle_update_workload()
            .once()
            .return_const(());

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
        );

        let workload_spec_1 = generate_test_workload_spec_with_param(
            AGENT_NAME.into(),
            WORKLOAD_1_NAME.into(),
            RUNTIME_NAME.into(),
        );

        let workload_spec_2 = generate_test_workload_spec_with_param(
            AGENT_NAME.into(),
            WORKLOAD_2_NAME.into(),
            RUNTIME_NAME.into(),
        );

        let update_workload_result = to_manager
            .update_workload(
                vec![workload_spec_1.clone(), workload_spec_2.clone()],
                vec![],
            )
            .await;
        assert!(update_workload_result.is_ok());

        let handle = agent_manager.start();
        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle);
    }

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    // [utest->swdd~agent-stores-all-workload-states~1]
    #[tokio::test]
    async fn utest_agent_manager_update_workload_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager.expect_handle_update_workload().never();
        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
        );

        let workload_states = vec![WorkloadState {
            workload_name: WORKLOAD_1_NAME.into(),
            agent_name: AGENT_NAME.into(),
            execution_state: ExecutionState::ExecRunning,
        }];

        let update_workload_result = to_manager.update_workload_state(workload_states).await;
        assert!(update_workload_result.is_ok());

        let handle = agent_manager.start();
        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle);

        let workload_states = agent_manager
            .parameter_storage
            .get_workload_states(&AGENT_NAME.into())
            .expect("expected workload states for agent");

        assert_eq!(
            workload_states.get(WORKLOAD_1_NAME).unwrap().to_owned(),
            ExecutionState::ExecRunning
        );
    }

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    #[tokio::test]
    async fn utest_agent_manager_no_update_on_empty_workload_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager.expect_handle_update_workload().never();
        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
        );

        let initial_workload_states = vec![WorkloadState {
            workload_name: WORKLOAD_1_NAME.into(),
            agent_name: AGENT_NAME.into(),
            execution_state: ExecutionState::ExecRunning,
        }];
        let initial_update_workload_result = to_manager
            .update_workload_state(initial_workload_states)
            .await;
        assert!(initial_update_workload_result.is_ok());

        let new_empty_states = vec![];
        let update_workload_result = to_manager.update_workload_state(new_empty_states).await;
        assert!(update_workload_result.is_ok());

        let handle = agent_manager.start();
        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle);

        let workload_states = agent_manager
            .parameter_storage
            .get_workload_states(&AGENT_NAME.into())
            .expect("expected workload states for agent");

        assert_eq!(
            workload_states.get(WORKLOAD_1_NAME).unwrap().to_owned(),
            ExecutionState::ExecRunning
        );
    }

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_agent_manager_forwards_complete_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);

        let complete_state = CompleteState {
            request_id: format!("{WORKLOAD_1_NAME}@{REQUEST_ID}"),
            ..Default::default()
        };

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_forward_complete_state()
            .with(eq(complete_state.clone()))
            .once()
            .return_const(());

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
        );

        let complete_state_result = to_manager.complete_state(complete_state).await;
        assert!(complete_state_result.is_ok());

        let handle = agent_manager.start();

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle);
    }
}

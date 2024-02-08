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
    from_server_interface::{FromServer, FromServerReceiver},
    std_extensions::IllegalStateResult,
    to_server_interface::{ToServer, ToServerInterface, ToServerReceiver, ToServerSender},
};

use crate::parameter_storage::ParameterStorage;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
// [impl->swdd~agent-shall-use-interfaces-to-server~1]
pub struct AgentManager {
    agent_name: String,
    runtime_manager: RuntimeManager,
    // [impl->swdd~communication-to-from-agent-middleware~1]
    from_server_receiver: FromServerReceiver,
    to_server: ToServerSender,
    workload_state_receiver: ToServerReceiver,
    parameter_storage: ParameterStorage,
}

impl AgentManager {
    pub fn new(
        agent_name: String,
        from_server_receiver: FromServerReceiver,
        runtime_manager: RuntimeManager,
        to_server: ToServerSender,
        workload_state_receiver: ToServerReceiver,
    ) -> AgentManager {
        AgentManager {
            agent_name,
            runtime_manager,
            from_server_receiver,
            to_server,
            workload_state_receiver,
            parameter_storage: ParameterStorage::new(),
        }
    }

    pub async fn start(&mut self) {
        log::info!("Starting ...");
        loop {
            tokio::select! {
                from_server_msg = self.from_server_receiver.recv() => {
                    if let Some(from_server) = from_server_msg {
                        if self.execute_from_server_command(from_server).await.is_none() {
                            break
                        }
                    }
                }
                to_server_msg = self.workload_state_receiver.recv() => {
                    if let Some(workload_states_msg) = to_server_msg {
                        self.store_and_forward_own_workload_states(workload_states_msg).await;
                    }
                }
            }
        }
    }

    // [impl->swdd~agent-manager-listens-requests-from-server~1]
    async fn execute_from_server_command(&mut self, from_server_msg: FromServer) -> Option<()> {
        log::debug!("Start listening to server.");
        match from_server_msg {
            FromServer::UpdateWorkload(method_obj) => {
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
                Some(())
            }
            FromServer::UpdateWorkloadState(method_obj) => {
                log::debug!(
                    "Agent '{}' received UpdateWorkloadState: {:?}",
                    self.agent_name,
                    method_obj
                );

                // [impl->swdd~agent-manager-stores-all-workload-states~1]
                method_obj
                        .workload_states
                        .into_iter()
                        .for_each(|workload_state| {
                            log::info!("The server reports workload state '{:?}' for the workload '{}' in the agent '{}'", workload_state.execution_state,
                            workload_state.workload_name, workload_state.agent_name);
                            self.parameter_storage.update_workload_state(workload_state)
                        });
                Some(())
            }
            FromServer::Response(method_obj) => {
                log::debug!(
                    "Agent '{}' received Response: {:?}",
                    self.agent_name,
                    method_obj
                );

                // [impl->swdd~agent-forward-responses-to-control-interface-pipe~1]
                self.runtime_manager.forward_response(method_obj).await;

                Some(())
            }
            FromServer::Stop(_method_obj) => {
                log::debug!("Agent '{}' received Stop from server", self.agent_name);
                None
            }
        }
    }

    async fn store_and_forward_own_workload_states(&mut self, to_server_msg: ToServer) {
        let ToServer::UpdateWorkloadState(common::commands::UpdateWorkloadState {
            workload_states,
        }) = to_server_msg
        else {
            std::unreachable!("expected UpdateWorkloadState msg.");
        };
        workload_states.iter().for_each(|workload_state| {
            self.parameter_storage
                .update_workload_state(workload_state.clone());
        });

        self.to_server
            .update_workload_state(workload_states)
            .await
            .unwrap_or_illegal_state();
        self.runtime_manager
            .state_update(&self.parameter_storage)
            .await;
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
        commands::{self, Response, ResponseContent},
        from_server_interface::FromServerInterface,
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
        // The from_server_receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle);
    }

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    // [utest->swdd~agent-manager-stores-all-workload-states~1]
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
        // The from_server_receiver in the agent receives the message and terminates the infinite waiting-loop.
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
        // The from_server_receiver in the agent receives the message and terminates the infinite waiting-loop.
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

        let request_id = format!("{WORKLOAD_1_NAME}@{REQUEST_ID}");
        let complete_state: commands::CompleteState = Default::default();

        let response = Response {
            request_id: request_id.clone(),
            response_content: ResponseContent::CompleteState(Box::new(complete_state.clone())),
        };

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_forward_response()
            .with(eq(response.clone()))
            .once()
            .return_const(());

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
        );

        let complete_state_result = to_manager.complete_state(request_id, complete_state).await;
        assert!(complete_state_result.is_ok());

        let handle = agent_manager.start();

        // The from_server_receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle);
    }
}

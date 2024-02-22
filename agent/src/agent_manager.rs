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
    std_extensions::{GracefulExitResult, IllegalStateResult},
    to_server_interface::{ToServer, ToServerInterface, ToServerReceiver, ToServerSender},
};

#[cfg_attr(test, mockall_double::double)]
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
                    let from_server = from_server_msg
                        .ok_or("Channel to listen to server closed.".to_string())
                        .unwrap_or_exit("Abort");

                    if self.execute_from_server_command(from_server).await.is_none() {
                        break;
                    }
                }
                to_server_msg = self.workload_state_receiver.recv() => {
                    let workload_states_msg = to_server_msg
                        .ok_or("Channel to listen to own workload states closed.".to_string())
                        .unwrap_or_exit("Abort");

                    self.store_and_forward_own_workload_states(workload_states_msg).await;
                }
            }
        }
    }

    // [impl->swdd~agent-manager-listens-requests-from-server~1]
    async fn execute_from_server_command(&mut self, from_server_msg: FromServer) -> Option<()> {
        log::debug!("Process command received from server.");

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
                        &self.parameter_storage,
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

                let new_workload_states = method_obj.workload_states;

                if !new_workload_states.is_empty() {
                    // [impl->swdd~agent-manager-stores-all-workload-states~1]
                    for new_workload_state in new_workload_states {
                        log::info!("The server reports workload state '{:?}' for the workload '{}' in the agent '{}'", new_workload_state.execution_state,
                    new_workload_state.instance_name.workload_name(), new_workload_state.instance_name.agent_name());
                        self.parameter_storage
                            .update_workload_state(new_workload_state);
                    }
                    self.runtime_manager
                        .update_workloads_on_new_workload_states(&self.parameter_storage)
                        .await;
                }

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
        log::debug!("Storing and forwarding own workload states.");

        let ToServer::UpdateWorkloadState(common::commands::UpdateWorkloadState {
            workload_states,
        }) = to_server_msg
        else {
            std::unreachable!("expected UpdateWorkloadState msg.");
        };

        workload_states.iter().for_each(|new_workload_state| {
            log::info!(
                "The agent '{}' reports workload state '{:?}' for the workload '{}'",
                new_workload_state.instance_name.agent_name(),
                new_workload_state.execution_state,
                new_workload_state.instance_name.workload_name(),
            );

            self.parameter_storage
                .update_workload_state(new_workload_state.clone());
        });

        if !workload_states.is_empty() {
            self.runtime_manager
                .update_workloads_on_new_workload_states(&self.parameter_storage)
                .await;

            self.to_server
                .update_workload_state(workload_states)
                .await
                .unwrap_or_illegal_state();
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
    use crate::parameter_storage::MockParameterStorage;
    use common::{
        commands::{Goodbye, Response, ResponseContent, UpdateWorkloadState},
        from_server_interface::FromServerInterface,
        objects::{generate_test_workload_spec_with_param, CompleteState, ExecutionState},
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

        let mock_parameter_storage_context = MockParameterStorage::new_context();
        mock_parameter_storage_context
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);
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
            workload_state_receiver,
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

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let update_workload_result = to_manager
            .update_workload(
                vec![workload_spec_1.clone(), workload_spec_2.clone()],
                vec![],
            )
            .await;
        assert!(update_workload_result.is_ok());

        // Terminate the infinite receiver loop
        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
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
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let workload_state = common::objects::generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_NAME,
            ExecutionState::running(),
        );

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager.expect_handle_update_workload().never();
        mock_runtime_manager
            .expect_update_workloads_on_new_workload_states()
            .once()
            .return_const(());

        let mut mock_parameter_storage = MockParameterStorage::default();
        mock_parameter_storage
            .expect_update_workload_state()
            .with(mockall::predicate::eq(workload_state.clone()))
            .once()
            .return_const(());

        let mock_parameter_storage_context = MockParameterStorage::new_context();
        mock_parameter_storage_context
            .expect()
            .once()
            .return_once(|| mock_parameter_storage);

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let update_workload_result = to_manager.update_workload_state(vec![workload_state]).await;
        assert!(update_workload_result.is_ok());

        // Terminate the infinite receiver loop
        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    #[tokio::test]
    async fn utest_agent_manager_no_update_on_empty_workload_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mock_parameter_storage = MockParameterStorage::default();
        mock_parameter_storage
            .expect_update_workload_state()
            .never()
            .return_const(());

        let mock_parameter_storage_context = MockParameterStorage::new_context();
        mock_parameter_storage_context
            .expect()
            .once()
            .return_once(|| mock_parameter_storage);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager.expect_handle_update_workload().never();

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let new_empty_states = vec![];
        let update_workload_result = to_manager.update_workload_state(new_empty_states).await;
        assert!(update_workload_result.is_ok());

        let handle = tokio::spawn(async move { agent_manager.start().await });

        // Terminate the infinite receiver loop
        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    // [utest->swdd~agent-forward-responses-to-control-interface-pipe~1]
    #[tokio::test]
    async fn utest_agent_manager_forwards_complete_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_parameter_storage_context = MockParameterStorage::new_context();
        mock_parameter_storage_context
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let request_id = format!("{WORKLOAD_1_NAME}@{REQUEST_ID}");
        let complete_state: CompleteState = Default::default();

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
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let complete_state_result = to_manager.complete_state(request_id, complete_state).await;
        assert!(complete_state_result.is_ok());

        // Terminate the infinite receiver loop
        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    // [utest->swdd~agent-uses-async-channels~1]
    #[tokio::test]
    async fn utest_agent_manager_receives_own_workload_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, mut to_server_receiver) = channel(BUFFER_SIZE);
        let (workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let workload_state = common::objects::generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_NAME,
            ExecutionState::running(),
        );

        let mut mock_parameter_storage = MockParameterStorage::default();
        mock_parameter_storage
            .expect_update_workload_state()
            .with(mockall::predicate::eq(workload_state.clone()))
            .once()
            .return_const(());

        let mock_parameter_storage_context = MockParameterStorage::new_context();
        mock_parameter_storage_context
            .expect()
            .once()
            .return_once(|| mock_parameter_storage);

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_update_workloads_on_new_workload_states()
            .once()
            .return_const(());

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let workload_states = vec![workload_state.clone()];
        assert!(workload_state_sender
            .update_workload_state(workload_states.clone())
            .await
            .is_ok());

        let expected_workload_states =
            ToServer::UpdateWorkloadState(UpdateWorkloadState { workload_states });
        assert_eq!(
            Ok(Some(expected_workload_states)),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(200),
                to_server_receiver.recv()
            )
            .await
        );

        // Terminate the infinite receiver loop
        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    // [utest->swdd~agent-uses-async-channels~1]
    #[tokio::test]
    async fn utest_agent_manager_no_update_on_own_empty_workload_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, mut to_server_receiver) = channel(BUFFER_SIZE);
        let (workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let mut mock_parameter_storage = MockParameterStorage::default();
        mock_parameter_storage
            .expect_update_workload_state()
            .never();

        let mock_parameter_storage_context = MockParameterStorage::new_context();
        mock_parameter_storage_context
            .expect()
            .once()
            .return_once(|| mock_parameter_storage);

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_update_workloads_on_new_workload_states()
            .never();

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let empty_workload_states = vec![];
        assert!(workload_state_sender
            .update_workload_state(empty_workload_states)
            .await
            .is_ok());

        assert!(to_server_receiver.try_recv().is_err());

        // Terminate the infinite receiver loop
        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    #[tokio::test]
    #[should_panic]
    async fn utest_agent_manager_receives_own_workload_states_panic_on_wrong_response() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_parameter_storage_context = MockParameterStorage::new_context();
        mock_parameter_storage_context
            .expect()
            .once()
            .return_once(MockParameterStorage::default);

        let (_to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _to_server_receiver) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let mock_runtime_manager = RuntimeManager::default();
        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        // shall panic because of wrong passed message
        agent_manager
            .store_and_forward_own_workload_states(ToServer::Goodbye(Goodbye {}))
            .await;
    }
}

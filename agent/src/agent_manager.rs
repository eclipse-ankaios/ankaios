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
    objects::WorkloadState,
    std_extensions::{GracefulExitResult, IllegalStateResult},
    to_server_interface::{ToServerInterface, ToServerSender},
};

#[cfg_attr(test, mockall_double::double)]
use crate::workload_state::workload_state_store::WorkloadStateStore;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
use crate::workload_state::WorkloadStateReceiver;
// [impl->swdd~agent-shall-use-interfaces-to-server~1]
pub struct AgentManager {
    agent_name: String,
    runtime_manager: RuntimeManager,
    // [impl->swdd~communication-to-from-agent-middleware~1]
    from_server_receiver: FromServerReceiver,
    to_server: ToServerSender,
    workload_state_receiver: WorkloadStateReceiver,
    workload_state_store: WorkloadStateStore,
}

impl AgentManager {
    pub fn new(
        agent_name: String,
        from_server_receiver: FromServerReceiver,
        runtime_manager: RuntimeManager,
        to_server: ToServerSender,
        workload_state_receiver: WorkloadStateReceiver,
    ) -> AgentManager {
        AgentManager {
            agent_name,
            runtime_manager,
            from_server_receiver,
            to_server,
            workload_state_receiver,
            workload_state_store: WorkloadStateStore::new(),
        }
    }

    pub async fn start(&mut self) {
        log::info!("Awaiting commands from the server ...");
        loop {
            tokio::select! {
                // [impl->swdd~agent-manager-listens-requests-from-server~1]
                from_server_msg = self.from_server_receiver.recv() => {
                    let from_server = from_server_msg
                        .ok_or("Channel to listen to server closed.".to_string())
                        .unwrap_or_exit("Abort");

                    if self.execute_from_server_command(from_server).await.is_none() {
                        break;
                    }
                }
                // [impl->swdd~agent-manager-receives-workload-states-of-its-workloads~1]
                workload_state = self.workload_state_receiver.recv() => {
                    let workload_state = workload_state
                        .ok_or("Channel to listen to own workload states closed.".to_string())
                        .unwrap_or_exit("Abort");

                    self.store_and_forward_own_workload_states(workload_state).await;
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

                // [impl->swdd~agent-handles-update-workload-requests~1]
                self.runtime_manager
                    .handle_update_workload(
                        method_obj.added_workloads,
                        method_obj.deleted_workloads,
                        &self.workload_state_store,
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
                        log::debug!("The server reports workload state '{:?}' for the workload '{}' in the agent '{}'", new_workload_state.execution_state,
                    new_workload_state.instance_name.workload_name(), new_workload_state.instance_name.agent_name());
                        self.workload_state_store
                            .update_workload_state(new_workload_state);
                    }
                    // [impl->swdd~agent-handles-update-workload-state-requests~1]
                    self.runtime_manager
                        .update_workloads_on_fulfilled_dependencies(&self.workload_state_store)
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

    async fn store_and_forward_own_workload_states(
        &mut self,
        mut new_workload_state: WorkloadState,
    ) {
        // execute hysteresis on the local workload states as we could be stopping
        // [impl->swdd~agent-manager-hysteresis_on-workload-states-of-its-workloads~1]
        if let Some(old_execution_state) = self
            .workload_state_store
            .get_state_of_workload(new_workload_state.instance_name.workload_name())
        {
            new_workload_state.execution_state =
                old_execution_state.transition(new_workload_state.execution_state);
        }

        log::debug!(
            "Storing and forwarding local workload state '{:?}'.",
            new_workload_state
        );

        // [impl->swdd~agent-stores-workload-states-of-its-workloads~1]
        self.workload_state_store
            .update_workload_state(new_workload_state.clone());

        // notify the runtime manager s.t. dependencies and restarts can be handled
        // [impl->swdd~agent-handles-update-workload-state-requests~1]
        self.runtime_manager
            .update_workloads_on_fulfilled_dependencies(&self.workload_state_store)
            .await;

        // [impl->swdd~agent-sends-workload-states-of-its-workloads-to-server~2]
        self.to_server
            .update_workload_state(vec![new_workload_state])
            .await
            .unwrap_or_illegal_state();
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
    use crate::workload_state::{
        workload_state_store::{mock_parameter_storage_new_returns, MockWorkloadStateStore},
        WorkloadStateSenderInterface,
    };
    use api::ank_base;
    use common::{
        commands::UpdateWorkloadState,
        from_server_interface::FromServerInterface,
        objects::{generate_test_workload_spec_with_param, ExecutionState},
        to_server_interface::ToServer,
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
    // [utest->swdd~agent-handles-update-workload-requests~1]
    #[tokio::test]
    async fn utest_agent_manager_update_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_wl_state_store_context = MockWorkloadStateStore::default();
        mock_parameter_storage_new_returns(mock_wl_state_store_context);

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
    // [utest->swdd~agent-handles-update-workload-state-requests~1]
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
            .expect_update_workloads_on_fulfilled_dependencies()
            .once()
            .return_const(());

        let mut mock_wl_state_store = MockWorkloadStateStore::default();
        mock_wl_state_store
            .expected_update_workload_state_parameters
            .push_back(workload_state.clone());
        mock_parameter_storage_new_returns(mock_wl_state_store);

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

        let mock_wl_state_store = MockWorkloadStateStore::default();
        mock_parameter_storage_new_returns(mock_wl_state_store);

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

        let mock_wl_state_store_context = MockWorkloadStateStore::default();
        mock_parameter_storage_new_returns(mock_wl_state_store_context);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let request_id = format!("{WORKLOAD_1_NAME}@{REQUEST_ID}");
        let complete_state: ank_base::CompleteState = Default::default();

        let response = ank_base::Response {
            request_id: request_id.clone(),
            response_content: Some(ank_base::response::ResponseContent::CompleteState(
                complete_state.clone(),
            )),
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

    // [utest->swdd~agent-manager-receives-workload-states-of-its-workloads~1]
    // [utest->swdd~agent-stores-workload-states-of-its-workloads~1]
    // [utest->swdd~agent-sends-workload-states-of-its-workloads-to-server~2]
    // [utest->swdd~agent-handles-update-workload-state-requests~1]
    // [utest->swdd~agent-manager-hysteresis_on-workload-states-of-its-workloads~1]
    #[tokio::test]
    async fn utest_agent_manager_receives_own_workload_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, mut to_server_receiver) = channel(BUFFER_SIZE);
        let (workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let workload_state_incoming = common::objects::generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_NAME,
            ExecutionState::running(),
        );

        let wl_state_after_hysteresis = common::objects::generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_NAME,
            ExecutionState::stopping_requested(),
        );

        let mut mock_wl_state_store = MockWorkloadStateStore::default();

        mock_wl_state_store.states_storage.insert(
            WORKLOAD_1_NAME.to_string(),
            ExecutionState::stopping_requested(),
        );

        mock_wl_state_store
            .expected_update_workload_state_parameters
            .push_back(wl_state_after_hysteresis.clone());

        mock_parameter_storage_new_returns(mock_wl_state_store);

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_update_workloads_on_fulfilled_dependencies()
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

        workload_state_sender
            .report_workload_execution_state(
                &workload_state_incoming.instance_name,
                workload_state_incoming.execution_state.clone(),
            )
            .await;

        let expected_workload_states = ToServer::UpdateWorkloadState(UpdateWorkloadState {
            workload_states: vec![wl_state_after_hysteresis],
        });
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
}

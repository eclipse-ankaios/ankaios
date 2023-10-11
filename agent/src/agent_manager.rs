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
        log::debug!("Start listening to communication server.");
        while let Some(x) = self.receiver.recv().await {
            match x {
                // [impl->swdd~agent-skips-unknown-runtime~1]
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

                    // [impl->swdd~agent-manager-stores-all-workload-states~1]
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
    // [utest->swdd~agent-adapter-start-new-workloads-if-non-found~1]
    // [utest->swdd~agent-starts-runtimes-adapters-with-initial-workloads~1]
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
    // [utest->swdd~agent-uses-id-prefix-forward-control-interface-response-correct-workload~1]
    // [utest->swdd~agent-remove-id-prefix-forwarding-control-interface-response~1]
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

    // // [utest->swdd~agent-skips-unknown-runtime~1]
    // // [utest->swdd~agent-create-control-interface-pipes-per-workload~1]
    // // [utest->swdd~agent-starts-runtimes-adapters-with-initial-workloads~1]
    // #[tokio::test]
    // async fn utest_agent_manager_handles_initial_update_workload_unknown_runtime() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_1_NAME.into(),
    //         "no such runtime".into(),
    //     );

    //     let PipesChannelContextMockData {
    //         pipes_channel_context_new_context,
    //         ..
    //     } = generate_test_pipes_channel_context_mock();

    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new().build();

    //     let update_workload_result = to_manager
    //         .update_workload(vec![workload_spec.clone()], vec![])
    //         .await;
    //     assert!(update_workload_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);
    //     pipes_channel_context_new_context.checkpoint();

    //     assert!(agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&workload_spec.workload.name)
    //         .is_none());

    //     assert!(agent_manager
    //         .workload_pipes_context_map
    //         .get(WORKLOAD_1_NAME)
    //         .is_none());
    // }

    // // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // // [utest->swdd~agent-forwards-start-workload~1]
    // // [utest->swdd~agent-uses-runtime-adapter~1]
    // // [utest->swdd~agent-uses-async-channels~1]
    // // [utest->swdd~agent-manager-stores-workload-runtime-mapping~1]
    // // [utest->swdd~agent-create-control-interface-pipes-per-workload~1]
    // #[tokio::test]
    // async fn utest_agent_manager_update_workload_added_workloads_forwards_to_runtimes() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_1_NAME.into(),
    //         RUNTIME_NAME.into(),
    //     );

    //     let PipesChannelContextMockData {
    //         pipes_channel_context_new_context,
    //         ..
    //     } = generate_test_pipes_channel_context_mock();

    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new()
    //         .expect_add_workload(&workload_spec)
    //         .initial_workload_list_received()
    //         .build();

    //     let update_workload_result = to_manager
    //         .update_workload(vec![workload_spec.clone()], vec![])
    //         .await;
    //     assert!(update_workload_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);
    //     pipes_channel_context_new_context.checkpoint();

    //     let stored_runtime_name = agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&workload_spec.workload.name);

    //     assert!(stored_runtime_name.is_some());
    //     assert_eq!(stored_runtime_name.unwrap(), &workload_spec.runtime);
    // }

    // // [utest->swdd~agent-update-on-add-known-workload~1]
    // #[tokio::test]
    // async fn utest_agent_manager_update_workload_added_workload_after_server_restart() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_1_NAME.into(),
    //         RUNTIME_NAME.into(),
    //     );

    //     let PipesChannelContextMockData {
    //         pipes_channel_context_new_context,
    //         ..
    //     } = generate_test_pipes_channel_context_mock();

    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new()
    //         .expect_update_workload(&workload_spec)
    //         .initial_workload_list_received()
    //         .build();

    //     agent_manager
    //         .parameter_storage
    //         .set_workload_runtime(&workload_spec);

    //     let update_workload_result = to_manager
    //         .update_workload(vec![workload_spec.clone()], vec![])
    //         .await;
    //     assert!(update_workload_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);
    //     pipes_channel_context_new_context.checkpoint();

    //     let stored_runtime_name = agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&workload_spec.workload.name);

    //     assert!(stored_runtime_name.is_some());
    //     assert_eq!(stored_runtime_name.unwrap(), &workload_spec.runtime);
    // }

    // // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // // [utest->swdd~agent-uses-runtime-adapter~1]
    // // [utest->swdd~agent-uses-async-channels~1]
    // // [utest->swdd~agent-updates-deleted-and-added-workloads~1]
    // // [utest->swdd~agent-manager-stores-workload-runtime-mapping~1]
    // // [utest->swdd~agent-create-control-interface-pipes-per-workload~1]
    // #[tokio::test]
    // async fn utest_agent_manager_update_workload_updated_workloads_forwards_to_runtimes() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_1_NAME.into(),
    //         RUNTIME_NAME.into(),
    //     );

    //     let deleted_workload = DeletedWorkload {
    //         agent: AGENT_NAME.into(),
    //         name: WORKLOAD_1_NAME.into(),
    //         dependencies: HashMap::new(),
    //     };

    //     let PipesChannelContextMockData {
    //         pipes_channel_context_new_context,
    //         ..
    //     } = generate_test_pipes_channel_context_mock();

    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new()
    //         .expect_update_workload(&workload_spec)
    //         .initial_workload_list_received()
    //         .build();

    //     let update_workload_result = to_manager
    //         .update_workload(vec![workload_spec.clone()], vec![deleted_workload.clone()])
    //         .await;
    //     assert!(update_workload_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);
    //     pipes_channel_context_new_context.checkpoint();

    //     let stored_runtime_name = agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&workload_spec.workload.name);

    //     assert!(stored_runtime_name.is_some());
    //     assert_eq!(stored_runtime_name.unwrap(), &workload_spec.runtime);
    // }

    // // [utest->swdd~agent-manager-deletes-workload-runtime-mapping~1]
    // // [utest->swdd~agent-manager-forwards-delete-workload~2]
    // // [utest->swdd~agent-manager-deletes-control-interface~1]
    // #[tokio::test]
    // async fn utest_agent_manager_update_workload_delete_workloads_forwards_to_runtimes() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_TO_DELETE_NAME.into(),
    //         RUNTIME_NAME.into(),
    //     );

    //     let _keep_pipes_channel_context_mock_context = generate_test_pipes_channel_context_mock();

    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new()
    //         .expect_add_workload(&workload_spec)
    //         .expect_delete_workload(WORKLOAD_TO_DELETE_NAME)
    //         .initial_workload_list_received()
    //         .build();

    //     let update_added_workloads_result = to_manager
    //         .update_workload(vec![workload_spec.clone()], vec![])
    //         .await;
    //     assert!(update_added_workloads_result.is_ok());

    //     let update_deleted_workloads_result = to_manager
    //         .update_workload(
    //             vec![],
    //             vec![generate_test_deleted_workload(
    //                 AGENT_NAME.into(),
    //                 WORKLOAD_TO_DELETE_NAME.into(),
    //             )],
    //         )
    //         .await;
    //     assert!(update_deleted_workloads_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);

    //     let stored_runtime_name = agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&WORKLOAD_TO_DELETE_NAME.into());

    //     assert!(stored_runtime_name.is_none());
    //     assert_eq!(agent_manager.workload_pipes_context_map.len(), 0)
    // }

    // // [utest->swdd~agent-handle-deleted-before-added-workloads~1]
    // #[tokio::test]
    // async fn utest_agent_manager_update_workload_delete_before_add() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_1_NAME.into(),
    //         RUNTIME_NAME.into(),
    //     );

    //     let workload_spec_for_del = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_TO_DELETE_NAME.into(),
    //         RUNTIME_NAME.into(),
    //     );

    //     let _keep_pipes_channel_context_mock_context = generate_test_pipes_channel_context_mock();

    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new()
    //         .expect_add_workload(&workload_spec_for_del)
    //         .expect_delete_workload(WORKLOAD_TO_DELETE_NAME)
    //         .expect_add_workload(&workload_spec)
    //         .initial_workload_list_received()
    //         .build();

    //     let update_added_workloads_result = to_manager
    //         .update_workload(vec![workload_spec_for_del.clone()], vec![])
    //         .await;
    //     assert!(update_added_workloads_result.is_ok());

    //     let update_added_deleted_workloads_result = to_manager
    //         .update_workload(
    //             vec![workload_spec],
    //             vec![generate_test_deleted_workload(
    //                 AGENT_NAME.into(),
    //                 WORKLOAD_TO_DELETE_NAME.into(),
    //             )],
    //         )
    //         .await;
    //     assert!(update_added_deleted_workloads_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);

    //     assert!(agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&WORKLOAD_TO_DELETE_NAME.into())
    //         .is_none());
    //     assert!(agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&WORKLOAD_1_NAME.into())
    //         .is_some());
    //     assert_eq!(agent_manager.workload_pipes_context_map.len(), 1)
    // }

    // // [utest->swdd~agent-skips-unknown-runtime~1]
    // #[tokio::test]
    // async fn utest_agent_manager_update_workload_skips_unknown_runtimes() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let _keep_pipes_channel_context_mock_context = generate_test_pipes_channel_context_mock();
    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new()
    //         .initial_workload_list_received()
    //         .build();

    //     let delete_workload =
    //         generate_test_deleted_workload(AGENT_NAME.into(), "some name".to_string());

    //     let workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_1_NAME.into(),
    //         "no_such_runtime".into(),
    //     );

    //     let updated_workload_spec = generate_test_workload_spec_with_param(
    //         AGENT_NAME.into(),
    //         WORKLOAD_2_NAME.into(),
    //         "no_such_runtime".into(),
    //     );

    //     let updated_workload_spec_delete = DeletedWorkload {
    //         agent: AGENT_NAME.into(),
    //         name: WORKLOAD_2_NAME.into(),
    //         dependencies: HashMap::new(),
    //     };

    //     let update_workload_result = to_manager
    //         .update_workload(
    //             vec![workload_spec.clone(), updated_workload_spec],
    //             vec![delete_workload, updated_workload_spec_delete],
    //         )
    //         .await;
    //     assert!(update_workload_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);

    //     assert!(agent_manager
    //         .parameter_storage
    //         .get_workload_runtime(&workload_spec.workload.name)
    //         .is_none());
    // }

    // // [utest->swdd~agent-manager-stores-all-workload-states~1]
    // #[tokio::test]
    // async fn utest_agent_manager_stores_update_workload_state() {
    //     let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
    //         .get_lock_async()
    //         .await;
    //     let _ = env_logger::builder().is_test(true).try_init();

    //     let _keep_pipes_channel_context_mock_context = generate_test_pipes_channel_context_mock();
    //     let (to_manager, mut agent_manager) = AgentManagerBuilder::new()
    //         .initial_workload_list_received()
    //         .build();

    //     let workload_states = vec![WorkloadState {
    //         workload_name: WORKLOAD_1_NAME.into(),
    //         agent_name: AGENT_NAME.into(),
    //         execution_state: common::objects::ExecutionState::ExecFailed,
    //     }];
    //     let update_workload_state_result = to_manager.update_workload_state(workload_states).await;
    //     assert!(update_workload_state_result.is_ok());

    //     let handle = agent_manager.start();

    //     // The receiver in the agent receives the message and terminates the infinite waiting-loop.
    //     drop(to_manager);
    //     join!(handle);

    //     let states_storage = agent_manager
    //         .parameter_storage
    //         .get_workload_states(&AGENT_NAME.into());

    //     assert!(states_storage.is_some());
    //     assert_eq!(states_storage.unwrap().len(), 1);
    // }

    // fn generate_test_pipes_channel_context_mock() -> PipesChannelContextMockData {
    //     let pipes_channel_context_new_context = MockPipesChannelContext::new_context();
    //     let (workload1_sender, workload1_receiver) = tokio::sync::mpsc::channel(BUFFER_SIZE);
    //     let (workload2_sender, workload2_receiver) = tokio::sync::mpsc::channel(BUFFER_SIZE);
    //     let (workload_to_delete_sender, _) = tokio::sync::mpsc::channel(BUFFER_SIZE);

    //     struct PipesChannelContextMockBuilder(MockPipesChannelContext);

    //     impl PipesChannelContextMockBuilder {
    //         fn new() -> Self {
    //             let mut mock = MockPipesChannelContext::default();
    //             mock.expect_drop().return_const(());
    //             PipesChannelContextMockBuilder(mock)
    //         }

    //         fn with_name(mut self, name: &str) -> Self {
    //             self.0
    //                 .expect_get_api_location()
    //                 .return_const(format!("{API_PIPES_LOCATION}/{name}"));
    //             self
    //         }

    //         fn with_pipe_sender(mut self, sender: Sender<ExecutionCommand>) -> Self {
    //             self.0.expect_get_input_pipe_sender().return_const(sender);
    //             self
    //         }

    //         fn expect_abort(mut self) -> Self {
    //             self.0
    //                 .expect_abort_pipes_channel_task()
    //                 .times(1)
    //                 .return_const(());
    //             self
    //         }

    //         fn expect_no_abort(mut self) -> Self {
    //             self.0.expect_abort_pipes_channel_task().never();
    //             self
    //         }

    //         fn build(self) -> MockPipesChannelContext {
    //             self.0
    //         }
    //     }

    //     pipes_channel_context_new_context
    //         .expect()
    //         .with(
    //             eq(Path::new(API_PIPES_LOCATION).to_path_buf()),
    //             function(|x: &WorkloadExecutionInstanceName| x.workload_name() == WORKLOAD_1_NAME),
    //             always(),
    //         )
    //         .return_once(move |_, _, _| {
    //             Ok(PipesChannelContextMockBuilder::new()
    //                 .with_name(WORKLOAD_1_NAME)
    //                 .with_pipe_sender(workload1_sender)
    //                 .expect_no_abort()
    //                 .build())
    //         });

    //     pipes_channel_context_new_context
    //         .expect()
    //         .with(
    //             eq(Path::new(API_PIPES_LOCATION).to_path_buf()),
    //             function(|x: &WorkloadExecutionInstanceName| x.workload_name() == WORKLOAD_2_NAME),
    //             always(),
    //         )
    //         .return_once(move |_, _, _| {
    //             Ok(PipesChannelContextMockBuilder::new()
    //                 .with_name(WORKLOAD_2_NAME)
    //                 .with_pipe_sender(workload2_sender)
    //                 .expect_no_abort()
    //                 .build())
    //         });

    //     pipes_channel_context_new_context
    //         .expect()
    //         .with(
    //             eq(Path::new(API_PIPES_LOCATION).to_path_buf()),
    //             function(|x: &WorkloadExecutionInstanceName| {
    //                 x.workload_name() == WORKLOAD_TO_DELETE_NAME
    //             }),
    //             always(),
    //         )
    //         .return_once(move |_, _, _| {
    //             Ok(PipesChannelContextMockBuilder::new()
    //                 .with_name(WORKLOAD_TO_DELETE_NAME)
    //                 .with_pipe_sender(workload_to_delete_sender)
    //                 .expect_abort()
    //                 .build())
    //         });

    //     PipesChannelContextMockData {
    //         pipes_channel_context_new_context,
    //         workload1_receiver,
    //         workload2_receiver,
    //     }
    // }

    // struct PipesChannelContextMockData {
    //     pipes_channel_context_new_context:
    //         crate::control_interface::__mock_MockPipesChannelContext::__new::Context,
    //     workload1_receiver: ExecutionReceiver,
    //     workload2_receiver: ExecutionReceiver,
    // }

    // struct AgentManagerBuilder {
    //     runtime_adapter: MockRuntimeAdapter,
    //     initial_workload_list_received: bool,
    //     call_sequence: Sequence,
    // }

    // impl AgentManagerBuilder {
    //     fn new() -> Self {
    //         AgentManagerBuilder {
    //             runtime_adapter: MockRuntimeAdapter::default(),
    //             initial_workload_list_received: false,
    //             call_sequence: Sequence::new(),
    //         }
    //     }

    //     fn expect_start(
    //         mut self,
    //         agent_name: &str,
    //         initial_workload_list: Vec<WorkloadSpec>,
    //     ) -> Self {
    //         let agent_name = agent_name.to_string();
    //         self.runtime_adapter
    //             .expect_start()
    //             .times(1)
    //             .in_sequence(&mut self.call_sequence)
    //             .with(eq(agent_name), eq(initial_workload_list))
    //             .returning(|_, _| ());
    //         self
    //     }

    //     fn expect_add_workload(mut self, workload_spec: &WorkloadSpec) -> Self {
    //         self.runtime_adapter
    //             .expect_add_workload()
    //             .times(1)
    //             .in_sequence(&mut self.call_sequence)
    //             .with(eq(workload_spec.clone()))
    //             .returning(|_| ());
    //         self
    //     }

    //     fn expect_update_workload(mut self, workload_spec: &WorkloadSpec) -> Self {
    //         self.runtime_adapter
    //             .expect_update_workload()
    //             .times(1)
    //             .in_sequence(&mut self.call_sequence)
    //             .with(eq(workload_spec.clone()))
    //             .returning(|_| ());
    //         self
    //     }

    //     fn expect_delete_workload(mut self, workload_name: &str) -> Self {
    //         self.runtime_adapter
    //             .expect_delete_workload()
    //             .times(1)
    //             .in_sequence(&mut self.call_sequence)
    //             .with(eq(workload_name.to_string()))
    //             .returning(|_| ());
    //         self
    //     }

    //     fn initial_workload_list_received(mut self) -> Self {
    //         self.initial_workload_list_received = true;
    //         self
    //     }

    //     fn build(self) -> (Sender<ExecutionCommand>, AgentManager<'static>) {
    //         let (to_manager, manager_receiver) = mpsc::channel::<ExecutionCommand>(BUFFER_SIZE);

    //         let (to_server, _) = mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

    //         let mut adapter_map: HashMap<&'static str, Box<dyn RuntimeAdapter + Send + Sync>> =
    //             HashMap::new();
    //         adapter_map.insert(RUNTIME_NAME, Box::new(self.runtime_adapter));

    //         let mut agent_manager = AgentManager::new(
    //             AGENT_NAME.to_string(),
    //             manager_receiver,
    //             adapter_map,
    //             to_server,
    //             Path::new(API_PIPES_LOCATION).to_path_buf(),
    //         );
    //         agent_manager.initial_workload_list_received = self.initial_workload_list_received;

    //         (to_manager, agent_manager)
    //     }
    // }
}

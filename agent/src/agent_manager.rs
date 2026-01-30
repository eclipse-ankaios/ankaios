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

use crate::{subscription_store::SubscriptionStore, workload_state::WorkloadStateReceiver};

use ankaios_api::ank_base::{WorkloadStateSpec, WorkloadStatesMapSpec};
use common::std_extensions::{GracefulExitResult, IllegalStateResult};
use common::{
    commands::AgentLoadStatus,
    from_server_interface::{FromServer, FromServerReceiver},
    to_server_interface::{ToServerInterface, ToServerSender},
};

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
#[cfg_attr(test, mockall_double::double)]
use crate::workload_log_facade::WorkloadLogFacade;

const RESOURCE_MEASUREMENT_INTERVAL_TICK: std::time::Duration = tokio::time::Duration::from_secs(2);

#[cfg_attr(test, mockall_double::double)]
use crate::resource_monitor::ResourceMonitor;

pub type SynchronizedSubscriptionStore = std::sync::Arc<std::sync::Mutex<SubscriptionStore>>;

// [impl->swdd~agent-shall-use-interfaces-to-server~1]
pub struct AgentManager {
    agent_name: String,
    runtime_manager: RuntimeManager,
    // [impl->swdd~communication-to-from-agent-middleware~1]
    from_server_receiver: FromServerReceiver,
    to_server: ToServerSender,
    workload_state_receiver: WorkloadStateReceiver,
    workload_state_store: WorkloadStatesMapSpec,
    res_monitor: ResourceMonitor,
    subscription_store: SynchronizedSubscriptionStore,
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
            workload_state_store: WorkloadStatesMapSpec::new(),
            res_monitor: ResourceMonitor::new(),
            subscription_store: Default::default(),
        }
    }

    pub async fn start(&mut self) {
        log::info!("Awaiting commands from the server ...");

        let mut interval = tokio::time::interval(RESOURCE_MEASUREMENT_INTERVAL_TICK);

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
                },
                // [impl->swdd~agent-manager-receives-workload-states-of-its-workloads~1]
                workload_state = self.workload_state_receiver.recv() => {
                    let workload_state = workload_state
                        .ok_or("Channel to listen to own workload states closed.".to_string())
                        .unwrap_or_exit("Abort");
                    self.store_and_forward_own_workload_states(workload_state).await;
                }
                // [impl->swdd~agent-sends-node-resource-availability-to-server~1]
                _ = interval.tick() => {
                    self.measure_and_forward_resource_availability().await;
                }
            }
        }
    }

    // [impl->swdd~agent-manager-listens-requests-from-server~1]
    async fn execute_from_server_command(&mut self, from_server_msg: FromServer) -> Option<()> {
        log::debug!("Process command received from server.");

        match from_server_msg {
            FromServer::ServerHello(method_obj) => {
                log::debug!(
                    "Agent '{}' received ServerHello:\n\tAdded workloads: {:?}",
                    self.agent_name,
                    method_obj.added_workloads
                );

                self.runtime_manager
                    .handle_server_hello(method_obj.added_workloads, &self.workload_state_store)
                    .await;
                Some(())
            }
            FromServer::UpdateWorkload(method_obj) => {
                log::debug!(
                    "Agent '{}' received UpdateWorkload:\n\tAdded workloads: {:?}\n\tDeleted workloads: {:?}",
                    self.agent_name,
                    method_obj.added_workloads,
                    method_obj.deleted_workloads
                );

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
                        log::debug!(
                            "The server reports workload state '{:?}' for the workload '{}' in the agent '{}'",
                            new_workload_state.execution_state,
                            new_workload_state.instance_name.workload_name(),
                            new_workload_state.instance_name.agent_name()
                        );
                        self.workload_state_store
                            .process_new_states(vec![new_workload_state]);
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
            // [impl->swdd~agent-handles-logs-requests-from-server~1]
            FromServer::LogsRequest(request_id, logs_request) => {
                WorkloadLogFacade::spawn_log_collection(
                    request_id,
                    logs_request,
                    self.to_server.clone(),
                    self.subscription_store.clone(),
                    &self.runtime_manager,
                )
                .await;

                Some(())
            }
            // [impl->swdd~agent-handles-logs-cancel-requests-from-server~1]
            FromServer::LogsCancelRequest(request_id) => {
                log::debug!(
                    "Agent '{}' received LogsCancelRequest with id {}",
                    self.agent_name,
                    request_id
                );
                self.subscription_store
                    .lock()
                    .unwrap()
                    .delete_subscription(&request_id);
                Some(())
            }
            FromServer::ServerGone => {
                log::info!("Agent '{}' received ServerGone.", self.agent_name);

                // [impl->swdd~agent-deletes-all-log-subscription-entries-upon-server-gone~1]
                self.subscription_store
                    .lock()
                    .unwrap()
                    .delete_all_subscriptions();
                Some(())
            }
        }
    }

    async fn store_and_forward_own_workload_states(
        &mut self,
        mut new_workload_state: WorkloadStateSpec,
    ) {
        // execute hysteresis on the local workload states as we could be stopping
        // [impl->swdd~agent-manager-hysteresis_on-workload-states-of-its-workloads~1]
        if let Some(old_execution_state) = self
            .workload_state_store
            .get_workload_state_for_workload(&new_workload_state.instance_name)
        {
            new_workload_state.execution_state =
                old_execution_state.transition(new_workload_state.execution_state);
        }

        log::debug!("Storing and forwarding local workload state '{new_workload_state:?}'.");

        // [impl->swdd~agent-stores-workload-states-of-its-workloads~1]
        self.workload_state_store
            .process_new_states(vec![new_workload_state.clone()]);

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

    // [impl->swdd~agent-sends-node-resource-availability-to-server~1]
    async fn measure_and_forward_resource_availability(&mut self) {
        let (cpu_usage, free_memory) = self.res_monitor.sample_resource_usage();

        log::trace!(
            "Agent '{}' reports resource usage: CPU Usage: {}%, Free Memory: {}B",
            self.agent_name,
            cpu_usage.cpu_usage,
            free_memory.free_memory,
        );

        self.to_server
            .agent_load_status(AgentLoadStatus {
                agent_name: self.agent_name.clone(),
                cpu_usage,
                free_memory,
            })
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
    use super::RuntimeManager;
    use crate::agent_manager::AgentManager;
    use crate::resource_monitor::MockResourceMonitor;
    use crate::subscription_store::generate_test_subscription_entry;
    use crate::workload_log_facade::MockWorkloadLogFacade;
    use crate::workload_state::WorkloadStateSenderInterface;

    use ankaios_api::ank_base::{self, ExecutionStateSpec, LogsRequest};
    use ankaios_api::test_utils::{
        fixtures, generate_test_workload_named, generate_test_workload_named_with_params,
        generate_test_workload_state_with_agent,
    };
    use common::{
        commands::UpdateWorkloadState,
        from_server_interface::{FromServer, FromServerInterface},
        to_server_interface::ToServer,
    };

    use mockall::predicate::{self, eq};
    use tokio::{
        join,
        sync::mpsc::{Sender, channel},
    };

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    // [utest->swdd~agent-handles-update-workload-requests~1]
    #[tokio::test]
    async fn utest_agent_manager_update_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, _) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_handle_update_workload()
            .once()
            .return_const(());

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let workload_1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let workload_2 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let update_workload_result = to_manager
            .update_workload(vec![workload_1.clone(), workload_2.clone()], vec![])
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

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, _) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);

        let workload_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager.expect_handle_update_workload().never();
        mock_runtime_manager
            .expect_update_workloads_on_fulfilled_dependencies()
            .once()
            .return_const(());

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let expected_state = workload_state.clone();
        let handle = tokio::spawn(async move {
            agent_manager.start().await;
            assert_eq!(
                agent_manager
                    .workload_state_store
                    .get_workload_state_for_workload(&expected_state.instance_name),
                Some(&expected_state.execution_state)
            );
        });

        let update_workload_result = to_manager.update_workload_state(vec![workload_state]).await;
        assert!(update_workload_result.is_ok());

        // Terminate the infinite receiver loop
        to_manager.stop().await.unwrap();
        handle.await.unwrap();
    }

    // [utest->swdd~agent-manager-listens-requests-from-server~1]
    // [utest->swdd~agent-uses-async-channels~1]
    #[tokio::test]
    async fn utest_agent_manager_no_update_on_empty_workload_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, _) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager.expect_handle_update_workload().never();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
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

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, _) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);

        let request_id = format!("{}@{}", fixtures::WORKLOAD_NAMES[0], fixtures::REQUEST_ID);
        let complete_state: ank_base::CompleteState = Default::default();

        let response = ank_base::Response {
            request_id: request_id.clone(),
            response_content: Some(ank_base::response::ResponseContent::CompleteStateResponse(
                Box::new(ank_base::CompleteStateResponse {
                    complete_state: Some(complete_state.clone()),
                    ..Default::default()
                }),
            )),
        };

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_forward_response()
            .with(eq(response.clone()))
            .once()
            .return_const(());

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let complete_state_result = to_manager
            .complete_state(request_id, complete_state, None)
            .await;
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

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, mut to_server_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);

        let workload_state_incoming = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );

        let current_workload_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::stopping_requested(),
        );

        // Ensure that we send an updated state for the same workload instance
        assert_eq!(
            workload_state_incoming.instance_name,
            current_workload_state.instance_name
        );

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_update_workloads_on_fulfilled_dependencies()
            .once()
            .return_const(());

        let mut mock_resource_monitor = MockResourceMonitor::default();
        mock_resource_monitor
            .expect_sample_resource_usage()
            .returning(|| (fixtures::CPU_USAGE_SPEC, fixtures::FREE_MEMORY_SPEC));

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(|| mock_resource_monitor);

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        agent_manager
            .workload_state_store
            .process_new_states(vec![current_workload_state.clone()]);

        let expected_state = current_workload_state.clone();
        let handle = tokio::spawn(async move {
            agent_manager.start().await;
            assert_eq!(
                agent_manager
                    .workload_state_store
                    .get_workload_state_for_workload(&expected_state.instance_name),
                Some(&expected_state.execution_state)
            );
        });

        workload_state_sender
            .report_workload_execution_state(
                &workload_state_incoming.instance_name,
                workload_state_incoming.execution_state.clone(),
            )
            .await;

        let expected_workload_states = ToServer::UpdateWorkloadState(UpdateWorkloadState {
            workload_states: vec![current_workload_state],
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
        handle.await.unwrap();
    }

    // [utest->swdd~agent-sends-node-resource-availability-to-server~1]
    #[tokio::test]
    async fn utest_agent_manager_sends_available_resources() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, mut server_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager.expect_handle_update_workload().never();
        mock_runtime_manager.expect_forward_response().never();
        mock_runtime_manager.expect_execute_workloads().never();
        mock_runtime_manager.expect_handle_server_hello().never();
        mock_runtime_manager
            .expect_update_workloads_on_fulfilled_dependencies()
            .never();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(|| {
                let mut mock_resource_monitor = MockResourceMonitor::default();
                mock_resource_monitor
                    .expect_sample_resource_usage()
                    .returning(|| (fixtures::CPU_USAGE_SPEC, fixtures::FREE_MEMORY_SPEC));
                mock_resource_monitor
            });

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let result = server_receiver.recv().await.unwrap();
        if let ToServer::AgentLoadStatus(load_status) = result {
            assert_eq!(load_status.agent_name, fixtures::AGENT_NAMES[0].to_string());
            assert_eq!(load_status.cpu_usage.cpu_usage, 50);
            assert_eq!(load_status.free_memory.free_memory, 1024);
        } else {
            panic!("Expected AgentLoadStatus, got something else");
        }

        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    // [utest->swdd~agent-handles-logs-requests-from-server~1]
    #[tokio::test]
    async fn utest_agent_manager_request_logs() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, _server_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);

        let workload = generate_test_workload_named();

        let mock_runtime_manager = RuntimeManager::default();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let logs_request = LogsRequest {
            workload_names: vec![workload.instance_name.into()],
            ..Default::default()
        };

        let to_server_clone = to_server.clone();
        let mock_workload_log_facade = MockWorkloadLogFacade::spawn_log_collection_context();
        mock_workload_log_facade
            .expect()
            .once()
            .with(
                predicate::eq(fixtures::REQUEST_ID.to_string()),
                predicate::eq(logs_request.clone()),
                predicate::function(move |to_server_sender: &Sender<ToServer>| {
                    to_server_sender.same_channel(&to_server_clone)
                }),
                predicate::always(),
                predicate::always(),
            )
            .return_const(());

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        assert!(
            to_manager
                .logs_request(fixtures::REQUEST_ID.to_string(), logs_request.clone())
                .await
                .is_ok()
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    // [utest->swdd~agent-handles-logs-cancel-requests-from-server~1]
    #[tokio::test]
    async fn utest_agent_manager_logs_cancel_request() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, _server_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);

        let mock_runtime_manager = RuntimeManager::default();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let subscription_store = agent_manager.subscription_store.clone();
        subscription_store.lock().unwrap().add_subscription(
            fixtures::REQUEST_ID.to_string(),
            generate_test_subscription_entry(),
        );

        assert!(
            to_manager
                .logs_cancel_request(fixtures::REQUEST_ID.to_string())
                .await
                .is_ok()
        );

        to_manager.stop().await.unwrap();
        agent_manager.start().await;
        assert!(subscription_store.lock().unwrap().is_empty());
    }

    // [utest->swdd~agent-deletes-all-log-subscription-entries-upon-server-gone~1]
    #[tokio::test]
    async fn utest_agent_manager_server_gone_delete_all_subscription_store_entries() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_manager, manager_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (to_server, _server_receiver) = channel(fixtures::TEST_CHANNEL_CAP);
        let (_workload_state_sender, workload_state_receiver) = channel(fixtures::TEST_CHANNEL_CAP);

        let mock_runtime_manager = RuntimeManager::default();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            fixtures::AGENT_NAMES[0].to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        agent_manager
            .subscription_store
            .lock()
            .unwrap()
            .add_subscription(
                fixtures::REQUEST_ID.to_string(),
                generate_test_subscription_entry(),
            );

        assert!(to_manager.send(FromServer::ServerGone).await.is_ok());
        to_manager.stop().await.unwrap();

        agent_manager.start().await;

        assert!(agent_manager.subscription_store.lock().unwrap().is_empty());
    }
}

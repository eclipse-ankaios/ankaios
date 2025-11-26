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

use ankaios_api::ank_base::WorkloadStateSpec;
use common::std_extensions::{GracefulExitResult, IllegalStateResult};
use common::{
    commands::AgentLoadStatus,
    from_server_interface::{FromServer, FromServerReceiver},
    to_server_interface::{ToServerInterface, ToServerSender},
};

#[cfg_attr(test, mockall_double::double)]
use crate::workload_state::workload_state_store::WorkloadStateStore;

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
    workload_state_store: WorkloadStateStore,
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
            workload_state_store: WorkloadStateStore::new(),
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
            .get_state_of_workload(new_workload_state.instance_name.workload_name())
        {
            new_workload_state.execution_state =
                old_execution_state.transition(new_workload_state.execution_state);
        }

        log::debug!("Storing and forwarding local workload state '{new_workload_state:?}'.");

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
    use crate::workload_state::{
        WorkloadStateSenderInterface,
        workload_state_store::{MockWorkloadStateStore, mock_parameter_storage_new_returns},
    };

    use ankaios_api::ank_base::{
        self, CpuUsageSpec, ExecutionStateSpec, FreeMemorySpec, LogsRequestSpec, WorkloadNamed,
    };
    use ankaios_api::test_utils::{
        generate_test_workload_state_with_agent, generate_test_workload_with_param,
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

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let workload_1 =
            generate_test_workload_with_param::<WorkloadNamed>(AGENT_NAME, RUNTIME_NAME)
                .name(WORKLOAD_1_NAME);

        let workload_2 =
            generate_test_workload_with_param::<WorkloadNamed>(AGENT_NAME, RUNTIME_NAME)
                .name(WORKLOAD_2_NAME);

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

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let workload_state = generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_NAME,
            ExecutionStateSpec::running(),
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

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

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

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

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
            AGENT_NAME.to_string(),
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

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, mut to_server_receiver) = channel(BUFFER_SIZE);
        let (workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let workload_state_incoming = generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_NAME,
            ExecutionStateSpec::running(),
        );

        let wl_state_after_hysteresis = generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_NAME,
            ExecutionStateSpec::stopping_requested(),
        );

        let mut mock_wl_state_store = MockWorkloadStateStore::default();

        mock_wl_state_store.states_storage.insert(
            WORKLOAD_1_NAME.to_string(),
            ExecutionStateSpec::stopping_requested(),
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

        let mut mock_resource_monitor = MockResourceMonitor::default();
        mock_resource_monitor
            .expect_sample_resource_usage()
            .returning(|| {
                (
                    CpuUsageSpec { cpu_usage: 50 },
                    FreeMemorySpec { free_memory: 1024 },
                )
            });

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(|| mock_resource_monitor);

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

    // [utest->swdd~agent-sends-node-resource-availability-to-server~1]
    #[tokio::test]
    async fn utest_agent_manager_sends_available_resources() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mock_wl_state_store = MockWorkloadStateStore::default();
        mock_parameter_storage_new_returns(mock_wl_state_store);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, mut server_receiver) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);
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
                    .returning(|| {
                        (
                            CpuUsageSpec { cpu_usage: 50 },
                            FreeMemorySpec { free_memory: 1024 },
                        )
                    });
                mock_resource_monitor
            });

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move { agent_manager.start().await });

        let result = server_receiver.recv().await.unwrap();
        if let ToServer::AgentLoadStatus(load_status) = result {
            assert_eq!(load_status.agent_name, AGENT_NAME.to_string());
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

        let mock_wl_state_store = MockWorkloadStateStore::default();
        mock_parameter_storage_new_returns(mock_wl_state_store);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _server_receiver) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let workload: WorkloadNamed = generate_test_workload_with_param(AGENT_NAME, RUNTIME_NAME);

        let mock_runtime_manager = RuntimeManager::default();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let logs_request = LogsRequestSpec {
            workload_names: vec![workload.instance_name],
            follow: false,
            tail: -1,
            since: None,
            until: None,
        };

        let to_server_clone = to_server.clone();
        let mock_workload_log_facade = MockWorkloadLogFacade::spawn_log_collection_context();
        mock_workload_log_facade
            .expect()
            .once()
            .with(
                predicate::eq(REQUEST_ID.to_string()),
                predicate::eq(logs_request.clone()),
                predicate::function(move |to_server_sender: &Sender<ToServer>| {
                    to_server_sender.same_channel(&to_server_clone)
                }),
                predicate::always(),
                predicate::always(),
            )
            .return_const(());

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        assert!(
            to_manager
                .logs_request(
                    REQUEST_ID.to_string(),
                    ank_base::LogsRequest {
                        workload_names: vec![ank_base::WorkloadInstanceName {
                            workload_name: logs_request.workload_names[0]
                                .workload_name()
                                .to_string(),
                            agent_name: logs_request.workload_names[0].agent_name().to_string(),
                            id: logs_request.workload_names[0].id().to_string()
                        }],
                        follow: Some(logs_request.follow),
                        tail: Some(logs_request.tail),
                        since: logs_request.since,
                        until: logs_request.until,
                    }
                )
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

        let mock_wl_state_store = MockWorkloadStateStore::default();
        mock_parameter_storage_new_returns(mock_wl_state_store);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _server_receiver) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let mock_runtime_manager = RuntimeManager::default();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let subscription_store = agent_manager.subscription_store.clone();
        subscription_store
            .lock()
            .unwrap()
            .add_subscription(REQUEST_ID.to_string(), generate_test_subscription_entry());

        assert!(
            to_manager
                .logs_cancel_request(REQUEST_ID.to_string())
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

        let mock_wl_state_store = MockWorkloadStateStore::default();
        mock_parameter_storage_new_returns(mock_wl_state_store);

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, _server_receiver) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let mock_runtime_manager = RuntimeManager::default();

        let mock_resource_monitor_context = MockResourceMonitor::new_context();
        mock_resource_monitor_context
            .expect()
            .once()
            .return_once(MockResourceMonitor::default);

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        agent_manager
            .subscription_store
            .lock()
            .unwrap()
            .add_subscription(REQUEST_ID.to_string(), generate_test_subscription_entry());

        assert!(to_manager.send(FromServer::ServerGone).await.is_ok());
        to_manager.stop().await.unwrap();

        agent_manager.start().await;

        assert!(agent_manager.subscription_store.lock().unwrap().is_empty());
    }
}

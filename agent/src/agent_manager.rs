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
use std::{future::Future, pin::Pin};

use api::ank_base;
use futures_util::{stream::FuturesUnordered, StreamExt};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

use common::{
    commands::AgentLoadStatus,
    from_server_interface::{FromServer, FromServerReceiver},
    objects::{CpuUsage, FreeMemory, WorkloadInstanceName, WorkloadState},
    std_extensions::{GracefulExitResult, IllegalStateResult},
    to_server_interface::{ToServerInterface, ToServerSender},
};

#[cfg(test)]
use tests::spawn;
#[cfg(not(test))]
use tokio::spawn;

#[cfg_attr(test, mockall_double::double)]
use crate::workload_state::workload_state_store::WorkloadStateStore;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
use crate::{subscription_store::SubscriptionStore, workload_state::WorkloadStateReceiver};

#[cfg(not(test))]
use crate::runtime_connectors::log_channel::Receiver;
#[cfg(test)]
use tests::MockRuntimeConnectorReceiver as Receiver;

#[cfg(not(test))]
use crate::runtime_connectors::log_collector_subscription::LogCollectorSubscription;
#[cfg(test)]
use tests::MockLogCollectorSubscription as LogCollectorSubscription;

const RESOURCE_MEASUREMENT_INTERVAL_TICK: std::time::Duration = tokio::time::Duration::from_secs(2);

struct ResourceMonitor {
    refresh_kind: RefreshKind,
    sys: System,
}

impl ResourceMonitor {
    fn new() -> ResourceMonitor {
        let refresh_kind = RefreshKind::new()
            .with_cpu(CpuRefreshKind::new().with_cpu_usage())
            .with_memory(MemoryRefreshKind::new().with_ram());
        ResourceMonitor {
            refresh_kind,
            sys: System::new_with_specifics(refresh_kind),
        }
    }

    fn sample_resource_usage(&mut self) -> (CpuUsage, FreeMemory) {
        self.sys.refresh_specifics(self.refresh_kind);

        let cpu_usage = self.sys.global_cpu_usage();
        let free_memory = self.sys.free_memory();

        (CpuUsage::new(cpu_usage), FreeMemory { free_memory })
    }
}

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
    subscription_store: SubscriptionStore,
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
            FromServer::LogsRequest(request_id, logs_request) => {
                let (names, log_collectors): (Vec<_>, _) = self
                    .runtime_manager
                    .get_logs(logs_request)
                    .await
                    .into_iter()
                    .unzip();
                let (subscription, receivers) =
                    LogCollectorSubscription::start_collecting_logs(log_collectors);
                let receivers = names.into_iter().zip(receivers).collect::<Vec<_>>();
                self.subscription_store
                    .add_subscription(request_id.clone(), subscription);
                let to_server = self.to_server.clone();
                spawn(async move {
                    type ContinuableResult = (WorkloadInstanceName, Receiver, Option<Vec<String>>);
                    let mut futures = FuturesUnordered::from_iter(receivers.into_iter().map(
                        |mut x| -> Pin<Box<dyn Future<Output = ContinuableResult> + Send>> {
                            Box::pin(async {
                                let n = x.1.read_log_lines().await;
                                (x.0, x.1, n)
                            })
                        },
                    ));
                    while let Some((workload, mut receiver, log_lines)) = futures.next().await {
                        log::debug!("Got new log lines: {:?}", log_lines);
                        if let Some(log_lines) = log_lines {
                            to_server
                                .logs_response(
                                    request_id.clone(),
                                    ank_base::LogEntriesResponse {
                                        log_entries: log_lines
                                            .into_iter()
                                            .map(|x| ank_base::LogEntry {
                                                workload_name: Some(workload.clone().into()),
                                                message: x,
                                            })
                                            .collect(),
                                    },
                                )
                                .await
                                .unwrap_or_illegal_state();
                            let x = async move {
                                let n = receiver.read_log_lines().await;
                                (workload, receiver, n)
                            };
                            futures.push(Box::pin(x));
                        } else {
                            log::debug!(
                                "No more log lines received for workload '{}', stop sending logs.",
                                workload
                            );
                            to_server
                                .logs_stop_response(
                                    request_id.clone(),
                                    ank_base::LogsStopResponse {
                                        workload_name: Some(workload.into()),
                                    },
                                )
                                .await
                                .unwrap_or_illegal_state();
                        }
                    }
                });
                Some(())
            }
            FromServer::LogsCancelRequest(request_id) => {
                log::debug!(
                    "Agent '{}' received LogsCancelRequest with id {}",
                    self.agent_name,
                    request_id
                );
                self.subscription_store.delete_subscription(&request_id);
                Some(())
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
    use std::collections::HashMap;
    use std::future::Future;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::RuntimeManager;
    use crate::agent_manager::AgentManager;
    use crate::runtime_connectors::log_collector::MockLogCollector;
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
    use lazy_static::lazy_static;
    use mockall::mock;
    use mockall::predicate::{self, eq};

    use tokio::sync::mpsc;
    use tokio::task::JoinHandle;
    use tokio::time::timeout;
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
            assert_ne!(load_status.cpu_usage.cpu_usage, 0);
        } else {
            panic!("Expected AgentLoadStatus, got something else");
        }

        to_manager.stop().await.unwrap();
        assert!(join!(handle).0.is_ok());
    }

    #[tokio::test]
    async fn utest_agent_manager_request_logs() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        reset_spawn_mock();

        let (to_manager, manager_receiver) = channel(BUFFER_SIZE);
        let (to_server, mut to_server_receiver) = channel(BUFFER_SIZE);
        let (_workload_state_sender, workload_state_receiver) = channel(BUFFER_SIZE);

        let mock_wl_state_store = MockWorkloadStateStore::default();

        mock_parameter_storage_new_returns(mock_wl_state_store);

        let mock_log_collector_1 = MockLogCollector::new();
        let mock_log_collector_2 = MockLogCollector::new();

        let mut mock_runtime_connector_receiver_1 = MockRuntimeConnectorReceiver::new();
        let mut mock_runtime_connector_receiver_2 = MockRuntimeConnectorReceiver::new();

        mock_runtime_connector_receiver_1
            .expect_read_log_lines()
            .once()
            .return_once(|| Some(vec!["rec1: line1".into(), "rec1: line2".into()]));
        mock_runtime_connector_receiver_2
            .expect_read_log_lines()
            .once()
            .return_once(|| Some(vec!["rec2: line1".into()]));
        mock_runtime_connector_receiver_2
            .expect_read_log_lines()
            .once()
            .return_once(|| None);
        mock_runtime_connector_receiver_1
            .expect_read_log_lines()
            .once()
            .return_once(|| Some(vec!["rec1: line3".into()]));
        mock_runtime_connector_receiver_1
            .expect_read_log_lines()
            .once()
            .return_once(|| None);

        let mut mock_log_collector_subscription = MockLogCollectorSubscription::new();
        let mock_log_collector_subscription_dropped = Arc::new(Mutex::new(false));
        let mock_log_collector_subscription_dropped_clone =
            mock_log_collector_subscription_dropped.clone();
        mock_log_collector_subscription
            .expect_drop()
            .returning(move || {
                *mock_log_collector_subscription_dropped_clone
                    .lock()
                    .unwrap() = true;
            });

        let collecting_logs_context = MockLogCollectorSubscription::start_collecting_logs_context();
        collecting_logs_context.expect().return_once(|_| {
            (
                mock_log_collector_subscription,
                vec![
                    mock_runtime_connector_receiver_1,
                    mock_runtime_connector_receiver_2,
                ],
            )
        });

        let workload_instance_name_1 = ank_base::WorkloadInstanceName {
            workload_name: WORKLOAD_1_NAME.into(),
            agent_name: AGENT_NAME.into(),
            id: "1234".into(),
        };

        let workload_instance_name_2 = ank_base::WorkloadInstanceName {
            workload_name: WORKLOAD_2_NAME.into(),
            agent_name: AGENT_NAME.into(),
            id: "1234".into(),
        };

        let logs_request = ank_base::LogsRequest {
            workload_names: vec![
                workload_instance_name_1.clone(),
                workload_instance_name_2.clone(),
            ],
            follow: None,
            tail: None,
            since: None,
            until: None,
        };

        let mut mock_runtime_manager = RuntimeManager::default();
        mock_runtime_manager
            .expect_get_logs()
            .with(predicate::eq(common::commands::LogsRequest::from(
                logs_request.clone(),
            )))
            .return_once(|_| {
                vec![
                    (
                        workload_instance_name_1.into(),
                        Box::new(mock_log_collector_1),
                    ),
                    (
                        workload_instance_name_2.into(),
                        Box::new(mock_log_collector_2),
                    ),
                ]
            });

        let mut agent_manager = AgentManager::new(
            AGENT_NAME.to_string(),
            manager_receiver,
            mock_runtime_manager,
            to_server,
            workload_state_receiver,
        );

        let handle = tokio::spawn(async move {
            agent_manager.start().await;
        });

        to_manager
            .logs_request(REQUEST_ID.into(), logs_request)
            .await
            .unwrap();

        let log_responses = get_log_responses(3, &mut to_server_receiver).await.unwrap();

        assert_eq!(log_responses.len(), 2);
        assert!(log_responses.contains_key(&(REQUEST_ID.into(), WORKLOAD_1_NAME.into())));
        assert_eq!(
            log_responses
                .get(&(REQUEST_ID.into(), WORKLOAD_1_NAME.into()))
                .unwrap(),
            &vec![
                "rec1: line1".to_string(),
                "rec1: line2".to_string(),
                "rec1: line3".to_string(),
            ]
        );
        assert!(log_responses.contains_key(&(REQUEST_ID.into(), WORKLOAD_2_NAME.into())));
        assert_eq!(
            log_responses
                .get(&(REQUEST_ID.into(), WORKLOAD_2_NAME.into()))
                .unwrap(),
            &vec!["rec2: line1".to_string(),]
        );

        let log_responses = timeout(
            Duration::from_millis(10),
            get_log_responses(1, &mut to_server_receiver),
        )
        .await;
        assert!(log_responses.is_err());

        assert!(!*mock_log_collector_subscription_dropped.lock().unwrap());
        to_manager
            .logs_cancel_request(REQUEST_ID.into())
            .await
            .unwrap();
        let log_responses = timeout(
            Duration::from_millis(10),
            get_log_responses(1, &mut to_server_receiver),
        )
        .await;
        assert!(log_responses.is_err());
        assert!(*mock_log_collector_subscription_dropped.lock().unwrap());

        assert!(spawn_mock_task_is_finished());
        to_manager.stop().await.unwrap();
        tokio::time::timeout(Duration::from_millis(1000), handle)
            .await
            .unwrap()
            .unwrap();
    }

    async fn get_log_responses(
        num: usize,
        to_server: &mut mpsc::Receiver<ToServer>,
    ) -> Option<HashMap<(String, String), Vec<String>>> {
        let mut result: HashMap<(String, String), Vec<String>> = HashMap::new();
        let mut responses = 0;
        while responses != num {
            let candidate = to_server.recv().await?;
            if let ToServer::LogEntriesResponse(request_id, logs_response) = candidate {
                responses += 1;
                for entry in logs_response.log_entries {
                    result
                        .entry((
                            request_id.clone(),
                            entry.workload_name.unwrap().workload_name,
                        ))
                        .or_default()
                        .push(entry.message);
                }
            };
        }
        Some(result)
    }

    lazy_static! {
        static ref SPAWN_JOIN_HANDLE: Mutex<BoxedJoinHandle> = Mutex::new(None);
    }

    type BoxedJoinHandle = Option<Box<dyn TypelessJoinHandle>>;

    trait TypelessJoinHandle: Send + Sync {
        fn is_finished(&mut self) -> bool;
    }

    impl<T: Send> TypelessJoinHandle for JoinHandle<T> {
        fn is_finished(&mut self) -> bool {
            JoinHandle::is_finished(self)
        }
    }

    pub fn spawn<F>(future: F)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let jh = tokio::spawn(future);
        let x = Box::new(jh);
        SPAWN_JOIN_HANDLE.lock().unwrap().replace(x);
    }

    pub fn reset_spawn_mock() {
        SPAWN_JOIN_HANDLE.lock().unwrap().take();
    }

    pub fn spawn_mock_task_is_finished() -> bool {
        let jh = SPAWN_JOIN_HANDLE.lock().unwrap().take();
        match jh {
            Some(mut jh) => jh.is_finished(),
            None => panic!("The function spawn was not called."),
        }
    }

    mock! {
        pub LogCollectorSubscription {
            pub fn start_collecting_logs(log_collectors: Vec<Box<dyn crate::runtime_connectors::log_collector::LogCollector>>) -> (Self, Vec<MockRuntimeConnectorReceiver>);
        }

        impl Drop for LogCollectorSubscription {
            fn drop(&mut self);
        }
    }

    mock! {
        pub RuntimeConnectorReceiver {
            pub async fn read_log_lines(&mut self) -> Option<Vec<String>>;
        }
    }
}

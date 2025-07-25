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

mod config_renderer;
mod cycle_check;
mod delete_graph;
mod log_campaign_store;
mod server_state;

use api::ank_base;
use common::commands::{Request, UpdateWorkload};
use common::from_server_interface::{FromServerReceiver, FromServerSender};
use common::objects::{
    CompleteState, DeletedWorkload, ExecutionState, State, WorkloadInstanceName, WorkloadState,
    WorkloadStatesMap,
};

use common::std_extensions::IllegalStateResult;
use common::to_server_interface::{ToServerReceiver, ToServerSender};

#[cfg_attr(test, mockall_double::double)]
use server_state::ServerState;

use common::{
    from_server_interface::{FromServer, FromServerInterface},
    to_server_interface::ToServer,
};

use tokio::sync::mpsc::channel;

pub type ToServerChannel = (ToServerSender, ToServerReceiver);
pub type FromServerChannel = (FromServerSender, FromServerReceiver);

pub fn create_to_server_channel(capacity: usize) -> ToServerChannel {
    channel::<ToServer>(capacity)
}
pub fn create_from_server_channel(capacity: usize) -> FromServerChannel {
    channel::<FromServer>(capacity)
}

#[cfg_attr(test, mockall_double::double)]
use log_campaign_store::LogCampaignStore;

use log_campaign_store::LogCollectorRequestId;

use std::collections::HashSet;

pub struct AnkaiosServer {
    // [impl->swdd~server-uses-async-channels~1]
    receiver: ToServerReceiver,
    // [impl->swdd~communication-to-from-server-middleware~1]
    to_agents: FromServerSender,
    server_state: ServerState,
    workload_states_map: WorkloadStatesMap,
    log_campaign_store: LogCampaignStore,
}

impl AnkaiosServer {
    pub fn new(receiver: ToServerReceiver, to_agents: FromServerSender) -> Self {
        AnkaiosServer {
            receiver,
            to_agents,
            server_state: ServerState::default(),
            workload_states_map: WorkloadStatesMap::default(),
            log_campaign_store: LogCampaignStore::default(),
        }
    }

    pub async fn start(&mut self, startup_state: Option<CompleteState>) -> Result<(), String> {
        if let Some(state) = startup_state {
            State::verify_api_version(&state.desired_state)?;

            match self.server_state.update(state, vec![]) {
                Ok(Some((added_workloads, deleted_workloads))) => {
                    // [impl->swdd~server-sets-state-of-new-workloads-to-pending~1]
                    self.workload_states_map.initial_state(&added_workloads);

                    let from_server_command = FromServer::UpdateWorkload(UpdateWorkload {
                        added_workloads,
                        deleted_workloads,
                    });
                    log::info!("Starting...");
                    self.to_agents
                        .send(from_server_command)
                        .await
                        .unwrap_or_illegal_state();
                }
                Ok(None) => log::info!("No initial workloads to send to agents."),
                Err(err) => {
                    // [impl->swdd~server-fails-on-invalid-startup-state~1]
                    return Err(err.to_string());
                }
            }
        } else {
            // [impl->swdd~server-starts-without-startup-config~1]
            log::info!("No startup manifest provided -> waiting for new workloads from the CLI");
        }
        self.listen_to_agents().await;
        Ok(())
    }

    async fn listen_to_agents(&mut self) {
        log::debug!("Start listening to agents...");
        while let Some(to_server_command) = self.receiver.recv().await {
            match to_server_command {
                ToServer::AgentHello(method_obj) => {
                    log::info!("Received AgentHello from '{}'", method_obj.agent_name);

                    let agent_name = method_obj.agent_name;

                    // [impl->swdd~server-informs-a-newly-connected-agent-workload-states~1]
                    let workload_states = self
                        .workload_states_map
                        .get_workload_state_excluding_agent(&agent_name);

                    if !workload_states.is_empty() {
                        log::debug!(
                            "Sending initial UpdateWorkloadState to agent '{}' with workload states: '{:?}'",
                            agent_name,
                            workload_states,
                        );

                        self.to_agents
                            .update_workload_state(workload_states)
                            .await
                            .unwrap_or_illegal_state();
                    } else {
                        log::debug!("No workload states to send.");
                    }

                    // Send this agent all workloads in the current state which are assigned to him
                    // [impl->swdd~agent-from-agent-field~1]
                    let added_workloads = self.server_state.get_workloads_for_agent(&agent_name);

                    log::debug!(
                        "Sending initial ServerHello to agent '{}' with added workloads: '{:?}'",
                        agent_name,
                        added_workloads,
                    );

                    // [impl->swdd~server-sends-all-workloads-on-start~2]
                    self.to_agents
                        .server_hello(Some(agent_name.clone()), added_workloads)
                        .await
                        .unwrap_or_illegal_state();

                    // [impl->swdd~server-stores-newly-connected-agent~1]
                    self.server_state.add_agent(agent_name);
                }
                // [impl->swdd~server-receives-resource-availability~1]
                ToServer::AgentLoadStatus(method_obj) => {
                    log::trace!(
                        "Received load status from agent '{}': CPU usage: {}%, Free Memory: {}B",
                        method_obj.agent_name,
                        method_obj.cpu_usage.cpu_usage,
                        method_obj.free_memory.free_memory,
                    );

                    self.server_state
                        .update_agent_resource_availability(method_obj);
                }
                ToServer::AgentGone(method_obj) => {
                    log::debug!("Received AgentGone from '{}'", method_obj.agent_name);
                    let agent_name = method_obj.agent_name;

                    // [impl->swdd~server-removes-disconnected-agents-from-state~1]
                    self.server_state.remove_agent(&agent_name);

                    // [impl->swdd~server-set-workload-state-on-disconnect~1]
                    self.workload_states_map.agent_disconnected(&agent_name);

                    // communicate the workload execution states to other agents
                    // [impl->swdd~server-distribute-workload-state-on-disconnect~1]
                    self.to_agents
                        .update_workload_state(
                            self.workload_states_map
                                .get_workload_state_for_agent(&agent_name),
                        )
                        .await
                        .unwrap_or_illegal_state();

                    // [impl->swdd~server-handles-log-campaign-for-disconnected-agent~1]
                    let removed_log_requests = self
                        .log_campaign_store
                        .remove_agent_log_campaign_entry(&agent_name);

                    self.cancel_log_requests_of_disconnected_collector(
                        &agent_name,
                        removed_log_requests.collector_requests,
                    )
                    .await;

                    self.send_log_stop_response_for_disconnected_agent(
                        removed_log_requests.disconnected_log_providers,
                    )
                    .await;
                }
                // [impl->swdd~server-provides-update-desired-state-interface~1]
                ToServer::Request(Request {
                    request_id,
                    request_content,
                }) => match request_content {
                    // [impl->swdd~server-provides-interface-get-complete-state~2]
                    // [impl->swdd~server-includes-id-in-control-interface-response~1]
                    common::commands::RequestContent::CompleteStateRequest(
                        complete_state_request,
                    ) => {
                        log::debug!(
                            "Received CompleteStateRequest with id '{}' and field mask: '{:?}'",
                            request_id,
                            complete_state_request.field_mask
                        );
                        match self.server_state.get_complete_state_by_field_mask(
                            complete_state_request,
                            &self.workload_states_map,
                        ) {
                            Ok(complete_state) => self
                                .to_agents
                                .complete_state(request_id, complete_state)
                                .await
                                .unwrap_or_illegal_state(),
                            Err(error) => {
                                log::error!("Failed to get complete state: '{}'", error);
                                self.to_agents
                                    .complete_state(
                                        request_id,
                                        ank_base::CompleteState {
                                            ..Default::default()
                                        },
                                    )
                                    .await
                                    .unwrap_or_illegal_state();
                            }
                        }
                    }

                    // [impl->swdd~server-provides-update-desired-state-interface~1]
                    common::commands::RequestContent::UpdateStateRequest(update_state_request) => {
                        log::debug!(
                            "Received UpdateState. State '{:?}', update mask '{:?}'",
                            update_state_request.state,
                            update_state_request.update_mask
                        );

                        // [impl->swdd~update-desired-state-with-invalid-version~1]
                        // [impl->swdd~update-desired-state-with-missing-version~1]
                        // [impl->swdd~server-desired-state-field-conventions~1]
                        let updated_desired_state = &update_state_request.state.desired_state;
                        if let Err(error_message) = State::verify_api_version(updated_desired_state)
                            .and_then(|_| State::verify_configs_format(updated_desired_state))
                        {
                            log::warn!("The CompleteState in the request has wrong format. {} -> ignoring the request", error_message);

                            self.to_agents
                                .error(request_id, error_message)
                                .await
                                .unwrap_or_illegal_state();
                            continue;
                        }

                        // [impl->swdd~update-desired-state-with-update-mask~1]
                        // [impl->swdd~update-desired-state-empty-update-mask~1]
                        match self
                            .server_state
                            .update(update_state_request.state, update_state_request.update_mask)
                        {
                            Ok(Some((added_workloads, deleted_workloads))) => {
                                log::info!(
                                        "The update has {} new or updated workloads, {} workloads to delete",
                                        added_workloads.len(),
                                        deleted_workloads.len()
                                    );

                                // [impl->swdd~server-sets-state-of-new-workloads-to-pending~1]
                                self.workload_states_map.initial_state(&added_workloads);

                                let added_workloads_names = added_workloads
                                    .iter()
                                    .map(|x| x.instance_name.to_string())
                                    .collect();
                                let deleted_workloads_names = deleted_workloads
                                    .iter()
                                    .map(|x| x.instance_name.to_string())
                                    .collect();

                                // [impl->swdd~server-cancels-log-campaign-for-deleted-workloads~1]
                                self.cancel_log_requests_of_deleted_workloads(&deleted_workloads)
                                    .await;

                                // [impl->swdd~server-handles-not-started-deleted-workloads~1]
                                let retained_deleted_workloads = self
                                    .handle_not_started_deleted_workloads(deleted_workloads)
                                    .await;

                                let from_server_command =
                                    FromServer::UpdateWorkload(UpdateWorkload {
                                        added_workloads,
                                        deleted_workloads: retained_deleted_workloads,
                                    });
                                self.to_agents
                                    .send(from_server_command)
                                    .await
                                    .unwrap_or_illegal_state();
                                log::debug!("Send UpdateStateSuccess for request '{}'", request_id);
                                // [impl->swdd~server-update-state-success-response~1]
                                self.to_agents
                                    .update_state_success(
                                        request_id,
                                        added_workloads_names,
                                        deleted_workloads_names,
                                    )
                                    .await
                                    .unwrap_or_illegal_state();
                            }
                            Ok(None) => {
                                log::debug!(
                                "The current state and new state are identical -> nothing to do"
                            );
                                self.to_agents
                                    .update_state_success(request_id, vec![], vec![])
                                    .await
                                    .unwrap_or_illegal_state();
                            }
                            Err(error_msg) => {
                                // [impl->swdd~server-continues-on-invalid-updated-state~1]
                                log::error!("Update rejected: '{error_msg}'",);
                                self.to_agents
                                    .error(request_id, format!("Update rejected: '{error_msg}'"))
                                    .await
                                    .unwrap_or_illegal_state();
                            }
                        }
                    }
                    // [impl->swdd~server-handles-logs-request-message~1]
                    common::commands::RequestContent::LogsRequest(mut logs_request) => {
                        log::debug!(
                            "Got log request. Id: '{}', Workload Instance Names: '{:?}'",
                            request_id,
                            logs_request.workload_names
                        );

                        // keep only workload instance names that are currently in the desired state
                        logs_request.workload_names.retain(|name| {
                            self.server_state.desired_state_contains_instance_name(name)
                        });
                        if !logs_request.workload_names.is_empty() {
                            log::debug!(
                                "Requesting logs from agents for the instance names: {:?}",
                                logs_request.workload_names
                            );
                            self.to_agents
                                .logs_request(request_id.clone(), logs_request.clone().into())
                                .await
                                .unwrap_or_illegal_state();

                            self.log_campaign_store
                                .insert_log_campaign(&request_id, &logs_request.workload_names);
                        }

                        self.to_agents
                            .logs_request_accepted(request_id.clone(), logs_request.into())
                            .await
                            .unwrap_or_illegal_state();
                    }
                    // [impl->swdd~server-handles-logs-cancel-request-message~1]
                    common::commands::RequestContent::LogsCancelRequest => {
                        log::debug!("Got log cancel request with ID: {}", request_id);

                        self.log_campaign_store.remove_logs_request_id(&request_id);

                        self.to_agents
                            .logs_cancel_request(request_id.clone())
                            .await
                            .unwrap_or_illegal_state();

                        self.to_agents
                            .logs_cancel_request_accepted(request_id)
                            .await
                            .unwrap_or_illegal_state();
                    }
                },
                ToServer::UpdateWorkloadState(method_obj) => {
                    log::debug!(
                        "Received UpdateWorkloadState: '{:?}'",
                        method_obj.workload_states
                    );

                    // [impl->swdd~server-stores-workload-state~1]
                    self.workload_states_map
                        .process_new_states(method_obj.workload_states.clone());

                    // [impl->swdd~server-cleans-up-state~1]
                    self.server_state.cleanup_state(&method_obj.workload_states);

                    // [impl->swdd~server-forwards-workload-state~1]
                    self.to_agents
                        .update_workload_state(method_obj.workload_states)
                        .await
                        .unwrap_or_illegal_state();
                }
                // [impl->swdd~server-forwards-logs-entries-response-messages~1]
                ToServer::LogEntriesResponse(request_id, logs_response) => {
                    self.to_agents
                        .log_entries_response(request_id, logs_response)
                        .await
                        .unwrap_or_illegal_state();
                }
                // [impl->swdd~server-forwards-logs-stop-response-messages~1]
                ToServer::LogsStopResponse(request_id, logs_stop_response) => {
                    log::debug!("Received LogsStopResponse with ID: {}", request_id);
                    self.to_agents
                        .logs_stop_response(request_id, logs_stop_response)
                        .await
                        .unwrap_or_illegal_state();
                }
                ToServer::Goodbye(goodbye) => {
                    log::debug!("Received 'Goodbye' from '{}'", goodbye.connection_name);

                    // [impl->swdd~server-cancels-log-campaign-for-disconnected-cli~1]
                    let removed_cli_log_requests = self
                        .log_campaign_store
                        .remove_cli_log_campaign_entry(&goodbye.connection_name);

                    self.cancel_log_requests_of_disconnected_collector(
                        &goodbye.connection_name,
                        removed_cli_log_requests,
                    )
                    .await;
                }
                ToServer::Stop(_method_obj) => {
                    log::debug!("Received Stop from communications server");
                    // TODO: handle the call
                    break;
                }
            }
        }
    }

    // [impl->swdd~server-handles-not-started-deleted-workloads~1]
    async fn handle_not_started_deleted_workloads(
        &mut self,
        mut deleted_workloads: Vec<DeletedWorkload>,
    ) -> Vec<DeletedWorkload> {
        let mut deleted_states = vec![];
        deleted_workloads.retain(|deleted_wl| {
            if deleted_wl.instance_name.agent_name().is_empty()
                || self.deleted_workload_never_started_on_agent(deleted_wl)
            {
                self.workload_states_map.remove(&deleted_wl.instance_name);
                deleted_states.push(WorkloadState {
                    instance_name: deleted_wl.instance_name.clone(),
                    execution_state: ExecutionState::removed(),
                });

                return false;
            }
            true
        });
        if !deleted_states.is_empty() {
            log::debug!(
                "Send UpdateWorkloadState for not started deleted workloads: '{:?}'",
                deleted_states
            );
            self.to_agents
                .update_workload_state(deleted_states)
                .await
                .unwrap_or_illegal_state();
        }

        deleted_workloads
    }

    fn deleted_workload_never_started_on_agent(&self, deleted_workload: &DeletedWorkload) -> bool {
        !self
            .server_state
            .contains_connected_agent(deleted_workload.instance_name.agent_name())
            && self
                .workload_states_map
                .get_workload_state_for_workload(&deleted_workload.instance_name)
                .is_some_and(|current_execution_state| current_execution_state.is_pending_initial())
    }

    // [impl->swdd~server-handles-log-campaign-for-disconnected-agent~1]
    // [impl->swdd~server-cancels-log-campaign-for-disconnected-cli~1]
    async fn cancel_log_requests_of_disconnected_collector(
        &mut self,
        connection_name: &str,
        request_ids_to_cancel: HashSet<LogCollectorRequestId>,
    ) {
        for request_id in request_ids_to_cancel {
            log::debug!(
                "Sending logs cancel request for disconnected connection '{}' with request id: {}",
                connection_name,
                request_id
            );
            self.to_agents
                .logs_cancel_request(request_id)
                .await
                .unwrap_or_illegal_state();
        }
    }

    // [impl->swdd~server-cancels-log-campaign-for-deleted-workloads~1]
    async fn cancel_log_requests_of_deleted_workloads(
        &mut self,
        deleted_workloads: &Vec<DeletedWorkload>,
    ) {
        for deleted_workload in deleted_workloads {
            let request_ids = self.log_campaign_store.remove_collector_campaign_entry(
                &deleted_workload.instance_name.workload_name().to_owned(),
            );
            for request_id in request_ids {
                log::debug!(
                    "Sending logs cancel request for deleted workload '{}' with request id: {}",
                    deleted_workload.instance_name.workload_name(),
                    request_id
                );
                self.to_agents
                    .logs_cancel_request(request_id)
                    .await
                    .unwrap_or_illegal_state();
            }
        }
    }

    // [impl->swdd~server-sends-logs-stop-response-for-disconnected-agents~1]
    async fn send_log_stop_response_for_disconnected_agent(
        &mut self,
        stopped_log_gatherings: Vec<(String, Vec<WorkloadInstanceName>)>,
    ) {
        for (request_id, stopped_log_providers) in stopped_log_gatherings {
            for workload_instance_name in stopped_log_providers {
                self.to_agents
                    .logs_stop_response(
                        request_id.to_owned(),
                        ank_base::LogsStopResponse {
                            workload_name: Some(workload_instance_name.into()),
                        },
                    )
                    .await
                    .unwrap_or_illegal_state();
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
    use std::collections::{HashMap, HashSet};

    use super::AnkaiosServer;
    use crate::ankaios_server::log_campaign_store::RemovedLogRequests;
    use crate::ankaios_server::server_state::{MockServerState, UpdateStateError};
    use crate::ankaios_server::{create_from_server_channel, create_to_server_channel};

    use super::ank_base;
    use api::ank_base::{LogsStopResponse, WorkloadMap};
    use common::commands::{
        AgentLoadStatus, CompleteStateRequest, LogsRequest, ServerHello, UpdateWorkload,
        UpdateWorkloadState,
    };
    use common::from_server_interface::FromServer;
    use common::objects::{
        generate_test_stored_workload_spec, generate_test_workload_spec_with_param,
        generate_test_workload_states_map_with_data, CompleteState, CpuUsage, DeletedWorkload,
        ExecutionState, ExecutionStateEnum, FreeMemory, PendingSubstate, State,
        WorkloadInstanceName, WorkloadState,
    };
    use common::test_utils::generate_test_proto_workload_with_param;
    use common::to_server_interface::ToServerInterface;
    use mockall::predicate;

    const AGENT_A: &str = "agent_A";
    const AGENT_B: &str = "agent_B";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_INSTANCE_NAME_1: &str = "workload_1.instanceId1.agent_A";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_INSTANCE_NAME_2: &str = "workload_2.instanceId2.agent_A";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const RUNTIME_NAME: &str = "runtime";
    const REQUEST_ID: &str = "request_1";
    const REQUEST_ID_A: &str = "agent_A@workload_1@request_1";
    const REQUEST_ID_A2: &str = "agent_A@workload_2@request_2";
    const INSTANCE_ID: &str = "instance_id";
    const MESSAGE: &str = "message";

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-fails-on-invalid-startup-state~1]
    #[tokio::test]
    async fn utest_server_start_fail_on_invalid_startup_manifest() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (_to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        // contains a self cycle to workload_A
        let workload = generate_test_stored_workload_spec(AGENT_A, RUNTIME_NAME);

        let startup_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([("workload_A".to_string(), workload)]),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(startup_state.clone()),
                mockall::predicate::eq(vec![]),
            )
            .once()
            .return_const(Err(UpdateStateError::CycleInDependencies(
                "workload_A part of cycle.".to_string(),
            )));
        server.server_state = mock_server_state;

        let result = server.start(Some(startup_state)).await;
        assert_eq!(
            result,
            Err("workload dependency 'workload_A part of cycle.' is part of a cycle.".into())
        );

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-fails-on-invalid-startup-state~1]
    #[tokio::test]
    async fn utest_server_start_fail_on_startup_manifest_with_invalid_version() {
        let (_to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let startup_state = CompleteState {
            desired_state: State {
                api_version: "invalidVersion".into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let result = server.start(Some(startup_state)).await;
        assert_eq!(
            result,
            Err("Unsupported API version. Received 'invalidVersion', expected 'v0.1'".into())
        );
    }

    // [utest->swdd~server-continues-on-invalid-updated-state~1]
    #[tokio::test]
    async fn utest_server_update_state_continues_on_invalid_new_state() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        /* new workload invalidates the state because
        it contains a self cycle in the inter workload dependencies config */
        let mut updated_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            "workload_A".to_string(),
            RUNTIME_NAME.to_string(),
        );

        let new_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(
                    updated_workload.instance_name.workload_name().to_owned(),
                    updated_workload.clone().into(),
                )]),
                ..Default::default()
            },
            ..Default::default()
        };

        // fix new state by deleting the dependencies
        let mut fixed_state = new_state.clone();
        updated_workload.dependencies.clear();
        fixed_state.desired_state.workloads = HashMap::from([(
            updated_workload.instance_name.workload_name().to_owned(),
            updated_workload.clone().into(),
        )]);

        let update_mask = vec!["desiredState.workloads".to_string()];

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        let mut seq = mockall::Sequence::new();
        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(new_state.clone()),
                mockall::predicate::eq(update_mask.clone()),
            )
            .once()
            .in_sequence(&mut seq)
            .return_const(Err(UpdateStateError::CycleInDependencies(
                "workload_A".to_string(),
            )));

        let added_workloads = vec![updated_workload.clone()];
        let deleted_workloads = vec![];

        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(fixed_state.clone()),
                mockall::predicate::eq(update_mask.clone()),
            )
            .once()
            .in_sequence(&mut seq)
            .return_const(Ok(Some((
                added_workloads.clone(),
                deleted_workloads.clone(),
            ))));

        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send the new invalid state update
        assert!(to_server
            .update_state(
                REQUEST_ID_A.to_string(),
                new_state.clone(),
                update_mask.clone()
            )
            .await
            .is_ok());

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::Error(_))
            }) if request_id == REQUEST_ID_A
        ));

        // send the update with the new clean state again
        assert!(to_server
            .update_state(REQUEST_ID_A.to_string(), fixed_state.clone(), update_mask)
            .await
            .is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        let expected_from_server_command = FromServer::UpdateWorkload(UpdateWorkload {
            added_workloads,
            deleted_workloads,
        });
        assert_eq!(from_server_command, expected_from_server_command);

        assert_eq!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID_A.into(),
                response_content: Some(ank_base::response::ResponseContent::UpdateStateSuccess(
                    ank_base::UpdateStateSuccess {
                        added_workloads: vec![updated_workload.instance_name.to_string()],
                        deleted_workloads: Vec::new(),
                    }
                )),
            })
        );

        // make sure all messages are consumed
        assert!(comm_middle_ware_receiver.try_recv().is_err());

        server_task.abort();
    }

    // [utest->swdd~server-sets-state-of-new-workloads-to-pending~1]
    // [utest->swdd~server-uses-async-channels~1]
    #[tokio::test]
    async fn utest_server_start_with_valid_startup_manifest() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let startup_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(
                    workload.instance_name.workload_name().to_owned(),
                    workload.clone().into(),
                )]),
                ..Default::default()
            },
            ..Default::default()
        };

        let added_workloads = vec![workload.clone()];
        let deleted_workloads = vec![];

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(startup_state.clone()),
                mockall::predicate::eq(vec![]),
            )
            .once()
            .return_const(Ok(Some((
                added_workloads.clone(),
                deleted_workloads.clone(),
            ))));

        server.server_state = mock_server_state;

        let server_handle = server.start(Some(startup_state));

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        tokio::join!(server_handle).0.unwrap();

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        let expected_from_server_command = FromServer::UpdateWorkload(UpdateWorkload {
            added_workloads,
            deleted_workloads,
        });
        assert_eq!(from_server_command, expected_from_server_command);

        assert_eq!(
            server
                .workload_states_map
                .get_workload_state_for_agent(AGENT_A),
            vec![WorkloadState {
                instance_name: workload.instance_name,
                execution_state: ExecutionState {
                    state: ExecutionStateEnum::Pending(PendingSubstate::Initial),
                    additional_info: Default::default()
                }
            }]
        );

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-sends-all-workloads-on-start~2]
    // [utest->swdd~agent-from-agent-field~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    // [utest->swdd~server-stores-newly-connected-agent~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let w2 = generate_test_workload_spec_with_param(
            AGENT_B.to_owned(),
            WORKLOAD_NAME_2.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let mut mock_server_state = MockServerState::new();

        mock_server_state.expect_cleanup_state().return_const(());

        let mut seq = mockall::Sequence::new();
        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(AGENT_A.to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w1.clone()]);

        mock_server_state
            .expect_add_agent()
            .with(predicate::eq(AGENT_A.to_owned()))
            .once()
            .in_sequence(&mut seq)
            .return_const(());

        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(AGENT_B.to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w2.clone()]);

        mock_server_state
            .expect_add_agent()
            .with(predicate::eq(AGENT_B.to_owned()))
            .once()
            .in_sequence(&mut seq)
            .return_const(());

        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        // first agent connects to the server
        let agent_hello_result = to_server.agent_hello(AGENT_A.to_string()).await;
        assert!(agent_hello_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::ServerHello(ServerHello {
                agent_name: Some(AGENT_A.to_string()),
                added_workloads: vec![w1],
            }),
            from_server_command
        );

        // [utest->swdd~server-informs-a-newly-connected-agent-workload-states~1]
        // [utest->swdd~server-starts-without-startup-config~1]
        // send update_workload_state for first agent which is then stored in the workload_state_db in ankaios server
        let test_wl_1_state_running = common::objects::generate_test_workload_state(
            WORKLOAD_NAME_1,
            ExecutionState::running(),
        );
        let update_workload_state_result = to_server
            .update_workload_state(vec![test_wl_1_state_running.clone()])
            .await;
        assert!(update_workload_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![test_wl_1_state_running.clone()]
            }),
            from_server_command
        );

        let agent_hello_result = to_server.agent_hello(AGENT_B.to_owned()).await;
        assert!(agent_hello_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![test_wl_1_state_running]
            }),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::ServerHello(ServerHello {
                agent_name: Some(AGENT_B.to_string()),
                added_workloads: vec![w2],
            }),
            from_server_command
        );

        // [utest->swdd~server-forwards-workload-state~1]
        // send update_workload_state for second agent which is then stored in the workload_state_db in ankaios server
        let test_wl_2_state_succeeded = common::objects::generate_test_workload_state(
            WORKLOAD_NAME_2,
            ExecutionState::succeeded(),
        );
        let update_workload_state_result = to_server
            .update_workload_state(vec![test_wl_2_state_succeeded.clone()])
            .await;
        assert!(update_workload_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![test_wl_2_state_succeeded.clone()]
            }),
            from_server_command
        );

        // send update_workload_state for first agent again which is then updated in the workload_state_db in ankaios server
        let test_wl_1_state_succeeded = common::objects::generate_test_workload_state(
            WORKLOAD_NAME_2,
            ExecutionState::succeeded(),
        );
        let update_workload_state_result = to_server
            .update_workload_state(vec![test_wl_1_state_succeeded.clone()])
            .await;
        assert!(update_workload_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![test_wl_1_state_succeeded.clone()]
            }),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-update-desired-state-interface~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    // [utest->swdd~server-update-state-success-response~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states_when_requested_update_state_success()
    {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME_NAME.to_string(),
        );
        w1.runtime_config = "changed".to_string();

        let update_state = CompleteState {
            desired_state: State {
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), w1.clone().into())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };

        let added_workloads = vec![w1.clone()];
        let deleted_workloads = vec![];

        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_1)];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(update_state.clone()),
                mockall::predicate::eq(update_mask.clone()),
            )
            .once()
            .return_const(Ok(Some((
                added_workloads.clone(),
                deleted_workloads.clone(),
            ))));
        server.server_state = mock_server_state;
        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(REQUEST_ID_A.to_string(), update_state, update_mask)
            .await;
        assert!(update_state_result.is_ok());

        let update_workload_message = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: added_workloads.clone(),
                deleted_workloads: deleted_workloads.clone(),
            }),
            update_workload_message
        );

        let update_state_success_message = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID_A.to_string(),
                response_content: Some(ank_base::response::ResponseContent::UpdateStateSuccess(
                    ank_base::UpdateStateSuccess {
                        added_workloads: added_workloads
                            .into_iter()
                            .map(|x| x.instance_name.to_string())
                            .collect(),
                        deleted_workloads: deleted_workloads
                            .into_iter()
                            .map(|x| x.instance_name.to_string())
                            .collect()
                    }
                ))
            }),
            update_state_success_message
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-update-desired-state-interface~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states_when_requested_update_state_nothing_to_do(
    ) {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut w1 =
            generate_test_stored_workload_spec(AGENT_A.to_owned(), RUNTIME_NAME.to_string());
        w1.runtime_config = "changed".to_string();

        let update_state = CompleteState {
            desired_state: State {
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), w1.clone())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_1)];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(update_state.clone()),
                mockall::predicate::eq(update_mask.clone()),
            )
            .once()
            .return_const(Ok(None));
        server.server_state = mock_server_state;
        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(REQUEST_ID_A.to_string(), update_state, update_mask)
            .await;
        assert!(update_state_result.is_ok());

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::UpdateStateSuccess(ank_base::UpdateStateSuccess {
                    added_workloads,
                    deleted_workloads
                }))
            }) if request_id == REQUEST_ID_A && added_workloads.is_empty() && deleted_workloads.is_empty()
        ));

        assert!(tokio::time::timeout(
            tokio::time::Duration::from_millis(200),
            comm_middle_ware_receiver.recv()
        )
        .await
        .is_err());

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-update-desired-state-interface~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states_when_requested_update_state_error() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let w1 = generate_test_stored_workload_spec(AGENT_A.to_owned(), RUNTIME_NAME.to_string());

        let update_state = CompleteState {
            desired_state: State {
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), w1.clone())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_1)];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(update_state.clone()),
                mockall::predicate::eq(update_mask.clone()),
            )
            .once()
            .return_const(Err(UpdateStateError::ResultInvalid(
                "some update error.".to_string(),
            )));
        server.server_state = mock_server_state;
        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(REQUEST_ID_A.to_string(), update_state, update_mask)
            .await;
        assert!(update_state_result.is_ok());

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::Error(_))
            }) if request_id == REQUEST_ID_A
        ));

        assert!(tokio::time::timeout(
            tokio::time::Duration::from_millis(200),
            comm_middle_ware_receiver.recv()
        )
        .await
        .is_err());

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-handles-logs-request-message~1]
    #[tokio::test]
    async fn utest_server_forward_logs_request_to_agents() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();

        mock_server_state
            .expect_desired_state_contains_instance_name()
            .with(mockall::predicate::function(
                |instance_name: &WorkloadInstanceName| {
                    instance_name
                        == &WorkloadInstanceName::new(AGENT_A, WORKLOAD_NAME_1, INSTANCE_ID)
                },
            ))
            .once()
            .return_const(true);

        server.server_state = mock_server_state;

        let log_providing_workloads = vec![WorkloadInstanceName::new(
            AGENT_A,
            WORKLOAD_NAME_1,
            INSTANCE_ID,
        )];

        server
            .log_campaign_store
            .expect_insert_log_campaign()
            .with(
                predicate::eq(REQUEST_ID_A.to_owned()),
                predicate::eq(log_providing_workloads.clone()),
            )
            .once()
            .return_const(());

        let server_task = tokio::spawn(async move { server.start(None).await });

        let logs_request = LogsRequest {
            workload_names: log_providing_workloads,
            follow: true,
            tail: 10,
            since: None,
            until: None,
        };

        // send logs request to server
        let logs_request_result = to_server
            .logs_request(REQUEST_ID_A.to_string(), logs_request)
            .await;
        assert!(logs_request_result.is_ok());
        drop(to_server);

        let logs_request_message = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::LogsRequest(
                REQUEST_ID_A.into(),
                LogsRequest {
                    workload_names: vec![WorkloadInstanceName::new(
                        AGENT_A,
                        WORKLOAD_NAME_1,
                        INSTANCE_ID,
                    )],
                    follow: true,
                    tail: 10,
                    since: None,
                    until: None
                }
            ),
            logs_request_message
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            from_server_command,
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID_A.to_string(),
                response_content: Some(ank_base::response::ResponseContent::LogsRequestAccepted(
                    ank_base::LogsRequestAccepted {
                        workload_names: vec![ank_base::WorkloadInstanceName {
                            workload_name: WORKLOAD_NAME_1.to_string(),
                            agent_name: AGENT_A.to_string(),
                            id: INSTANCE_ID.to_string()
                        }],
                    }
                )),
            })
        );

        assert!(comm_middle_ware_receiver.recv().await.is_none());

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-handles-logs-request-message~1]
    #[tokio::test]
    async fn utest_server_forward_logs_request_invalid_workload_names() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();

        mock_server_state
            .expect_desired_state_contains_instance_name()
            .with(mockall::predicate::eq(WorkloadInstanceName::new(
                AGENT_A,
                WORKLOAD_NAME_1,
                INSTANCE_ID,
            )))
            .once()
            .return_const(false);

        server.server_state = mock_server_state;

        server
            .log_campaign_store
            .expect_insert_log_campaign()
            .never();

        let logs_request = LogsRequest {
            workload_names: vec![WorkloadInstanceName::new(
                AGENT_A,
                WORKLOAD_NAME_1,
                INSTANCE_ID,
            )],
            follow: true,
            tail: 10,
            since: None,
            until: None,
        };

        // send logs request to server
        let logs_request_result = to_server
            .logs_request(REQUEST_ID.to_string(), logs_request)
            .await;
        assert!(logs_request_result.is_ok());

        assert!(to_server.stop().await.is_ok());
        let server_result = server.start(None).await;

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            from_server_command,
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID.to_string(),
                response_content: Some(ank_base::response::ResponseContent::LogsRequestAccepted(
                    ank_base::LogsRequestAccepted {
                        workload_names: vec![],
                    }
                )),
            })
        );

        assert!(comm_middle_ware_receiver.try_recv().is_err());
        assert!(server_result.is_ok());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-interface-get-complete-state~2]
    // [utest->swdd~server-includes-id-in-control-interface-response~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_returns_complete_state_when_received_request_complete_state() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let w1 = generate_test_proto_workload_with_param(AGENT_A, RUNTIME_NAME);

        let w2 = generate_test_proto_workload_with_param(AGENT_A, RUNTIME_NAME);

        let w3 = generate_test_proto_workload_with_param(AGENT_B, RUNTIME_NAME);

        let workloads = HashMap::from([
            (WORKLOAD_NAME_1.to_owned(), w1),
            (WORKLOAD_NAME_2.to_owned(), w2),
            (WORKLOAD_NAME_3.to_owned(), w3),
        ]);

        let workload_map = WorkloadMap { workloads };

        let current_complete_state = ank_base::CompleteState {
            desired_state: Some(ank_base::State {
                workloads: Some(workload_map),
                ..Default::default()
            }),
            ..Default::default()
        };
        let request_id = format!("{AGENT_A}@my_request_id");
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_get_complete_state_by_field_mask()
            .with(
                mockall::predicate::function(|request_complete_state| {
                    request_complete_state == &CompleteStateRequest { field_mask: vec![] }
                }),
                mockall::predicate::always(),
            )
            .once()
            .return_const(Ok(current_complete_state.clone()));
        server.server_state = mock_server_state;
        let server_task = tokio::spawn(async move { server.start(None).await });

        // send command 'CompleteStateRequest'
        // CompleteState shall contain the complete state
        let request_complete_state_result = to_server
            .request_complete_state(
                request_id.clone(),
                CompleteStateRequest { field_mask: vec![] },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            from_server_command,
            common::from_server_interface::FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::CompleteState(
                    current_complete_state
                ))
            })
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-interface-get-complete-state~2]
    // [utest->swdd~server-includes-id-in-control-interface-response~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_returns_complete_state_when_received_request_complete_state_error() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_get_complete_state_by_field_mask()
            .with(
                mockall::predicate::function(|request_complete_state| {
                    request_complete_state == &CompleteStateRequest { field_mask: vec![] }
                }),
                mockall::predicate::always(),
            )
            .once()
            .return_const(Err("complete state error.".to_string()));
        server.server_state = mock_server_state;
        let server_task = tokio::spawn(async move { server.start(None).await });

        let request_id = format!("{AGENT_A}@my_request_id");
        // send command 'CompleteStateRequest'
        // CompleteState shall contain the complete state
        let request_complete_state_result = to_server
            .request_complete_state(
                request_id.clone(),
                CompleteStateRequest { field_mask: vec![] },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        let expected_complete_state = ank_base::CompleteState {
            ..Default::default()
        };

        assert_eq!(
            from_server_command,
            common::from_server_interface::FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::CompleteState(
                    expected_complete_state
                ))
            })
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-stores-workload-state~1]
    // [utest->swdd~server-set-workload-state-on-disconnect~1]
    // [utest->swdd~server-distribute-workload-state-on-disconnect~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    // [utest->swdd~server-removes-disconnected-agents-from-state~1]
    #[tokio::test]
    async fn utest_server_start_distributes_workload_states_after_agent_disconnect() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_agent_log_campaign_entry()
            .once()
            .return_const(RemovedLogRequests::default());

        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_cleanup_state()
            .once()
            .return_const(());

        mock_server_state
            .expect_remove_agent()
            .once()
            .with(predicate::eq(AGENT_A))
            .return_const(());

        server.server_state = mock_server_state;

        // send update_workload_state for first agent which is then stored in the workload_state_db in ankaios server
        let test_wl_1_state_running = common::objects::generate_test_workload_state_with_agent(
            WORKLOAD_NAME_1,
            AGENT_A,
            ExecutionState::running(),
        );
        let update_workload_state_result = to_server
            .update_workload_state(vec![test_wl_1_state_running.clone()])
            .await;
        assert!(update_workload_state_result.is_ok());

        // first agent disconnects from the ankaios server
        let agent_gone_result = to_server.agent_gone(AGENT_A.to_owned()).await;
        assert!(agent_gone_result.is_ok());

        let server_handle = server.start(None);

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        tokio::join!(server_handle).0.unwrap();

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![test_wl_1_state_running.clone()]
            }),
            from_server_command
        );

        let workload_states = server
            .workload_states_map
            .get_workload_state_for_agent(AGENT_A);

        let expected_workload_state = common::objects::generate_test_workload_state_with_agent(
            WORKLOAD_NAME_1,
            AGENT_A,
            ExecutionState::agent_disconnected(),
        );
        assert_eq!(vec![expected_workload_state.clone()], workload_states);

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![expected_workload_state]
            }),
            from_server_command
        );
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-handles-log-campaign-for-disconnected-agent~1]
    #[tokio::test]
    async fn utest_server_sends_logs_cancel_requests_on_disconnected_agent() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_agent_log_campaign_entry()
            .with(predicate::eq(AGENT_A.to_owned()))
            .once()
            .return_const(RemovedLogRequests {
                collector_requests: HashSet::from([
                    REQUEST_ID_A.to_owned(),
                    REQUEST_ID_A2.to_owned(),
                ]),
                ..Default::default()
            });

        let mut mock_server_state = MockServerState::new();
        mock_server_state.expect_cleanup_state().never();

        mock_server_state
            .expect_remove_agent()
            .once()
            .with(predicate::eq(AGENT_A))
            .return_const(());

        server.server_state = mock_server_state;

        let agent_gone_result = to_server.agent_gone(AGENT_A.to_owned()).await;
        assert!(agent_gone_result.is_ok());

        let server_task = tokio::spawn(async move { server.start(None).await });

        let expected_logs_cancel_requests = vec![
            FromServer::LogsCancelRequest(REQUEST_ID_A.to_string()),
            FromServer::LogsCancelRequest(REQUEST_ID_A2.to_string()),
        ];
        let mut actual_logs_cancel_requests = Vec::new();
        let _update_workload_state = comm_middle_ware_receiver.recv().await.unwrap();
        let logs_cancel_request_a = comm_middle_ware_receiver.recv().await.unwrap();
        actual_logs_cancel_requests.push(logs_cancel_request_a);
        let logs_cancel_request_a2 = comm_middle_ware_receiver.recv().await.unwrap();
        actual_logs_cancel_requests.push(logs_cancel_request_a2);

        for request in actual_logs_cancel_requests {
            assert!(
                expected_logs_cancel_requests.contains(&request),
                "Actual request: '{:?}' not found in expected requests.",
                request
            );
        }

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-handles-log-campaign-for-disconnected-agent~1]
    #[tokio::test]
    async fn utest_server_sends_logs_stop_responses_on_disconnected_agent() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let instance_name_1: WorkloadInstanceName = WORKLOAD_INSTANCE_NAME_1.try_into().unwrap();
        let instance_name_2: WorkloadInstanceName = WORKLOAD_INSTANCE_NAME_2.try_into().unwrap();

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_agent_log_campaign_entry()
            .with(predicate::eq(AGENT_A.to_owned()))
            .once()
            .return_const(RemovedLogRequests {
                collector_requests: HashSet::new(),
                disconnected_log_providers: vec![(
                    REQUEST_ID_A.to_owned(),
                    vec![instance_name_1.clone(), instance_name_2.clone()],
                )],
            });

        let mut mock_server_state = MockServerState::new();
        mock_server_state.expect_cleanup_state().never();

        mock_server_state
            .expect_remove_agent()
            .once()
            .with(predicate::eq(AGENT_A))
            .return_const(());

        server.server_state = mock_server_state;

        let agent_gone_result = to_server.agent_gone(AGENT_A.to_owned()).await;
        assert!(agent_gone_result.is_ok());

        let server_task = tokio::spawn(async move { server.start(None).await });

        let expected_logs_stop_responses = vec![
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID_A.to_string(),
                response_content: Some(ank_base::response::ResponseContent::LogsStopResponse(
                    LogsStopResponse {
                        workload_name: Some(instance_name_1.into()),
                    },
                )),
            }),
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID_A.to_string(),
                response_content: Some(ank_base::response::ResponseContent::LogsStopResponse(
                    LogsStopResponse {
                        workload_name: Some(instance_name_2.into()),
                    },
                )),
            }),
        ];
        let mut actual_logs_stop_response = Vec::new();
        let _update_workload_state = comm_middle_ware_receiver.recv().await.unwrap();
        let logs_stop_response_wl1 = comm_middle_ware_receiver.recv().await.unwrap();
        actual_logs_stop_response.push(logs_stop_response_wl1);
        let logs_stop_response_wl2 = comm_middle_ware_receiver.recv().await.unwrap();
        actual_logs_stop_response.push(logs_stop_response_wl2);

        for request in actual_logs_stop_response {
            assert!(
                expected_logs_stop_responses.contains(&request),
                "Actual request: '{:?}' not found in expected requests.",
                request
            );
        }

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-sets-state-of-new-workloads-to-pending~1]
    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_start_calls_agents_in_update_state_command() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let w2 = generate_test_workload_spec_with_param(
            AGENT_B.to_owned(),
            WORKLOAD_NAME_2.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let mut updated_w1 = w1.clone();
        updated_w1.instance_name = WorkloadInstanceName::builder()
            .workload_name(w1.instance_name.workload_name())
            .agent_name(w1.instance_name.agent_name())
            .config(&String::from("changed"))
            .build();
        let update_state = CompleteState {
            desired_state: State {
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), updated_w1.clone().into())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        let update_mask = vec!["desiredState.workloads".to_string()];

        let added_workloads = vec![updated_w1.clone()];
        let deleted_workloads = vec![DeletedWorkload {
            instance_name: w1.instance_name.clone(),
            dependencies: HashMap::new(),
        }];

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        let mut seq = mockall::Sequence::new();
        mock_server_state
            .expect_contains_connected_agent()
            .return_const(true);
        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(AGENT_A.to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w1.clone()]);

        mock_server_state
            .expect_add_agent()
            .times(2)
            .return_const(());

        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(AGENT_B.to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w2.clone()]);

        mock_server_state
            .expect_update()
            .with(
                mockall::predicate::eq(update_state.clone()),
                mockall::predicate::eq(update_mask.clone()),
            )
            .once()
            .in_sequence(&mut seq)
            .return_const(Ok(Some((added_workloads, deleted_workloads))));
        server.server_state = mock_server_state;

        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .return_const(HashSet::new());

        let agent_hello1_result = to_server.agent_hello(AGENT_A.to_owned()).await;
        assert!(agent_hello1_result.is_ok());

        let agent_hello2_result = to_server.agent_hello(AGENT_B.to_owned()).await;
        assert!(agent_hello2_result.is_ok());

        let update_state_result = to_server
            .update_state(REQUEST_ID_A.to_string(), update_state, update_mask.clone())
            .await;
        assert!(update_state_result.is_ok());

        let server_handle = server.start(None);

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        tokio::join!(server_handle).0.unwrap();

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::ServerHello(ServerHello {
                agent_name: Some(AGENT_A.to_string()),
                added_workloads: vec![w1.clone()]
            }),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::ServerHello(ServerHello {
                agent_name: Some(AGENT_B.to_string()),
                added_workloads: vec![w2],
            }),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![updated_w1.clone()],
                deleted_workloads: vec![DeletedWorkload {
                    instance_name: w1.instance_name.clone(),
                    dependencies: HashMap::new(),
                }]
            }),
            from_server_command
        );

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::UpdateStateSuccess(ank_base::UpdateStateSuccess {
                    added_workloads,
                    deleted_workloads
                }))
            }) if request_id == REQUEST_ID_A && added_workloads == vec![updated_w1.instance_name.to_string()] && deleted_workloads == vec![w1.instance_name.to_string()]
        ));

        assert_eq!(
            server
                .workload_states_map
                .get_workload_state_for_agent(AGENT_A),
            vec![WorkloadState {
                instance_name: updated_w1.instance_name,
                execution_state: ExecutionState {
                    state: ExecutionStateEnum::Pending(PendingSubstate::Initial),
                    additional_info: Default::default()
                }
            }]
        );

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_stop() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mock_server_state = MockServerState::new();
        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        assert!(to_server.stop().await.is_ok());

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert!(server_task.is_finished());

        if !server_task.is_finished() {
            server_task.abort();
        }
    }

    // [utest->swdd~server-forwards-logs-entries-response-messages~1]
    #[tokio::test]
    async fn utest_logs_response() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mock_server_state = MockServerState::new();
        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        assert!(to_server
            .log_entries_response(
                REQUEST_ID.into(),
                ank_base::LogEntriesResponse {
                    log_entries: vec![ank_base::LogEntry {
                        workload_name: Some(ank_base::WorkloadInstanceName {
                            workload_name: WORKLOAD_NAME_1.into(),
                            agent_name: AGENT_A.into(),
                            id: INSTANCE_ID.into()
                        }),
                        message: MESSAGE.into()
                    }]
                }
            )
            .await
            .is_ok());

        assert_eq!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID.into(),
                response_content: Some(ank_base::response::ResponseContent::LogEntriesResponse(
                    ank_base::LogEntriesResponse {
                        log_entries: vec![ank_base::LogEntry {
                            workload_name: Some(ank_base::WorkloadInstanceName {
                                workload_name: WORKLOAD_NAME_1.into(),
                                agent_name: AGENT_A.into(),
                                id: INSTANCE_ID.into()
                            }),
                            message: MESSAGE.into()
                        },]
                    }
                ))
            })
        );
        server_task.abort();
    }

    // [utest->swdd~update-desired-state-with-invalid-version~1]
    #[tokio::test]
    async fn utest_server_rejects_update_state_with_incompatible_version() {
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let update_state = CompleteState {
            desired_state: State {
                api_version: "incompatible_version".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_1)];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(REQUEST_ID_A.to_string(), update_state.clone(), update_mask)
            .await;
        assert!(update_state_result.is_ok());

        let error_message = format!(
            "Unsupported API version. Received 'incompatible_version', expected '{}'",
            State::default().api_version
        );
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID_A.to_string(),
                response_content: Some(ank_base::response::ResponseContent::Error(
                    ank_base::Error {
                        message: error_message
                    }
                )),
            }),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~update-desired-state-with-missing-version~1]
    #[tokio::test]
    async fn utest_server_rejects_update_state_without_api_version() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut update_state_ankaios_no_version: CompleteState = CompleteState {
            ..Default::default()
        };
        update_state_ankaios_no_version.desired_state.api_version = "".to_string();

        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_1)];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(
                REQUEST_ID_A.to_string(),
                update_state_ankaios_no_version.clone(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        let error_message = format!(
            "Unsupported API version. Received '', expected '{}'",
            State::default().api_version
        );
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID_A.to_string(),
                response_content: Some(ank_base::response::ResponseContent::Error(
                    ank_base::Error {
                        message: error_message
                    }
                )),
            }),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-cleans-up-state~1]
    #[tokio::test]
    async fn utest_server_triggers_delete_of_actually_removed_workloads_from_delete_graph() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        let mut mock_server_state = MockServerState::new();

        let workload_states = vec![common::objects::generate_test_workload_state(
            WORKLOAD_NAME_1,
            ExecutionState::removed(),
        )];

        mock_server_state
            .expect_cleanup_state()
            .with(mockall::predicate::eq(workload_states.clone()))
            .return_const(());
        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        let update_workload_state_result = to_server.update_workload_state(workload_states).await;
        assert!(update_workload_state_result.is_ok());

        server_task.abort();
    }

    // [utest->swdd~server-handles-not-started-deleted-workloads~1]
    #[tokio::test]
    async fn utest_server_handles_deleted_workload_on_empty_agent() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let workload_without_agent = generate_test_workload_spec_with_param(
            "".to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let workload_with_agent = generate_test_workload_spec_with_param(
            AGENT_B.to_owned(),
            WORKLOAD_NAME_2.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let update_state = CompleteState::default();
        let update_mask = vec!["desiredState.workloads".to_string()];

        let deleted_workload_without_agent = DeletedWorkload {
            instance_name: workload_without_agent.instance_name.clone(),
            ..Default::default()
        };
        let deleted_workload_with_agent = DeletedWorkload {
            instance_name: workload_with_agent.instance_name.clone(),
            ..Default::default()
        };

        let deleted_workloads = vec![
            deleted_workload_without_agent.clone(),
            deleted_workload_with_agent.clone(),
        ];

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_contains_connected_agent()
            .once()
            .return_const(false);
        mock_server_state
            .expect_update()
            .once()
            .return_const(Ok(Some((vec![], deleted_workloads.clone()))));
        server.server_state = mock_server_state;

        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .return_const(HashSet::new());

        let update_state_result = to_server
            .update_state(REQUEST_ID_A.to_string(), update_state, update_mask.clone())
            .await;
        assert!(update_state_result.is_ok());

        let server_handle = server.start(None);

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        tokio::join!(server_handle).0.unwrap();

        // the server sends the ExecutionState removed for the workload with an empty agent name
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    instance_name: workload_without_agent.instance_name,
                    execution_state: ExecutionState::removed()
                }]
            }),
            from_server_command
        );

        // the server sends only a deleted workload for the workload with a non-empty agent name
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![],
                deleted_workloads: vec![deleted_workload_with_agent.clone()],
            }),
            from_server_command
        );

        // ignore UpdateStateSuccessful response
        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(_)
        ));

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-cancels-log-campaign-for-deleted-workloads~1]
    #[tokio::test]
    async fn utest_server_cancels_log_collection_of_deleted_workload() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let log_collecting_workload = generate_test_workload_spec_with_param(
            AGENT_B.to_owned(),
            WORKLOAD_NAME_2.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let update_state = CompleteState::default();
        let update_mask = vec!["desiredState.workloads".to_string()];

        let deleted_workload_with_agent = DeletedWorkload {
            instance_name: log_collecting_workload.instance_name.clone(),
            ..Default::default()
        };

        let deleted_workloads = vec![deleted_workload_with_agent.clone()];

        let mut server: AnkaiosServer = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_contains_connected_agent()
            .once()
            .return_const(false);
        mock_server_state
            .expect_update()
            .once()
            .return_const(Ok(Some((vec![], deleted_workloads.clone()))));
        server.server_state = mock_server_state;

        let logs_request_id = format!(
            "{}@{}@{}",
            log_collecting_workload.instance_name.agent_name(),
            log_collecting_workload.instance_name.workload_name(),
            "uuid2"
        );
        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .once()
            .with(predicate::eq(
                log_collecting_workload
                    .instance_name
                    .workload_name()
                    .to_owned(),
            ))
            .return_const(HashSet::from([logs_request_id.to_owned()]));

        let update_state_result = to_server
            .update_state(REQUEST_ID_A.to_string(), update_state, update_mask.clone())
            .await;
        assert!(update_state_result.is_ok());

        let server_handle = server.start(None);

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        tokio::join!(server_handle).0.unwrap();

        // the server sends the LogsCancelRequest for workload 2
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::LogsCancelRequest(logs_request_id),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![],
                deleted_workloads: vec![deleted_workload_with_agent.clone()],
            }),
            from_server_command
        );

        // ignore UpdateStateSuccessful response
        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(_)
        ));

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-receives-resource-availability~1]
    #[tokio::test]
    async fn utest_server_receives_agent_status_load() {
        let payload = AgentLoadStatus {
            agent_name: AGENT_A.to_string(),
            cpu_usage: CpuUsage { cpu_usage: 42 },
            free_memory: FreeMemory { free_memory: 42 },
        };

        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_update_agent_resource_availability()
            .with(mockall::predicate::eq(payload.clone()))
            .return_const(());
        server.server_state = mock_server_state;

        let agent_resource_result = to_server.agent_load_status(payload).await;
        assert!(agent_resource_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;

        assert!(result.is_ok());
    }

    // [utest->swdd~server-handles-not-started-deleted-workloads~1]
    #[tokio::test]
    async fn utest_server_handles_pending_initial_deleted_workload_on_not_connected_agent() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (_to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_contains_connected_agent()
            .once()
            .return_const(false);

        let workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        server.server_state = mock_server_state;
        server.workload_states_map = generate_test_workload_states_map_with_data(
            workload.instance_name.agent_name(),
            workload.instance_name.workload_name(),
            workload.instance_name.id(),
            ExecutionState::initial(),
        );

        let deleted_workload_with_not_connected_agent = DeletedWorkload {
            instance_name: workload.instance_name.clone(),
            ..Default::default()
        };

        let deleted_workloads = vec![deleted_workload_with_not_connected_agent.clone()];

        let retained_deleted_workloads = server
            .handle_not_started_deleted_workloads(deleted_workloads)
            .await;

        assert!(retained_deleted_workloads.is_empty());

        assert_eq!(
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                comm_middle_ware_receiver.recv(),
            )
            .await,
            Ok(Some(FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    instance_name: workload.instance_name,
                    execution_state: ExecutionState::removed()
                }]
            })))
        );
    }

    // [utest->swdd~server-handles-logs-cancel-request-message~1]
    #[tokio::test]
    async fn utest_server_log_cancel_request() {
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let request_id = REQUEST_ID.to_string();
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_logs_request_id()
            .once()
            .return_const(());

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let result = to_server.logs_cancel_request(request_id.clone()).await;
        assert!(result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::LogsCancelRequest(request_id.clone(),),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::LogsCancelAccepted(
                    ank_base::LogsCancelAccepted {}
                )),
            }),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-forwards-logs-stop-response-messages~1]
    #[tokio::test]
    async fn utest_server_logs_stop_response() {
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let request_id = REQUEST_ID.to_string();
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let server_task = tokio::spawn(async move { server.start(None).await });

        let workload_instance_name = ank_base::WorkloadInstanceName {
            workload_name: WORKLOAD_NAME_1.to_string(),
            agent_name: AGENT_A.to_string(),
            id: INSTANCE_ID.to_string(),
        };

        // send new state to server
        let result = to_server
            .logs_stop_response(
                request_id.clone(),
                ank_base::LogsStopResponse {
                    workload_name: Some(workload_instance_name.clone()),
                },
            )
            .await;
        assert!(result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(ank_base::Response {
                request_id: request_id.clone(),
                response_content: Some(ank_base::response::ResponseContent::LogsStopResponse(
                    ank_base::LogsStopResponse {
                        workload_name: Some(workload_instance_name),
                    }
                ))
            }),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-cancels-log-campaign-for-disconnected-cli~1]
    #[tokio::test]
    async fn utest_server_sends_logs_cancel_request_on_cli_disconnect() {
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let cli_connection_name = "cli-conn-1234".to_string();
        let cli_request_id = format!("{cli_connection_name}@cli-request-id-1");
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_cli_log_campaign_entry()
            .with(mockall::predicate::eq(cli_connection_name.clone()))
            .once()
            .return_const(HashSet::from([cli_request_id.clone()]));

        let server_task = tokio::spawn(async move { server.start(None).await });

        let result = to_server.goodbye(cli_connection_name).await;
        assert!(result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::LogsCancelRequest(cli_request_id),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }
}

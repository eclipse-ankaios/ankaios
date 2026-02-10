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
mod event_handler;
mod log_campaign_store;
mod rendered_workloads;
mod request_id;
mod server_state;
mod state_comparator;

use ankaios_api::ank_base::{
    AgentAttributesSpec, AgentMapSpec, AgentStatusSpec, CompleteState, CompleteStateSpec,
    DeletedWorkload, ExecutionStateSpec, LogsStopResponse, Request, RequestContent,
    WorkloadInstanceName, WorkloadInstanceNameSpec, WorkloadStateSpec, WorkloadStatesMapSpec,
};
use common::state_manipulation::Path;

use server_state::AddedDeletedWorkloads;

use common::std_extensions::{IllegalStateResult, UnreachableResult};

#[cfg_attr(test, mockall_double::double)]
use server_state::ServerState;

#[cfg_attr(test, mockall_double::double)]
use event_handler::EventHandler;

#[cfg_attr(test, mockall_double::double)]
use state_comparator::StateComparator;

use common::from_server_interface::{
    FromServer, FromServerInterface, FromServerReceiver, FromServerSender,
};
use common::to_server_interface::{ToServer, ToServerReceiver, ToServerSender};
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

use crate::ankaios_server::server_state::StateGenerationResult;

pub struct AnkaiosServer {
    // [impl->swdd~server-uses-async-channels~1]
    receiver: ToServerReceiver,
    // [impl->swdd~communication-to-from-server-middleware~1]
    to_agents: FromServerSender,
    server_state: ServerState,
    workload_states_map: WorkloadStatesMapSpec,
    agent_map: AgentMapSpec,
    log_campaign_store: LogCampaignStore,
    event_handler: EventHandler,
}

impl AnkaiosServer {
    pub fn new(receiver: ToServerReceiver, to_agents: FromServerSender) -> Self {
        AnkaiosServer {
            receiver,
            to_agents,
            server_state: ServerState::default(),
            workload_states_map: WorkloadStatesMapSpec::default(),
            agent_map: AgentMapSpec::default(),
            log_campaign_store: LogCampaignStore::default(),
            event_handler: EventHandler::default(),
        }
    }

    pub async fn start(&mut self, startup_state: Option<CompleteStateSpec>) -> Result<(), String> {
        if let Some(state) = startup_state {
            // [impl->swdd~server-validates-desired-state-api-version~1]
            state.desired_state.validate_pre_rendering()?;

            match self.server_state.update(state.desired_state) {
                Ok(Some(added_deleted_workloads)) => {
                    let added_workloads = added_deleted_workloads.added_workloads;
                    let deleted_workloads = added_deleted_workloads.deleted_workloads;

                    // [impl->swdd~server-sets-state-of-new-workloads-to-pending~1]
                    self.workload_states_map.initial_state(&added_workloads);

                    log::info!("Starting...");
                    self.to_agents
                        .update_workload(added_workloads, deleted_workloads)
                        .await
                        .unwrap_or_illegal_state();
                }
                Ok(None) => {
                    log::info!("No initial workloads to send to agents.");
                }
                Err(err) => {
                    // [impl->swdd~server-fails-on-invalid-startup-state~1]
                    return Err(err.to_string());
                }
            }

            // at this point, there are no event subscribers yet, so no need to send events
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
                    let tags = method_obj.tags.try_into().unwrap_or_unreachable();

                    // [impl->swdd~server-informs-a-newly-connected-agent-workload-states~1]
                    let workload_states = self
                        .workload_states_map
                        .get_workload_state_excluding_agent(&agent_name);

                    if !workload_states.is_empty() {
                        log::debug!(
                            "Sending initial UpdateWorkloadState to agent '{agent_name}' with workload states: '{workload_states:?}'",
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
                        "Sending initial ServerHello to agent '{agent_name}' with added workloads: '{added_workloads:?}'",
                    );

                    // [impl->swdd~server-sends-all-workloads-on-start~2]
                    self.to_agents
                        .server_hello(Some(agent_name.clone()), added_workloads)
                        .await
                        .unwrap_or_illegal_state();

                    // [impl->swdd~server-sends-state-differences-as-events~1]
                    let old_state = if self.event_handler.has_subscribers() {
                        CompleteStateSpec {
                            agents: self.agent_map.clone(),
                            ..Default::default()
                        }
                    } else {
                        CompleteStateSpec::default()
                    };

                    // [impl->swdd~server-stores-newly-connected-agent~2]
                    self.agent_map.agents.insert(
                        agent_name.clone(),
                        AgentAttributesSpec {
                            tags,
                            ..Default::default()
                        },
                    );

                    if self.event_handler.has_subscribers() {
                        // [impl->swdd~server-sends-state-differences-as-events~1]
                        let new_state = CompleteStateSpec {
                            agents: self.agent_map.clone(),
                            ..Default::default()
                        };

                        let state_comparator = StateComparator::new(old_state, new_state);
                        let state_difference_tree = state_comparator.state_differences();
                        self.event_handler
                            .send_events(
                                &self.server_state,
                                &self.workload_states_map,
                                &self.agent_map,
                                state_difference_tree,
                                &self.to_agents,
                            )
                            .await;
                    }
                }
                // [impl->swdd~server-receives-resource-availability~2]
                ToServer::AgentLoadStatus(method_obj) => {
                    log::trace!(
                        "Received load status from agent '{}': CPU usage: {}%, Free Memory: {}B",
                        method_obj.agent_name,
                        method_obj.cpu_usage.cpu_usage,
                        method_obj.free_memory.free_memory,
                    );

                    // [impl->swdd~server-sends-state-differences-as-events~1]
                    let old_state = if self.event_handler.has_subscribers() {
                        CompleteStateSpec {
                            agents: self.agent_map.clone(),
                            ..Default::default()
                        }
                    } else {
                        CompleteStateSpec::default()
                    };

                    self.agent_map
                        .agents
                        .entry(method_obj.agent_name)
                        .and_modify(|e| {
                            e.status = Some(AgentStatusSpec {
                                cpu_usage: Some(method_obj.cpu_usage),
                                free_memory: Some(method_obj.free_memory),
                            })
                        });

                    if self.event_handler.has_subscribers() {
                        // [impl->swdd~server-sends-state-differences-as-events~1]
                        let new_state = CompleteStateSpec {
                            agents: self.agent_map.clone(),
                            ..Default::default()
                        };

                        let state_comparator = StateComparator::new(old_state, new_state);

                        let state_difference_tree = state_comparator.state_differences();

                        self.event_handler
                            .send_events(
                                &self.server_state,
                                &self.workload_states_map,
                                &self.agent_map,
                                state_difference_tree,
                                &self.to_agents,
                            )
                            .await;
                    }
                }
                ToServer::AgentGone(method_obj) => {
                    log::debug!("Received AgentGone from '{}'", method_obj.agent_name);
                    // [impl->swdd~server-sends-state-differences-as-events~1]
                    let old_state = if self.event_handler.has_subscribers() {
                        CompleteStateSpec {
                            agents: self.agent_map.clone(),
                            workload_states: self.workload_states_map.clone(),
                            ..Default::default()
                        }
                    } else {
                        CompleteStateSpec::default()
                    };

                    let agent_name = method_obj.agent_name;

                    // [impl->swdd~server-removes-disconnected-agents-from-state~2]
                    self.agent_map.agents.remove(&agent_name);

                    // [impl->swdd~server-set-workload-state-on-disconnect~1]
                    self.workload_states_map.agent_disconnected(&agent_name);

                    // communicate the workload execution states to other agents
                    // [impl->swdd~server-distribute-workload-state-on-disconnect~1]
                    let agent_workload_states = self
                        .workload_states_map
                        .get_workload_state_for_agent(&agent_name);

                    if self.event_handler.has_subscribers() {
                        // [impl->swdd~server-sends-state-differences-as-events~1]
                        let new_state = CompleteStateSpec {
                            agents: self.agent_map.clone(),
                            workload_states: self.workload_states_map.clone(),
                            ..Default::default()
                        };

                        let state_comparator = StateComparator::new(old_state, new_state);

                        let state_difference_tree = state_comparator.state_differences();

                        self.event_handler
                            .send_events(
                                &self.server_state,
                                &self.workload_states_map,
                                &self.agent_map,
                                state_difference_tree,
                                &self.to_agents,
                            )
                            .await;

                        self.event_handler.remove_subscribers_of_agent(&agent_name);
                    }

                    // [impl->swdd~server-distribute-workload-state-on-disconnect~1]
                    self.to_agents
                        .update_workload_state(agent_workload_states)
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
                }) => {
                    let Some(request_content) = request_content else {
                        log::warn!(
                            "The Request with id '{request_id}' does not contain any content -> ignoring the request",
                        );
                        self.to_agents
                            .error(
                                request_id,
                                "The Request does not contain any content".to_string(),
                            )
                            .await
                            .unwrap_or_illegal_state();
                        continue;
                    };
                    match request_content {
                        // [impl->swdd~server-provides-interface-get-complete-state~2]
                        // [impl->swdd~server-includes-id-in-control-interface-response~1]
                        RequestContent::CompleteStateRequest(complete_state_request) => {
                            log::debug!(
                                "Received CompleteStateRequest with id '{}' and field mask: '{:?}'",
                                request_id,
                                complete_state_request.field_mask
                            );
                            match self.server_state.get_complete_state_by_field_mask(
                                complete_state_request.clone(),
                                &self.workload_states_map,
                                &self.agent_map,
                            ) {
                                Ok(complete_state) => {
                                    self.to_agents
                                        .complete_state(request_id.clone(), complete_state, None)
                                        .await
                                        .unwrap_or_illegal_state();

                                    if complete_state_request.subscribe_for_events {
                                        // [impl->swdd~server-stores-new-event-subscription~1]
                                        self.event_handler.add_subscriber(
                                            request_id,
                                            complete_state_request
                                                .field_mask
                                                .into_iter()
                                                .map(Path::from)
                                                .collect(),
                                        );
                                    }
                                }
                                Err(error) => {
                                    log::error!("Failed to get complete state: '{error}'");
                                    self.to_agents
                                        .complete_state(request_id, CompleteState::default(), None)
                                        .await
                                        .unwrap_or_illegal_state();
                                }
                            }
                        }

                        // [impl->swdd~server-provides-update-desired-state-interface~1]
                        RequestContent::UpdateStateRequest(update_state_request) => {
                            let Some(new_state) = update_state_request.new_state else {
                                log::warn!(
                                    "The UpdateStateRequest does not contain a new state -> ignoring the request"
                                );
                                self.to_agents
                                    .error(
                                        request_id,
                                        "The UpdateStateRequest does not contain a new state"
                                            .to_string(),
                                    )
                                    .await
                                    .unwrap_or_illegal_state();
                                continue;
                            };
                            let update_mask = update_state_request.update_mask;

                            log::debug!(
                                "Received UpdateState. State '{new_state:?}', update mask '{update_mask:?}'"
                            );

                            // [impl->swdd~update-desired-state-with-update-mask~1]
                            // [impl->swdd~update-desired-state-empty-update-mask~1]

                            let state_generation_result = match self
                                .server_state
                                .generate_new_state(new_state, update_mask)
                            {
                                Ok(state_generation_result) => state_generation_result,
                                Err(error_msg) => {
                                    log::error!("Update rejected: '{error_msg}'",);
                                    self.to_agents
                                        .error(
                                            request_id,
                                            format!("Update rejected: '{error_msg}'"),
                                        )
                                        .await
                                        .unwrap_or_illegal_state();
                                    continue;
                                }
                            };

                            // [impl->swdd~update-desired-state-with-invalid-version~1]
                            // [impl->swdd~update-desired-state-with-missing-version~1]
                            // [impl->swdd~server-desired-state-field-conventions~1]
                            // [impl->swdd~server-validates-desired-state-api-version~1]
                            if let Err(error_message) = state_generation_result
                                .new_desired_state
                                .validate_pre_rendering()
                            {
                                log::warn!(
                                    "The CompleteState in the request has wrong format. {error_message} -> ignoring the request"
                                );

                                self.to_agents
                                    .error(request_id, error_message)
                                    .await
                                    .unwrap_or_illegal_state();
                                continue;
                            }

                            match self
                                .server_state
                                .update(state_generation_result.new_desired_state.clone())
                            {
                                Ok(Some(added_deleted_workloads)) => {
                                    self.set_agent_tags(&state_generation_result.new_agent_map);
                                    self.handle_post_update_steps(
                                        request_id,
                                        added_deleted_workloads,
                                        state_generation_result,
                                    )
                                    .await;
                                }
                                Ok(None) => {
                                    log::debug!(
                                        "The current state and new state are identical -> nothing to do"
                                    );
                                    self.set_agent_tags(&state_generation_result.new_agent_map);
                                    self.to_agents
                                        .update_state_success(request_id, vec![], vec![])
                                        .await
                                        .unwrap_or_illegal_state();

                                    if self.event_handler.has_subscribers() {
                                        // [impl->swdd~server-sends-state-differences-as-events~1]
                                        // state changes must be calculated after every update since only config item can be changed as well
                                        let old_state = CompleteStateSpec {
                                            desired_state: state_generation_result
                                                .old_desired_state,
                                            ..Default::default()
                                        };

                                        let new_state = CompleteStateSpec {
                                            desired_state: state_generation_result
                                                .new_desired_state,
                                            ..Default::default()
                                        };

                                        let state_comparator =
                                            StateComparator::new(old_state, new_state);

                                        let state_difference_tree =
                                            state_comparator.state_differences();

                                        if !state_difference_tree.is_empty() {
                                            self.event_handler
                                                .send_events(
                                                    &self.server_state,
                                                    &self.workload_states_map,
                                                    &self.agent_map,
                                                    state_difference_tree,
                                                    &self.to_agents,
                                                )
                                                .await;
                                        }
                                    }
                                }
                                Err(error_msg) => {
                                    // [impl->swdd~server-continues-on-invalid-updated-state~1]
                                    log::error!("Update rejected: '{error_msg}'",);
                                    self.to_agents
                                        .error(
                                            request_id,
                                            format!("Update rejected: '{error_msg}'"),
                                        )
                                        .await
                                        .unwrap_or_illegal_state();
                                }
                            }
                        }
                        // [impl->swdd~server-handles-logs-request-message~1]
                        RequestContent::LogsRequest(mut logs_request) => {
                            log::debug!(
                                "Got log request. Id: '{}', Workload Instance Names: '{:?}'",
                                request_id,
                                logs_request.workload_names
                            );

                            // keep only workload instance names that are currently in the desired state
                            logs_request.workload_names.retain(
                                |name: &ankaios_api::ank_base::WorkloadInstanceName| {
                                    self.server_state.desired_state_contains_instance_name(name)
                                },
                            );
                            if !logs_request.workload_names.is_empty() {
                                log::debug!(
                                    "Requesting logs from agents for the instance names: {:?}",
                                    logs_request.workload_names
                                );
                                self.to_agents
                                    .logs_request(request_id.clone(), logs_request.clone())
                                    .await
                                    .unwrap_or_illegal_state();

                                self.log_campaign_store
                                    .insert_log_campaign(&request_id, &logs_request.workload_names);
                            }

                            self.to_agents
                                .logs_request_accepted(request_id.clone(), logs_request)
                                .await
                                .unwrap_or_illegal_state();
                        }
                        // [impl->swdd~server-handles-logs-cancel-request-message~1]
                        RequestContent::LogsCancelRequest(_) => {
                            log::debug!("Got log cancel request with ID: {request_id}");

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
                        // [impl->swdd~server-removes-event-subscription~1]
                        RequestContent::EventsCancelRequest(_) => {
                            log::debug!("Got event cancel request with ID: {request_id}");

                            self.event_handler.remove_subscriber(request_id.clone());

                            self.to_agents
                                .event_cancel_request_accepted(request_id)
                                .await
                                .unwrap_or_illegal_state();
                        }
                    }
                }
                ToServer::UpdateWorkloadState(method_obj) => {
                    log::debug!(
                        "Received UpdateWorkloadState: '{:?}'",
                        method_obj.workload_states
                    );

                    // [impl->swdd~server-sends-state-differences-as-events~1]
                    let old_state = if self.event_handler.has_subscribers() {
                        CompleteStateSpec {
                            workload_states: self.workload_states_map.clone(),
                            ..Default::default()
                        }
                    } else {
                        CompleteStateSpec::default()
                    };

                    // [impl->swdd~server-stores-workload-state~1]
                    self.workload_states_map
                        .process_new_states(method_obj.workload_states.clone());

                    // [impl->swdd~server-cleans-up-state~1]
                    self.server_state.cleanup_state(&method_obj.workload_states);

                    if self.event_handler.has_subscribers() {
                        // [impl->swdd~server-sends-state-differences-as-events~1]
                        let new_state = CompleteStateSpec {
                            workload_states: self.workload_states_map.clone(),
                            ..Default::default()
                        };

                        let state_comparator = StateComparator::new(old_state, new_state);

                        let state_difference_tree = state_comparator.state_differences();

                        self.event_handler
                            .send_events(
                                &self.server_state,
                                &self.workload_states_map,
                                &self.agent_map,
                                state_difference_tree,
                                &self.to_agents,
                            )
                            .await;

                        // [impl->swdd~server-removes-subscription-for-deleted-subscriber-workload~1]
                        method_obj
                            .workload_states
                            .iter()
                            .for_each(|workload_state| {
                                if workload_state.execution_state.is_removed() {
                                    self.event_handler.remove_workload_subscriber(
                                        &workload_state.instance_name.agent_name().to_owned(),
                                        &workload_state.instance_name.workload_name().to_owned(),
                                    );
                                }
                            });
                    }

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
                    log::debug!("Received LogsStopResponse with ID: {request_id}");
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

                    // [impl->swdd~server-removes-event-subscription-for-disconnected-cli~1]
                    self.event_handler
                        .remove_cli_subscriber(&goodbye.connection_name);
                }
                ToServer::Stop(_method_obj) => {
                    log::debug!("Received Stop from communications server");
                    // TODO: handle the call
                    break;
                }
            }
        }
    }

    // [impl->swdd~server-state-updates-agent-tags~1]
    fn set_agent_tags(&mut self, agent_map: &AgentMapSpec) {
        for (agent_name, new_agent_attributes) in &agent_map.agents {
            if let Some(existing_agent) = self.agent_map.agents.get_mut(agent_name) {
                existing_agent.tags = new_agent_attributes.tags.to_owned();
            }
        }
    }

    async fn handle_post_update_steps(
        &mut self,
        request_id: String,
        added_deleted_workloads: AddedDeletedWorkloads,
        state_generation_result: StateGenerationResult,
    ) {
        let added_workloads = added_deleted_workloads.added_workloads;
        let deleted_workloads = added_deleted_workloads.deleted_workloads;
        log::info!(
            "The update has {} new or updated workloads, {} workloads to delete",
            added_workloads.len(),
            deleted_workloads.len()
        );

        let old_state = if self.event_handler.has_subscribers() {
            CompleteStateSpec {
                desired_state: state_generation_result.old_desired_state,
                workload_states: self.workload_states_map.clone(),
                ..Default::default()
            }
        } else {
            CompleteStateSpec::default()
        };

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
        let (retained_deleted_workloads, deleted_workload_states) = self
            .handle_not_started_deleted_workloads(deleted_workloads)
            .await;

        if self.event_handler.has_subscribers() {
            // [impl->swdd~server-sends-state-differences-as-events~1]
            let new_state = CompleteStateSpec {
                desired_state: state_generation_result.new_desired_state,
                workload_states: self.workload_states_map.clone(),
                ..Default::default()
            };

            let state_comparator = StateComparator::new(old_state, new_state);

            let state_difference_tree = state_comparator.state_differences();

            if !state_difference_tree.is_empty() {
                self.event_handler
                    .send_events(
                        &self.server_state,
                        &self.workload_states_map,
                        &self.agent_map,
                        state_difference_tree,
                        &self.to_agents,
                    )
                    .await;
            }
        }

        if !deleted_workload_states.is_empty() {
            log::debug!(
                "Send UpdateWorkloadState for not started deleted workloads: '{deleted_workload_states:?}'"
            );
            self.to_agents
                .update_workload_state(deleted_workload_states)
                .await
                .unwrap_or_illegal_state();
        }

        self.to_agents
            .update_workload(added_workloads, retained_deleted_workloads)
            .await
            .unwrap_or_illegal_state();

        log::debug!("Send UpdateStateSuccess for request '{request_id}'");
        // [impl->swdd~server-update-state-success-response~1]
        self.to_agents
            .update_state_success(request_id, added_workloads_names, deleted_workloads_names)
            .await
            .unwrap_or_illegal_state();
    }

    // [impl->swdd~server-handles-not-started-deleted-workloads~1]
    async fn handle_not_started_deleted_workloads(
        &mut self,
        mut deleted_workloads: Vec<DeletedWorkload>,
    ) -> (Vec<DeletedWorkload>, Vec<WorkloadStateSpec>) {
        let mut deleted_states = vec![];
        deleted_workloads.retain(|deleted_wl| {
            if deleted_wl.instance_name.agent_name().is_empty()
                || self.deleted_workload_never_started_on_agent(deleted_wl)
            {
                self.workload_states_map.remove(&deleted_wl.instance_name);
                deleted_states.push(WorkloadStateSpec {
                    instance_name: deleted_wl.instance_name.clone(),
                    execution_state: ExecutionStateSpec::removed(),
                });

                return false;
            }
            true
        });

        (deleted_workloads, deleted_states)
    }

    fn deleted_workload_never_started_on_agent(&self, deleted_workload: &DeletedWorkload) -> bool {
        !self
            .agent_map
            .agents
            .contains_key(deleted_workload.instance_name.agent_name())
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
                "Sending logs cancel request for disconnected connection '{connection_name}' with request id: {request_id}"
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

    // [impl->swdd~server-handles-log-campaign-for-disconnected-agent~1]
    async fn send_log_stop_response_for_disconnected_agent(
        &mut self,
        stopped_log_gatherings: Vec<(String, Vec<WorkloadInstanceName>)>,
    ) {
        for (request_id, stopped_log_providers) in stopped_log_gatherings {
            for workload_instance_name in stopped_log_providers {
                self.to_agents
                    .logs_stop_response(
                        request_id.to_owned(),
                        LogsStopResponse {
                            workload_name: Some(workload_instance_name),
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
    use std::vec;

    use super::{
        AnkaiosServer, FromServerSender, create_from_server_channel, create_to_server_channel,
    };
    use crate::ankaios_server::log_campaign_store::RemovedLogRequests;
    use crate::ankaios_server::server_state::{
        AddedDeletedWorkloads, MockServerState, StateGenerationResult, UpdateStateError,
    };
    use crate::ankaios_server::state_comparator::{
        MockStateComparator, StateDifferenceTree, generate_difference_tree_from_paths,
    };

    use ankaios_api::ank_base::{
        AgentAttributesSpec, AgentMapSpec, AgentStatusSpec, CompleteState, CompleteStateRequest,
        CompleteStateResponse, CompleteStateSpec, CpuUsageSpec, DeletedWorkload, Error,
        ExecutionStateEnumSpec, ExecutionStateSpec, FreeMemorySpec, LogEntriesResponse, LogEntry,
        LogsCancelAccepted, LogsRequest, LogsRequestAccepted, LogsStopResponse,
        Pending as PendingSubstate, Response, ResponseContent, State, StateSpec, TagsSpec,
        UpdateStateSuccess, Workload, WorkloadInstanceName, WorkloadInstanceNameSpec, WorkloadMap,
        WorkloadMapSpec, WorkloadStateSpec, WorkloadStatesMapSpec,
    };
    use ankaios_api::test_utils::fixtures::{AGENT_NAMES, REQUEST_ID};
    use ankaios_api::test_utils::{
        fixtures, generate_test_agent_map, generate_test_agent_tags, generate_test_config_map,
        generate_test_workload_instance_name_with_params, generate_test_workload_named,
        generate_test_workload_named_with_params, generate_test_workload_state,
        generate_test_workload_state_with_agent, generate_test_workload_states_map_with_data,
        generate_test_workload_with_params,
    };
    use common::commands::{AgentLoadStatus, ServerHello, UpdateWorkload, UpdateWorkloadState};
    use common::from_server_interface::FromServer;
    use common::to_server_interface::ToServerInterface;

    use mockall::predicate;

    const SECOND_REQUEST_ID: &str = "request_id_2";
    const MESSAGE: &str = "message";
    const CLI_CONNECTION_NAME: &str = "cli-conn-1234";

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-fails-on-invalid-startup-state~1]
    #[tokio::test]
    async fn utest_server_start_fail_on_invalid_startup_manifest() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (_to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        // contains a self cycle to workload_A
        let workload = generate_test_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let startup_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(fixtures::WORKLOAD_NAMES[0].to_string(), workload)]),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();

        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(startup_state.desired_state.clone()))
            .once()
            .return_const(Err(UpdateStateError::CycleInDependencies(
                fixtures::WORKLOAD_NAMES[0].to_string() + " part of cycle.",
            )));
        server.server_state = mock_server_state;

        let result = server.start(Some(startup_state)).await;
        assert_eq!(
            result,
            Err(format!(
                "workload dependency '{} part of cycle.' is part of a cycle.",
                fixtures::WORKLOAD_NAMES[0]
            ))
        );

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-fails-on-invalid-startup-state~1]
    // [utest->swdd~server-validates-desired-state-api-version~1]
    #[tokio::test]
    async fn utest_server_start_fail_on_startup_manifest_with_invalid_version() {
        let (_to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let startup_state = CompleteStateSpec {
            desired_state: StateSpec {
                api_version: "invalidVersion".into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let result = server.start(Some(startup_state)).await;
        assert_eq!(
            result,
            Err("Unsupported API version. Received 'invalidVersion', expected 'v1'".into())
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
        let mut updated_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let new_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        updated_workload.instance_name.workload_name().to_owned(),
                        updated_workload.workload.clone(),
                    )]),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // fix new state by deleting the dependencies
        let mut fixed_state = new_state.clone();
        updated_workload.workload.dependencies.dependencies.clear();
        fixed_state.desired_state.workloads.workloads = HashMap::from([(
            updated_workload.instance_name.workload_name().to_owned(),
            updated_workload.workload.clone(),
        )]);

        let update_mask = vec!["desiredState.workloads".to_string()];

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        let mut seq = mockall::Sequence::new();
        let new_desired_state = new_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .in_sequence(&mut seq)
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: new_desired_state.clone(),
                    ..Default::default()
                })
            });
        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(new_state.desired_state.clone()))
            .once()
            .in_sequence(&mut seq)
            .return_const(Err(UpdateStateError::CycleInDependencies(
                fixtures::WORKLOAD_NAMES[0].to_string(),
            )));

        server.event_handler.expect_has_subscribers().never();

        let added_workloads = vec![updated_workload.clone()];
        let deleted_workloads = vec![];

        let fixed_desired_state = fixed_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .in_sequence(&mut seq)
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: fixed_desired_state.clone(),
                    ..Default::default()
                })
            });
        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(fixed_state.desired_state.clone()))
            .once()
            .in_sequence(&mut seq)
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads: added_workloads.clone(),
                deleted_workloads: deleted_workloads.clone(),
            })));

        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .in_sequence(&mut seq)
            .return_const(false);

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send the new invalid state update
        assert!(
            to_server
                .update_state(
                    fixtures::REQUEST_ID.to_string(),
                    new_state.clone().into(),
                    update_mask.clone()
                )
                .await
                .is_ok()
        );

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::Error(_))
            }) if request_id == fixtures::REQUEST_ID
        ));

        // send the update with the new clean state again
        assert!(
            to_server
                .update_state(
                    fixtures::REQUEST_ID.to_string(),
                    fixed_state.clone().into(),
                    update_mask
                )
                .await
                .is_ok()
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        let expected_from_server_command = FromServer::UpdateWorkload(UpdateWorkload {
            added_workloads,
            deleted_workloads,
        });
        assert_eq!(from_server_command, expected_from_server_command);

        assert_eq!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.into(),
                response_content: Some(ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                    added_workloads: vec![updated_workload.instance_name.to_string()],
                    deleted_workloads: Vec::new(),
                })),
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

        let workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let startup_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        workload.instance_name.workload_name().to_owned(),
                        workload.workload.clone(),
                    )]),
                },
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
            .with(mockall::predicate::eq(startup_state.desired_state.clone()))
            .once()
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads: added_workloads.clone(),
                deleted_workloads: deleted_workloads.clone(),
            })));

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
                .get_workload_state_for_agent(fixtures::AGENT_NAMES[0]),
            vec![WorkloadStateSpec {
                instance_name: workload.instance_name,
                execution_state: ExecutionStateSpec {
                    execution_state_enum: ExecutionStateEnumSpec::Pending(PendingSubstate::Initial),
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
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w2 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
        );

        let mut mock_server_state = MockServerState::new();

        mock_server_state.expect_cleanup_state().return_const(());

        let mut seq = mockall::Sequence::new();
        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(fixtures::AGENT_NAMES[0].to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w1.clone()]);

        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(fixtures::AGENT_NAMES[1].to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w2.clone()]);

        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .return_const(false);

        let server_task = tokio::spawn(async move { server.start(None).await });

        // first agent connects to the server
        let agent_hello_result = to_server
            .agent_hello(
                fixtures::AGENT_NAMES[0].to_string(),
                generate_test_agent_tags().into(),
            )
            .await;
        assert!(agent_hello_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::ServerHello(ServerHello {
                agent_name: Some(fixtures::AGENT_NAMES[0].to_string()),
                added_workloads: vec![w1],
            }),
            from_server_command
        );

        // [utest->swdd~server-informs-a-newly-connected-agent-workload-states~1]
        // [utest->swdd~server-starts-without-startup-config~1]
        // send update_workload_state for first agent which is then stored in the workload_state_db in ankaios server
        let test_wl_1_state_running = generate_test_workload_state(
            fixtures::WORKLOAD_NAMES[0],
            ExecutionStateSpec::running(),
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

        let agent_hello_result = to_server
            .agent_hello(
                fixtures::AGENT_NAMES[1].to_owned(),
                generate_test_agent_tags().into(),
            )
            .await;
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
                agent_name: Some(fixtures::AGENT_NAMES[1].to_string()),
                added_workloads: vec![w2],
            }),
            from_server_command
        );

        // [utest->swdd~server-forwards-workload-state~1]
        // send update_workload_state for second agent which is then stored in the workload_state_db in ankaios server
        let test_wl_2_state_succeeded = generate_test_workload_state(
            fixtures::WORKLOAD_NAMES[1],
            ExecutionStateSpec::succeeded(),
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
        let test_wl_1_state_succeeded = generate_test_workload_state(
            fixtures::WORKLOAD_NAMES[1],
            ExecutionStateSpec::succeeded(),
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

        let mut w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        w1.workload.runtime_config = "changed".to_string();

        let update_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        fixtures::WORKLOAD_NAMES[0].to_owned(),
                        w1.workload.clone(),
                    )]),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let added_workloads = vec![w1.clone()];
        let deleted_workloads = vec![];

        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[0]
        )];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        let updated_desired_state = update_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: updated_desired_state.clone(),
                    ..Default::default()
                })
            });
        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(update_state.desired_state.clone()))
            .once()
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads: added_workloads.clone(),
                deleted_workloads: deleted_workloads.clone(),
            })));
        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(false);

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask,
            )
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
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                    added_workloads: added_workloads
                        .into_iter()
                        .map(|x| x.instance_name.to_string())
                        .collect(),
                    deleted_workloads: deleted_workloads
                        .into_iter()
                        .map(|x| x.instance_name.to_string())
                        .collect()
                }))
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
    async fn utest_server_sends_workloads_and_workload_states_when_requested_update_state_nothing_to_do()
     {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        w1.workload.runtime_config = "changed".to_string();

        let update_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        fixtures::WORKLOAD_NAMES[0].to_owned(),
                        w1.workload.clone(),
                    )]),
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[0]
        )];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        let updated_desired_state = update_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: updated_desired_state.clone(),
                    ..Default::default()
                })
            });
        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(update_state.desired_state.clone()))
            .once()
            .return_const(Ok(None));
        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .once()
            .return_const(false);

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                    added_workloads,
                    deleted_workloads
                }))
            }) if request_id == fixtures::REQUEST_ID && added_workloads.is_empty() && deleted_workloads.is_empty()
        ));

        assert!(
            tokio::time::timeout(
                tokio::time::Duration::from_millis(200),
                comm_middle_ware_receiver.recv()
            )
            .await
            .is_err()
        );

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

        let w1 = generate_test_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let update_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        fixtures::WORKLOAD_NAMES[0].to_owned(),
                        w1.clone(),
                    )]),
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[0]
        )];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        let updated_desired_state = update_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: updated_desired_state.clone(),
                    ..Default::default()
                })
            });
        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(update_state.desired_state.clone()))
            .once()
            .return_const(Err(UpdateStateError::ResultInvalid(
                "some update error.".to_string(),
            )));
        server.server_state = mock_server_state;
        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::Error(_))
            }) if request_id == fixtures::REQUEST_ID
        ));

        assert!(
            tokio::time::timeout(
                tokio::time::Duration::from_millis(200),
                comm_middle_ware_receiver.recv()
            )
            .await
            .is_err()
        );

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
                        == &WorkloadInstanceName {
                            agent_name: fixtures::AGENT_NAMES[0].to_string(),
                            workload_name: fixtures::WORKLOAD_NAMES[0].to_string(),
                            id: fixtures::WORKLOAD_IDS[0].to_string(),
                        }
                },
            ))
            .once()
            .return_const(true);

        server.server_state = mock_server_state;

        let log_providing_workloads = vec![WorkloadInstanceName {
            workload_name: fixtures::WORKLOAD_NAMES[0].to_string(),
            agent_name: fixtures::AGENT_NAMES[0].to_string(),
            id: fixtures::WORKLOAD_IDS[0].to_string(),
        }];

        server
            .log_campaign_store
            .expect_insert_log_campaign()
            .with(
                predicate::eq(fixtures::REQUEST_ID.to_owned()),
                predicate::eq(log_providing_workloads.clone()),
            )
            .once()
            .return_const(());

        let server_task = tokio::spawn(async move { server.start(None).await });

        let logs_request = LogsRequest {
            workload_names: log_providing_workloads,
            follow: Some(true),
            tail: Some(10),
            since: None,
            until: None,
        };

        // send logs request to server
        let logs_request_result = to_server
            .logs_request(fixtures::REQUEST_ID.to_string(), logs_request)
            .await;
        assert!(logs_request_result.is_ok());
        drop(to_server);

        let logs_request_message = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::LogsRequest(
                fixtures::REQUEST_ID.into(),
                LogsRequest {
                    workload_names: vec![WorkloadInstanceName {
                        agent_name: fixtures::AGENT_NAMES[0].to_string(),
                        workload_name: fixtures::WORKLOAD_NAMES[0].to_string(),
                        id: fixtures::WORKLOAD_IDS[0].to_string(),
                    }],
                    follow: Some(true),
                    tail: Some(10),
                    since: None,
                    until: None
                }
            ),
            logs_request_message
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            from_server_command,
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                    workload_names: vec![WorkloadInstanceName {
                        workload_name: fixtures::WORKLOAD_NAMES[0].to_string(),
                        agent_name: fixtures::AGENT_NAMES[0].to_string(),
                        id: fixtures::WORKLOAD_IDS[0].to_string()
                    }],
                })),
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
            .with(mockall::predicate::eq(WorkloadInstanceName {
                agent_name: fixtures::AGENT_NAMES[0].to_string(),
                workload_name: fixtures::WORKLOAD_NAMES[0].to_string(),
                id: fixtures::WORKLOAD_IDS[0].to_string(),
            }))
            .once()
            .return_const(false);

        server.server_state = mock_server_state;

        server
            .log_campaign_store
            .expect_insert_log_campaign()
            .never();

        let logs_request = LogsRequest {
            workload_names: vec![WorkloadInstanceName {
                agent_name: fixtures::AGENT_NAMES[0].to_string(),
                workload_name: fixtures::WORKLOAD_NAMES[0].to_string(),
                id: fixtures::WORKLOAD_IDS[0].to_string(),
            }],
            follow: Some(true),
            tail: Some(10),
            since: None,
            until: None,
        };

        // send logs request to server
        let logs_request_result = to_server
            .logs_request(fixtures::REQUEST_ID.to_string(), logs_request)
            .await;
        assert!(logs_request_result.is_ok());

        assert!(to_server.stop().await.is_ok());
        let server_result = server.start(None).await;

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            from_server_command,
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                    workload_names: vec![],
                })),
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

        let w1: Workload = generate_test_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        )
        .into();
        let w2 = w1.clone();
        let w3 = Workload {
            agent: Some(fixtures::AGENT_NAMES[1].to_string()),
            ..w1.clone()
        };

        let workloads = HashMap::from([
            (fixtures::WORKLOAD_NAMES[0].to_owned(), w1),
            (fixtures::WORKLOAD_NAMES[1].to_owned(), w2),
            (fixtures::WORKLOAD_NAMES[2].to_owned(), w3),
        ]);

        let workload_map = WorkloadMap { workloads };

        let current_complete_state = CompleteState {
            desired_state: Some(State {
                workloads: Some(workload_map),
                ..Default::default()
            }),
            ..Default::default()
        };
        let request_id = format!("{}@my_request_id", fixtures::AGENT_NAMES[0]);
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_get_complete_state_by_field_mask()
            .with(
                mockall::predicate::function(|request_complete_state| {
                    request_complete_state
                        == &CompleteStateRequest {
                            field_mask: vec![],
                            subscribe_for_events: false,
                        }
                }),
                mockall::predicate::always(),
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
                CompleteStateRequest {
                    field_mask: vec![],
                    subscribe_for_events: false,
                },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            from_server_command,
            common::from_server_interface::FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                    CompleteStateResponse {
                        complete_state: Some(current_complete_state),
                        ..Default::default()
                    }
                )))
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
                    request_complete_state
                        == &CompleteStateRequest {
                            field_mask: vec![],
                            subscribe_for_events: false,
                        }
                }),
                mockall::predicate::always(),
                mockall::predicate::always(),
            )
            .once()
            .return_const(Err("complete state error.".to_string()));
        server.server_state = mock_server_state;
        let server_task = tokio::spawn(async move { server.start(None).await });

        let request_id = format!("{}@my_request_id", fixtures::AGENT_NAMES[0]);
        // send command 'CompleteStateRequest'
        // CompleteState shall contain the complete state
        let request_complete_state_result = to_server
            .request_complete_state(
                request_id.clone(),
                CompleteStateRequest {
                    field_mask: vec![],
                    subscribe_for_events: false,
                },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        let expected_complete_state = CompleteState {
            ..Default::default()
        };

        assert_eq!(
            from_server_command,
            common::from_server_interface::FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                    CompleteStateResponse {
                        complete_state: Some(expected_complete_state),
                        ..Default::default()
                    }
                )))
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

        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .times(4)
            .return_const(false);

        // send update_workload_state for first agent which is then stored in the workload_state_db in ankaios server
        let test_wl_1_state_running = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );
        let update_workload_state_result = to_server
            .update_workload_state(vec![test_wl_1_state_running.clone()])
            .await;
        assert!(update_workload_state_result.is_ok());

        // first agent disconnects from the ankaios server
        let agent_gone_result = to_server
            .agent_gone(fixtures::AGENT_NAMES[0].to_owned())
            .await;
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
            .get_workload_state_for_agent(fixtures::AGENT_NAMES[0]);

        let expected_workload_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::agent_disconnected(),
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
            .with(predicate::eq(fixtures::AGENT_NAMES[0].to_owned()))
            .once()
            .return_const(RemovedLogRequests {
                collector_requests: HashSet::from([
                    fixtures::REQUEST_ID.to_owned(),
                    SECOND_REQUEST_ID.to_owned(),
                ]),
                ..Default::default()
            });

        let mut mock_server_state = MockServerState::new();
        mock_server_state.expect_cleanup_state().never();

        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(false);

        let agent_gone_result = to_server
            .agent_gone(fixtures::AGENT_NAMES[0].to_owned())
            .await;
        assert!(agent_gone_result.is_ok());

        let server_task = tokio::spawn(async move { server.start(None).await });

        let expected_logs_cancel_requests = [
            FromServer::LogsCancelRequest(fixtures::REQUEST_ID.to_string()),
            FromServer::LogsCancelRequest(SECOND_REQUEST_ID.to_string()),
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
                "Actual request: '{request:?}' not found in expected requests."
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

        let instance_name_1: WorkloadInstanceName =
            generate_test_workload_instance_name_with_params(
                fixtures::WORKLOAD_NAMES[0],
                fixtures::AGENT_NAMES[0],
            )
            .into();
        let instance_name_2: WorkloadInstanceName =
            generate_test_workload_instance_name_with_params(
                fixtures::WORKLOAD_NAMES[1],
                fixtures::AGENT_NAMES[0],
            )
            .into();
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_agent_log_campaign_entry()
            .with(predicate::eq(fixtures::AGENT_NAMES[0].to_owned()))
            .once()
            .return_const(RemovedLogRequests {
                collector_requests: HashSet::new(),
                disconnected_log_providers: vec![(
                    fixtures::REQUEST_ID.to_owned(),
                    vec![instance_name_1.clone(), instance_name_2.clone()],
                )],
            });

        let mut mock_server_state = MockServerState::new();
        mock_server_state.expect_cleanup_state().never();

        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(false);

        let agent_gone_result = to_server
            .agent_gone(fixtures::AGENT_NAMES[0].to_owned())
            .await;
        assert!(agent_gone_result.is_ok());

        let server_task = tokio::spawn(async move { server.start(None).await });

        let expected_logs_stop_responses = [
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::LogsStopResponse(LogsStopResponse {
                    workload_name: Some(instance_name_1),
                })),
            }),
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::LogsStopResponse(LogsStopResponse {
                    workload_name: Some(instance_name_2),
                })),
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
                "Actual request: '{request:?}' not found in expected requests."
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

        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w2 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
        );

        let mut updated_w1 = w1.clone();
        updated_w1.instance_name = WorkloadInstanceNameSpec::builder()
            .workload_name(w1.instance_name.workload_name())
            .agent_name(w1.instance_name.agent_name())
            .config(&String::from("changed"))
            .build();
        let update_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        fixtures::WORKLOAD_NAMES[0].to_owned(),
                        updated_w1.workload.clone(),
                    )]),
                },
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
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(fixtures::AGENT_NAMES[0].to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w1.clone()]);

        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(fixtures::AGENT_NAMES[1].to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w2.clone()]);

        let updated_desired_state = update_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .in_sequence(&mut seq)
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: updated_desired_state.clone(),
                    ..Default::default()
                })
            });

        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(update_state.desired_state.clone()))
            .once()
            .in_sequence(&mut seq)
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads,
                deleted_workloads,
            })));
        server.server_state = mock_server_state;

        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .return_const(HashSet::new());

        server
            .event_handler
            .expect_has_subscribers()
            .times(6)
            .return_const(false);

        let agent_hello1_result = to_server
            .agent_hello(fixtures::AGENT_NAMES[0].to_owned(), Default::default())
            .await;
        assert!(agent_hello1_result.is_ok());

        let agent_hello2_result = to_server
            .agent_hello(fixtures::AGENT_NAMES[1].to_owned(), Default::default())
            .await;
        assert!(agent_hello2_result.is_ok());

        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask.clone(),
            )
            .await;
        assert!(update_state_result.is_ok());

        let server_handle = server.start(None);

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        tokio::join!(server_handle).0.unwrap();

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::ServerHello(ServerHello {
                agent_name: Some(fixtures::AGENT_NAMES[0].to_string()),
                added_workloads: vec![w1.clone()]
            }),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::ServerHello(ServerHello {
                agent_name: Some(fixtures::AGENT_NAMES[1].to_string()),
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
            FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                    added_workloads,
                    deleted_workloads
                }))
            }) if request_id == fixtures::REQUEST_ID && added_workloads == vec![updated_w1.instance_name.to_string()] && deleted_workloads == vec![w1.instance_name.to_string()]
        ));

        assert_eq!(
            server
                .workload_states_map
                .get_workload_state_for_agent(fixtures::AGENT_NAMES[0]),
            vec![WorkloadStateSpec {
                instance_name: updated_w1.instance_name,
                execution_state: ExecutionStateSpec {
                    execution_state_enum: ExecutionStateEnumSpec::Pending(PendingSubstate::Initial),
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

        assert!(
            to_server
                .log_entries_response(
                    fixtures::REQUEST_ID.into(),
                    LogEntriesResponse {
                        log_entries: vec![LogEntry {
                            workload_name: Some(WorkloadInstanceName {
                                workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                                agent_name: fixtures::AGENT_NAMES[0].into(),
                                id: fixtures::WORKLOAD_IDS[0].into()
                            }),
                            message: MESSAGE.into()
                        }]
                    }
                )
                .await
                .is_ok()
        );

        assert_eq!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.into(),
                response_content: Some(ResponseContent::LogEntriesResponse(LogEntriesResponse {
                    log_entries: vec![LogEntry {
                        workload_name: Some(WorkloadInstanceName {
                            workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                            agent_name: fixtures::AGENT_NAMES[0].into(),
                            id: fixtures::WORKLOAD_IDS[0].into()
                        }),
                        message: MESSAGE.into()
                    },]
                }))
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

        let update_state = CompleteStateSpec {
            desired_state: StateSpec {
                api_version: "incompatible_version".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let update_mask = vec![format!("desiredState")];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        let mut mock_server_state = MockServerState::new();
        let new_desired_state = update_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .return_once(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state,
                    ..Default::default()
                })
            });
        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        let error_message = format!(
            "Unsupported API version. Received 'incompatible_version', expected '{}'",
            StateSpec::default().api_version
        );
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::Error(Error {
                    message: error_message
                })),
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

        let update_state_ankaios_no_version = CompleteStateSpec {
            desired_state: StateSpec {
                api_version: "".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let update_mask = vec![format!("desiredState")];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        let mut mock_server_state = MockServerState::new();
        let new_desired_state = update_state_ankaios_no_version.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .return_once(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state,
                    ..Default::default()
                })
            });
        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state_ankaios_no_version.into(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        let error_message = format!(
            "Unsupported API version. Received '', expected '{}'",
            StateSpec::default().api_version
        );
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::Error(Error {
                    message: error_message
                })),
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

        let workload_states = vec![generate_test_workload_state(
            fixtures::WORKLOAD_NAMES[0],
            ExecutionStateSpec::removed(),
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

        let workload_without_agent = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            "",
            fixtures::RUNTIME_NAMES[0],
        );

        let workload_with_agent = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
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
            .expect_generate_new_state()
            .once()
            .returning(move |_, _| Ok(StateGenerationResult::default()));
        mock_server_state
            .expect_update()
            .once()
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads: Vec::default(),
                deleted_workloads: deleted_workloads.clone(),
            })));
        server.server_state = mock_server_state;

        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .return_const(HashSet::new());

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(false);

        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state,
                update_mask.clone(),
            )
            .await;
        assert!(update_state_result.is_ok());

        let server_handle = server.start(None);

        // The receiver in the server receives the messages and terminates the infinite waiting-loop
        drop(to_server);
        tokio::join!(server_handle).0.unwrap();

        // the server sends the ExecutionStateSpec removed for the workload with an empty agent name
        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadStateSpec {
                    instance_name: workload_without_agent.instance_name,
                    execution_state: ExecutionStateSpec::removed()
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

        let log_collecting_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
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
            .expect_generate_new_state()
            .once()
            .returning(move |_, _| Ok(StateGenerationResult::default()));

        mock_server_state
            .expect_update()
            .once()
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads: Vec::default(),
                deleted_workloads: deleted_workloads.clone(),
            })));
        server.server_state = mock_server_state;

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(false);

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
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state,
                update_mask.clone(),
            )
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

    // [utest->swdd~server-receives-resource-availability~2]
    #[tokio::test]
    async fn utest_server_receives_agent_status_load() {
        let payload = AgentLoadStatus {
            agent_name: fixtures::AGENT_NAMES[0].to_string(),
            cpu_usage: fixtures::CPU_USAGE_SPEC,
            free_memory: fixtures::FREE_MEMORY_SPEC,
        };

        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .agent_map
            .agents
            .entry(fixtures::AGENT_NAMES[0].to_owned())
            .insert_entry(AgentAttributesSpec {
                tags: generate_test_agent_tags(),
                ..Default::default()
            });

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(false);

        let agent_resource_result = to_server.agent_load_status(payload).await;
        assert!(agent_resource_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;

        assert!(result.is_ok());

        let expected_agent_map: AgentMapSpec = generate_test_agent_map(fixtures::AGENT_NAMES[0]);

        assert_eq!(expected_agent_map, server.agent_map);
    }

    // [utest->swdd~server-stores-newly-connected-agent~2]
    #[tokio::test]
    async fn utest_server_stores_newly_connected_agents() {
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .server_state
            .expect_get_workloads_for_agent()
            .times(2)
            .return_const(Vec::default());

        server
            .event_handler
            .expect_has_subscribers()
            .times(4)
            .return_const(false);

        let agent_resource_result = to_server
            .agent_hello(fixtures::AGENT_NAMES[0].to_owned(), Default::default())
            .await;
        assert!(agent_resource_result.is_ok());
        let agent_resource_result = to_server
            .agent_hello(fixtures::AGENT_NAMES[1].to_owned(), Default::default())
            .await;
        assert!(agent_resource_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;

        assert!(result.is_ok());

        let mut expected_agent_map = AgentMapSpec {
            agents: HashMap::new(),
        };
        expected_agent_map
            .agents
            .entry(fixtures::AGENT_NAMES[0].to_owned())
            .or_default();
        expected_agent_map
            .agents
            .entry(fixtures::AGENT_NAMES[1].to_owned())
            .or_default();

        assert_eq!(expected_agent_map, server.agent_map);
    }

    // [utest->swdd~server-removes-disconnected-agents-from-state~2]
    #[tokio::test]
    async fn utest_server_removes_disconnected_agents_from_agent_map() {
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_agent_log_campaign_entry()
            .times(2)
            .return_const(RemovedLogRequests::default());

        server
            .event_handler
            .expect_has_subscribers()
            .times(4)
            .return_const(false);

        let mut agent_map = AgentMapSpec {
            agents: HashMap::new(),
        };
        agent_map
            .agents
            .entry(fixtures::AGENT_NAMES[0].to_owned())
            .or_default();
        agent_map
            .agents
            .entry(fixtures::AGENT_NAMES[1].to_owned())
            .or_default();
        server.agent_map = agent_map;

        let agent_resource_result = to_server
            .agent_gone(fixtures::AGENT_NAMES[0].to_owned())
            .await;
        assert!(agent_resource_result.is_ok());
        let agent_resource_result = to_server
            .agent_gone(fixtures::AGENT_NAMES[1].to_owned())
            .await;
        assert!(agent_resource_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;

        assert!(result.is_ok());

        assert_eq!(AgentMapSpec::default(), server.agent_map);
    }

    // [utest->swdd~server-handles-not-started-deleted-workloads~1]
    #[tokio::test]
    async fn utest_server_handles_pending_initial_deleted_workload_on_not_connected_agent() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        let workload = generate_test_workload_named();

        let update_state = CompleteStateSpec::default();

        let deleted_workload_with_not_connected_agent = DeletedWorkload {
            instance_name: workload.instance_name.clone(),
            ..Default::default()
        };

        let deleted_workloads = vec![deleted_workload_with_not_connected_agent];

        let mut mock_server_state = MockServerState::new();
        let updated_desired_state = update_state.desired_state.clone();
        mock_server_state
            .expect_generate_new_state()
            .once()
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: updated_desired_state.clone(),
                    ..Default::default()
                })
            });
        mock_server_state
            .expect_update()
            .once()
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads: Vec::default(),
                deleted_workloads,
            })));
        server.server_state = mock_server_state;

        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .once()
            .return_const(HashSet::new());

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(false);

        server.workload_states_map = generate_test_workload_states_map_with_data(
            workload.instance_name.agent_name(),
            workload.instance_name.workload_name(),
            workload.instance_name.id(),
            ExecutionStateSpec::initial(),
        );

        assert!(
            to_server
                .update_state(
                    fixtures::REQUEST_ID.to_owned(),
                    update_state.into(),
                    vec![format!(
                        "desiredState.workloads.{}",
                        fixtures::WORKLOAD_NAMES[0]
                    )],
                )
                .await
                .is_ok()
        );

        drop(to_server);
        let result = server.start(None).await;

        assert!(result.is_ok());

        assert_eq!(
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                comm_middle_ware_receiver.recv(),
            )
            .await,
            Ok(Some(FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadStateSpec {
                    instance_name: workload.instance_name,
                    execution_state: ExecutionStateSpec::removed()
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

        let request_id = fixtures::REQUEST_ID.to_string();
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
            FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::LogsCancelAccepted(LogsCancelAccepted {})),
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

        let request_id = fixtures::REQUEST_ID.to_string();
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let server_task = tokio::spawn(async move { server.start(None).await });

        let workload_instance_name = WorkloadInstanceName {
            workload_name: fixtures::WORKLOAD_NAMES[0].to_string(),
            agent_name: fixtures::AGENT_NAMES[0].to_string(),
            id: fixtures::WORKLOAD_IDS[0].to_string(),
        };

        // send new state to server
        let result = to_server
            .logs_stop_response(
                request_id.clone(),
                LogsStopResponse {
                    workload_name: Some(workload_instance_name.clone()),
                },
            )
            .await;
        assert!(result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::Response(Response {
                request_id: request_id.clone(),
                response_content: Some(ResponseContent::LogsStopResponse(LogsStopResponse {
                    workload_name: Some(workload_instance_name),
                }))
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

        let cli_request_id = format!("{CLI_CONNECTION_NAME}@cli-request-id-1");
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_cli_log_campaign_entry()
            .with(mockall::predicate::eq(CLI_CONNECTION_NAME.to_owned()))
            .once()
            .return_const(HashSet::from([cli_request_id.clone()]));
        server
            .event_handler
            .expect_remove_cli_subscriber()
            .with(mockall::predicate::eq(CLI_CONNECTION_NAME.to_owned()))
            .once()
            .return_const(());

        let server_task = tokio::spawn(async move { server.start(None).await });

        let result = to_server.goodbye(CLI_CONNECTION_NAME.to_owned()).await;
        assert!(result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::LogsCancelRequest(cli_request_id),
            from_server_command
        );

        let result = to_server.stop().await;
        assert!(result.is_ok());

        assert!(server_task.await.is_ok());
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-stores-new-event-subscription~1]
    #[tokio::test]
    async fn utest_server_adds_event_subscribers_upon_complete_state_request() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let current_complete_state = CompleteState::default();

        let request_id = format!("{}@my_request_id", fixtures::AGENT_NAMES[0]);
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .server_state
            .expect_get_complete_state_by_field_mask()
            .once()
            .return_const(Ok(current_complete_state.clone()));

        server
            .event_handler
            .expect_add_subscriber()
            .with(
                mockall::predicate::eq(request_id.clone()),
                mockall::predicate::eq(vec![
                    "desiredState.workloads.workload_1".into(),
                    "workloadStates.*".into(),
                ]),
            )
            .once()
            .return_const(());

        // send command 'CompleteStateRequest' with enabled event subscription
        let request_complete_state_result = to_server
            .request_complete_state(
                request_id.clone(),
                CompleteStateRequest {
                    field_mask: vec![
                        "desiredState.workloads.workload_1".to_owned(),
                        "workloadStates.*".to_owned(),
                    ],
                    subscribe_for_events: true,
                },
            )
            .await;
        assert!(request_complete_state_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            from_server_command,
            common::from_server_interface::FromServer::Response(ankaios_api::ank_base::Response {
                request_id,
                response_content: Some(
                    ankaios_api::ank_base::response::ResponseContent::CompleteStateResponse(
                        Box::new(CompleteStateResponse {
                            complete_state: Some(current_complete_state),
                            ..Default::default()
                        })
                    )
                )
            })
        );

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-removes-event-subscription~1]
    #[tokio::test]
    async fn utest_server_removes_event_subscribers_upon_events_cancel_request() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let request_id = format!("{}@my_request_id", fixtures::AGENT_NAMES[0]);
        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        server
            .event_handler
            .expect_remove_subscriber()
            .with(mockall::predicate::eq(request_id.clone()))
            .once()
            .return_const(());

        let events_cancel_request_result = to_server.event_cancel_request(request_id.clone()).await;
        assert!(events_cancel_request_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            from_server_command,
            common::from_server_interface::FromServer::Response(ankaios_api::ank_base::Response {
                request_id,
                response_content: Some(
                    ankaios_api::ank_base::response::ResponseContent::EventsCancelAccepted(
                        ankaios_api::ank_base::EventsCancelAccepted {}
                    )
                )
            })
        );

        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-sends-state-differences-as-events~1]
    #[tokio::test]
    async fn utest_server_sends_events_for_newly_connected_agents() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        server.server_state.expect_cleanup_state().return_const(());

        server
            .server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(fixtures::AGENT_NAMES[0].to_string()))
            .once()
            .return_const(Vec::default());

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(true);

        let mut state_diference_tree = StateDifferenceTree::new();

        state_diference_tree.added_tree.first_difference_tree =
            generate_difference_tree_from_paths(&[vec![
                "agents".to_owned(),
                fixtures::AGENT_NAMES[0].to_owned(),
            ]]);

        state_diference_tree.added_tree.full_difference_tree = state_diference_tree
            .added_tree
            .first_difference_tree
            .clone();

        let mock_state_comparator_context = MockStateComparator::new_context();
        let mut mock_state_comparator = MockStateComparator::default();
        mock_state_comparator
            .expect_state_differences()
            .once()
            .return_const(state_diference_tree.clone());
        mock_state_comparator_context
            .expect()
            .once()
            .return_once(|_, _| mock_state_comparator);

        server
            .event_handler
            .expect_send_events()
            .with(
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::eq(state_diference_tree),
                mockall::predicate::always(),
            )
            .once()
            .return_const(());

        let agent_hello_result = to_server
            .agent_hello(fixtures::AGENT_NAMES[0].to_string(), Default::default())
            .await;
        assert!(agent_hello_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-sends-state-differences-as-events~1]
    #[tokio::test]
    async fn utest_server_sends_events_for_disconnected_agents() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_agent_log_campaign_entry()
            .once()
            .return_const(RemovedLogRequests::default());

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(true);

        let test_wl_1_state_running = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );

        let mut state_diference_tree = StateDifferenceTree::new();
        state_diference_tree.removed_tree.first_difference_tree =
            generate_difference_tree_from_paths(&[vec![
                "agents".to_owned(),
                fixtures::AGENT_NAMES[0].to_owned(),
            ]]);

        state_diference_tree.removed_tree.full_difference_tree = state_diference_tree
            .removed_tree
            .first_difference_tree
            .clone();

        let wl_state = test_wl_1_state_running.clone();
        state_diference_tree.updated_tree.full_difference_tree =
            generate_difference_tree_from_paths(&[vec![
                "workloadStates".to_owned(),
                wl_state.instance_name.agent_name().to_owned(),
                wl_state.instance_name.workload_name().to_owned(),
                wl_state.instance_name.id().to_owned(),
            ]]);

        let mock_state_comparator_context = MockStateComparator::new_context();
        let mut mock_state_comparator = MockStateComparator::default();
        mock_state_comparator
            .expect_state_differences()
            .once()
            .return_const(state_diference_tree.clone());
        mock_state_comparator_context
            .expect()
            .once()
            .return_once(|_, _| mock_state_comparator);

        server
            .event_handler
            .expect_send_events()
            .with(
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::eq(state_diference_tree),
                mockall::predicate::always(),
            )
            .once()
            .return_const(());

        server
            .event_handler
            .expect_remove_subscribers_of_agent()
            .with(mockall::predicate::eq(fixtures::AGENT_NAMES[0].to_owned()))
            .once()
            .return_const(());

        server
            .workload_states_map
            .process_new_states(vec![test_wl_1_state_running]);

        // first agent disconnects from the ankaios server
        let agent_gone_result = to_server
            .agent_gone(fixtures::AGENT_NAMES[0].to_owned())
            .await;
        assert!(agent_gone_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-sends-state-differences-as-events~1]
    #[tokio::test]
    async fn utest_server_sends_events_upon_update_state_with_updated_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0].to_owned(),
            fixtures::RUNTIME_NAMES[0].to_string(),
        );

        let updated_w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[1].to_owned(),
            fixtures::RUNTIME_NAMES[0].to_string(),
        );

        let mut old_state = CompleteStateSpec::default();
        old_state.desired_state.workloads = WorkloadMapSpec {
            workloads: vec![(fixtures::WORKLOAD_NAMES[0].to_owned(), w1.workload.clone())]
                .into_iter()
                .collect(),
        };

        let mut update_state = CompleteStateSpec::default();
        update_state.desired_state.workloads = WorkloadMapSpec {
            workloads: vec![(
                fixtures::WORKLOAD_NAMES[0].to_owned(),
                updated_w1.workload.clone(),
            )]
            .into_iter()
            .collect(),
        };

        let added_workloads = vec![updated_w1.clone()];
        let deleted_workloads = vec![DeletedWorkload {
            instance_name: w1.instance_name.clone(),
            ..Default::default()
        }];

        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[0]
        )];
        let mut server = AnkaiosServer::new(server_receiver, to_agents.clone());

        server
            .workload_states_map
            .process_new_states(vec![WorkloadStateSpec {
                instance_name: w1.instance_name.clone(),
                execution_state: ExecutionStateSpec::initial(),
            }]);

        let updated_desired_state = update_state.desired_state.clone();

        let state_generation_result = StateGenerationResult {
            old_desired_state: old_state.desired_state,
            new_desired_state: updated_desired_state.clone(),
            ..Default::default()
        };

        let mut expected_state_difference_tree = StateDifferenceTree::new();
        let updated_instance_name = updated_w1.instance_name.clone();
        let removed_instance_name = w1.instance_name.clone();
        expected_state_difference_tree
            .added_tree
            .first_difference_tree = generate_difference_tree_from_paths(&[
            vec![
                "desiredState".to_owned(),
                "workloads".to_owned(),
                fixtures::WORKLOAD_NAMES[0].to_owned(),
            ],
            vec![
                "workloadStates".to_owned(),
                updated_instance_name.agent_name().to_owned(),
                updated_instance_name.workload_name().to_owned(),
                updated_instance_name.id().to_owned(),
            ],
        ]);

        expected_state_difference_tree
            .removed_tree
            .first_difference_tree = generate_difference_tree_from_paths(&[
            vec![
                "desiredState".to_owned(),
                "workloads".to_owned(),
                fixtures::WORKLOAD_NAMES[0].to_owned(),
            ],
            vec![
                "workloadStates".to_owned(),
                removed_instance_name.agent_name().to_owned(),
                removed_instance_name.workload_name().to_owned(),
                removed_instance_name.id().to_owned(),
            ],
        ]);

        expected_state_difference_tree
            .added_tree
            .full_difference_tree = expected_state_difference_tree
            .added_tree
            .first_difference_tree
            .clone();
        expected_state_difference_tree
            .removed_tree
            .full_difference_tree = expected_state_difference_tree
            .removed_tree
            .first_difference_tree
            .clone();

        let state_difference_tree = expected_state_difference_tree.clone();
        let state_comparator_context = MockStateComparator::new_context();
        let mut state_comparator = MockStateComparator::default();
        state_comparator
            .expect_state_differences()
            .once()
            .return_const(state_difference_tree);
        state_comparator_context
            .expect()
            .once()
            .return_once(|_, _| state_comparator);
        server
            .server_state
            .expect_generate_new_state()
            .once()
            .return_once(move |_, _| Ok(state_generation_result));
        server
            .server_state
            .expect_update()
            .once()
            .return_const(Ok(Some(AddedDeletedWorkloads {
                added_workloads,
                deleted_workloads,
            })));

        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .return_const(HashSet::new());

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(true);

        let mut expected_workload_states_map = WorkloadStatesMapSpec::default();
        expected_workload_states_map.process_new_states(vec![WorkloadStateSpec {
            instance_name: updated_w1.instance_name.clone(),
            execution_state: ExecutionStateSpec::initial(),
        }]);

        server
            .event_handler
            .expect_send_events()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(expected_workload_states_map),
                mockall::predicate::eq(AgentMapSpec::default()),
                mockall::predicate::eq(expected_state_difference_tree),
                mockall::predicate::function(move |event_sender_channel: &FromServerSender| {
                    event_sender_channel.same_channel(&to_agents)
                }),
            )
            .once()
            .return_const(());

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-sends-state-differences-as-events~1]
    #[tokio::test]
    async fn utest_server_sends_events_upon_update_state_with_updated_configs() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut update_state = CompleteStateSpec::default();
        update_state.desired_state.configs = generate_test_config_map();
        update_state
            .desired_state
            .configs
            .configs
            .retain(|key, _| key == "config_2");

        let update_mask = vec!["desiredState.configs".to_owned()];
        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        let updated_desired_state = update_state.desired_state.clone();

        let state_generation_result = StateGenerationResult {
            new_desired_state: updated_desired_state.clone(),
            ..Default::default()
        };

        let mut expected_state_difference_tree = StateDifferenceTree::new();
        let updated_path = vec![
            "desiredState".to_owned(),
            "configs".to_owned(),
            "config_2".to_owned(),
        ];
        expected_state_difference_tree
            .updated_tree
            .full_difference_tree =
            generate_difference_tree_from_paths(std::slice::from_ref(&updated_path));

        let state_difference_tree = expected_state_difference_tree.clone();
        let state_comparator_context = MockStateComparator::new_context();
        let mut state_comparator = MockStateComparator::default();
        state_comparator
            .expect_state_differences()
            .once()
            .return_const(state_difference_tree);
        state_comparator_context
            .expect()
            .once()
            .return_once(|_, _| state_comparator);
        server
            .server_state
            .expect_generate_new_state()
            .once()
            .return_once(move |_, _| Ok(state_generation_result));
        server
            .server_state
            .expect_update()
            .once()
            .return_const(Ok(None));

        server
            .log_campaign_store
            .expect_remove_collector_campaign_entry()
            .return_const(HashSet::new());

        server
            .event_handler
            .expect_has_subscribers()
            .once()
            .return_const(true);

        server
            .event_handler
            .expect_send_events()
            .with(
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::eq(expected_state_difference_tree),
                mockall::predicate::always(),
            )
            .once()
            .return_const(());

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-sends-state-differences-as-events~1]
    #[tokio::test]
    async fn utest_server_sends_events_for_workload_states() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        server.server_state.expect_cleanup_state().return_const(());

        server
            .event_handler
            .expect_has_subscribers()
            .return_const(true);

        let workload_state_1 = generate_test_workload_state(
            fixtures::WORKLOAD_NAMES[0],
            ExecutionStateSpec::succeeded(),
        );

        let workload_state_2 = generate_test_workload_state(
            fixtures::WORKLOAD_NAMES[1],
            ExecutionStateSpec::removed(),
        );

        let updated_instance_name = workload_state_1.instance_name.clone();
        let removed_instance_name = workload_state_2.instance_name.clone();

        let mut expected_state_difference_tree = StateDifferenceTree::new();
        expected_state_difference_tree
            .updated_tree
            .full_difference_tree = generate_difference_tree_from_paths(&[vec![
            "workloadStates".to_owned(),
            updated_instance_name.agent_name().to_owned(),
            updated_instance_name.workload_name().to_owned(),
            updated_instance_name.id().to_owned(),
        ]]);

        expected_state_difference_tree
            .removed_tree
            .first_difference_tree = generate_difference_tree_from_paths(&[vec![
            "workloadStates".to_owned(),
            removed_instance_name.agent_name().to_owned(),
            removed_instance_name.workload_name().to_owned(),
            removed_instance_name.id().to_owned(),
        ]]);

        expected_state_difference_tree
            .removed_tree
            .full_difference_tree = expected_state_difference_tree
            .removed_tree
            .first_difference_tree
            .clone();

        let mut state_comparator = MockStateComparator::default();
        let state_difference_tree = expected_state_difference_tree.clone();
        state_comparator
            .expect_state_differences()
            .once()
            .return_once(|| state_difference_tree);

        let mock_state_comparator = MockStateComparator::new_context();
        mock_state_comparator
            .expect()
            .once()
            .return_once(|_, _| state_comparator);

        server
            .event_handler
            .expect_remove_workload_subscriber()
            .once()
            .return_const(());

        server
            .event_handler
            .expect_send_events()
            .with(
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::eq(expected_state_difference_tree),
                mockall::predicate::always(),
            )
            .once()
            .return_const(());

        let update_workload_state_result = to_server
            .update_workload_state(vec![workload_state_1, workload_state_2])
            .await;
        assert!(update_workload_state_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-sends-state-differences-as-events~1]
    #[tokio::test]
    async fn utest_server_sends_events_for_agent_load_status() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .agent_map
            .agents
            .entry(fixtures::AGENT_NAMES[0].to_owned())
            .or_default();

        server
            .event_handler
            .expect_has_subscribers()
            .times(2)
            .return_const(true);

        let mut expected_state_difference_tree = StateDifferenceTree::new();
        expected_state_difference_tree
            .updated_tree
            .full_difference_tree = generate_difference_tree_from_paths(&[
            vec![
                "agents".to_owned(),
                fixtures::AGENT_NAMES[0].to_owned(),
                "status".to_owned(),
                "cpuUsage".to_owned(),
            ],
            vec![
                "agents".to_owned(),
                fixtures::AGENT_NAMES[0].to_owned(),
                "status".to_owned(),
                "freeMemory".to_owned(),
            ],
        ]);

        let mut state_comparator = MockStateComparator::default();
        let state_difference_tree = expected_state_difference_tree.clone();
        state_comparator
            .expect_state_differences()
            .once()
            .return_once(|| state_difference_tree);

        let mock_state_comparator = MockStateComparator::new_context();
        mock_state_comparator
            .expect()
            .once()
            .return_once(|_, _| state_comparator);

        server
            .event_handler
            .expect_send_events()
            .with(
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::always(),
                mockall::predicate::eq(expected_state_difference_tree),
                mockall::predicate::always(),
            )
            .once()
            .return_const(());

        let new_agent_load_status = AgentLoadStatus {
            agent_name: fixtures::AGENT_NAMES[0].to_string(),
            cpu_usage: fixtures::CPU_USAGE_SPEC,
            free_memory: fixtures::FREE_MEMORY_SPEC,
        };
        let update_agent_load_status_result =
            to_server.agent_load_status(new_agent_load_status).await;
        assert!(update_agent_load_status_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-removes-event-subscription-for-disconnected-cli~1]
    #[tokio::test]
    async fn utest_server_removes_event_subscriber_on_cli_disconnect() {
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        server
            .log_campaign_store
            .expect_remove_cli_log_campaign_entry()
            .with(mockall::predicate::eq(CLI_CONNECTION_NAME.to_owned()))
            .once()
            .return_const(HashSet::default());

        server
            .event_handler
            .expect_remove_cli_subscriber()
            .with(mockall::predicate::eq(CLI_CONNECTION_NAME.to_owned()))
            .once()
            .return_const(());

        assert!(
            to_server
                .goodbye(CLI_CONNECTION_NAME.to_owned())
                .await
                .is_ok()
        );
        assert!(to_server.stop().await.is_ok());

        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-removes-subscription-for-deleted-subscriber-workload~1]
    #[tokio::test]
    async fn utest_server_remove_subscription_for_deleted_subscriber_workload() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, _comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);

        server.server_state.expect_cleanup_state().return_const(());

        server
            .event_handler
            .expect_has_subscribers()
            .return_const(true);

        let expected_state_difference_tree = StateDifferenceTree::new();
        let mut state_comparator = MockStateComparator::default();
        let state_difference_tree = expected_state_difference_tree.clone();
        state_comparator
            .expect_state_differences()
            .once()
            .return_once(|| state_difference_tree);

        let mock_state_comparator = MockStateComparator::new_context();
        mock_state_comparator
            .expect()
            .once()
            .return_once(|_, _| state_comparator);

        let removed_workload_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::removed(),
        );

        server
            .event_handler
            .expect_remove_workload_subscriber()
            .with(
                mockall::predicate::eq(fixtures::AGENT_NAMES[0].to_owned()),
                mockall::predicate::eq(fixtures::WORKLOAD_NAMES[0].to_owned()),
            )
            .once()
            .return_const(());

        server
            .event_handler
            .expect_send_events()
            .once()
            .return_const(());

        let update_workload_state_result = to_server
            .update_workload_state(vec![removed_workload_state])
            .await;
        assert!(update_workload_state_result.is_ok());

        drop(to_server);
        let result = server.start(None).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~server-state-updates-agent-tags~1]
    #[tokio::test]
    async fn utest_set_complete_state_only_updates_tags_for_existing_agents() {
        const OTHER_CPU_USAGE: u32 = 75;
        const OTHER_FREE_MEMORY: u64 = 512;

        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let update_state = CompleteStateSpec {
            agents: AgentMapSpec {
                agents: [
                    (
                        AGENT_NAMES[0].to_string(),
                        AgentAttributesSpec {
                            status: Some(AgentStatusSpec {
                                // Status changed
                                cpu_usage: Some(CpuUsageSpec { cpu_usage: 99 }),
                                free_memory: Some(FreeMemorySpec { free_memory: 1 }),
                            }),
                            tags: TagsSpec {
                                tags: HashMap::from([
                                    ("location".to_string(), "on-car".to_string()), // Updated
                                    ("new_tag".to_string(), "value".to_string()),   // Added
                                                                                    // "type" removed
                                ]),
                            },
                        },
                    ),
                    ("new_agent".to_string(), AgentAttributesSpec::default()),
                ]
                .into(),
            },
            ..Default::default()
        };
        let update_mask = vec![
            format!("agents.{}", AGENT_NAMES[0].to_string()),
            "agents.new_agent".to_string(),
        ];

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();

        let agents_clone = update_state.agents.clone();
        let update_state_clone: CompleteState = update_state.clone().into();
        mock_server_state
            .expect_generate_new_state()
            .with(
                mockall::predicate::eq(update_state_clone),
                mockall::predicate::eq(update_mask.clone()),
            )
            .returning(move |_, _| {
                Ok(StateGenerationResult {
                    new_desired_state: Default::default(),
                    new_agent_map: agents_clone.clone(),
                    ..Default::default()
                })
            });
        mock_server_state
            .expect_update()
            .with(mockall::predicate::eq(update_state.desired_state.clone()))
            .once()
            .return_const(Ok(None));
        mock_server_state
            .expect_get_complete_state_by_field_mask()
            .returning(|_, _, agents| {
                Ok(CompleteState {
                    agents: Some(agents.to_owned().into()),
                    ..Default::default()
                })
            });
        server.server_state = mock_server_state;
        server.agent_map.agents.insert(
            fixtures::AGENT_NAMES[0].to_string(),
            AgentAttributesSpec {
                status: Some(AgentStatusSpec {
                    cpu_usage: Some(fixtures::CPU_USAGE_SPEC),
                    free_memory: Some(fixtures::FREE_MEMORY_SPEC),
                }),
                tags: TagsSpec {
                    tags: HashMap::from([
                        ("type".to_string(), "AI-agent".to_string()),
                        ("location".to_string(), "online".to_string()),
                    ]),
                },
            },
        );
        server
            .event_handler
            .expect_has_subscribers()
            .return_const(false);

        server.agent_map.agents.insert(
            fixtures::AGENT_NAMES[1].to_string(),
            AgentAttributesSpec {
                status: Some(AgentStatusSpec {
                    cpu_usage: Some(CpuUsageSpec {
                        cpu_usage: OTHER_CPU_USAGE,
                    }),
                    free_memory: Some(FreeMemorySpec {
                        free_memory: OTHER_FREE_MEMORY,
                    }),
                }),
                tags: TagsSpec {
                    tags: HashMap::from([("type".to_string(), "watchdog".to_string())]),
                },
            },
        );

        let update_state = CompleteStateSpec {
            agents: AgentMapSpec {
                agents: [
                    (
                        AGENT_NAMES[0].to_string(),
                        AgentAttributesSpec {
                            status: Some(AgentStatusSpec {
                                // Status changed
                                cpu_usage: Some(CpuUsageSpec { cpu_usage: 99 }),
                                free_memory: Some(FreeMemorySpec { free_memory: 1 }),
                            }),
                            tags: TagsSpec {
                                tags: HashMap::from([
                                    ("location".to_string(), "on-car".to_string()), // Updated
                                    ("new_tag".to_string(), "value".to_string()),   // Added
                                                                                    // "type" removed
                                ]),
                            },
                        },
                    ),
                    ("new_agent".to_string(), AgentAttributesSpec::default()),
                ]
                .into(),
            },
            ..Default::default()
        };

        let update_mask = vec![
            format!("agents.{}", AGENT_NAMES[0].to_string()),
            "agents.new_agent".to_string(),
        ];

        let server_task = tokio::spawn(async move { server.start(None).await });

        // send new state to server
        let update_state_result = to_server
            .update_state(
                fixtures::REQUEST_ID.to_string(),
                update_state.into(),
                update_mask,
            )
            .await;
        assert!(update_state_result.is_ok());

        assert!(matches!(
            comm_middle_ware_receiver.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::UpdateStateSuccess(_)),
            }) if request_id == fixtures::REQUEST_ID
        ));

        let request_id_2 = REQUEST_ID.to_string() + "2";

        let request_complete_state_result = to_server
            .request_complete_state(
                request_id_2.clone(),
                CompleteStateRequest {
                    field_mask: ["agents".to_string()].into(),
                    subscribe_for_events: false,
                },
            )
            .await;
        assert!(request_complete_state_result.is_ok());
        let foo = comm_middle_ware_receiver.recv().await.unwrap();

        assert!(matches!(
            foo,
            FromServer::Response(Response {
                request_id,
                response_content: Some(ResponseContent::CompleteStateResponse(complete_state_response)),
            }) if request_id == request_id_2 && get_agent_names(&complete_state_response) == Some(HashSet::from([AGENT_NAMES[0].to_string(), AGENT_NAMES[1].to_string()]))
        ));

        fn get_agent_names(
            complete_state_response: &CompleteStateResponse,
        ) -> Option<HashSet<String>> {
            Some(
                complete_state_response
                    .complete_state
                    .as_ref()?
                    .agents
                    .as_ref()?
                    .agents
                    .keys()
                    .map(|k| k.to_string())
                    .collect::<HashSet<_>>(),
            )
        }

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }
}

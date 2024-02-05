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

mod cycle_check;
mod delete_graph;
mod server_state;

use common::commands::{CompleteState, Request, UpdateWorkload};
use common::from_server_interface::{FromServerReceiver, FromServerSender};
use common::std_extensions::IllegalStateResult;
use common::to_server_interface::{ToServerReceiver, ToServerSender};

#[cfg_attr(test, mockall_double::double)]
use server_state::ServerState;

use crate::workload_state_db::WorkloadStateDB;
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

pub struct AnkaiosServer {
    // [impl->swdd~server-uses-async-channels~1]
    receiver: ToServerReceiver,
    // [impl->swdd~communication-to-from-server-middleware~1]
    to_agents: FromServerSender,
    server_state: ServerState,
    workload_state_db: WorkloadStateDB,
}

impl AnkaiosServer {
    pub fn new(receiver: ToServerReceiver, to_agents: FromServerSender) -> Self {
        AnkaiosServer {
            receiver,
            to_agents,
            server_state: ServerState::default(),
            workload_state_db: WorkloadStateDB::default(),
        }
    }

    pub async fn start(&mut self, startup_state: Option<CompleteState>) -> Result<(), String> {
        if let Some(state) = startup_state {
            match self.server_state.update(state, vec![]) {
                Ok(Some((added_workloads, deleted_workloads))) => {
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
            log::info!("No startup state provided -> waiting for new workloads from the CLI");
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

                    // Send this agent all workloads in the current state which are assigned to him
                    // [impl->swdd~agent-from-agent-field~1]
                    let added_workloads = self
                        .server_state
                        .get_workloads_for_agent(&method_obj.agent_name);

                    log::debug!(
                        "Sending initial UpdateWorkload to agent '{}' with added workloads: '{:?}'",
                        method_obj.agent_name,
                        added_workloads,
                    );

                    // [impl->swdd~server-sends-all-workloads-on-start~1]
                    self.to_agents
                        .update_workload(
                            added_workloads,
                            // It's a newly connected agent, no need to delete anything.
                            vec![],
                        )
                        .await
                        .unwrap_or_illegal_state();

                    // [impl->swdd~server-informs-a-newly-connected-agent-workload-states~1]
                    // [impl->swdd~server-sends-all-workload-states-on-agent-connect~1]
                    let workload_states = self
                        .workload_state_db
                        .get_workload_state_excluding_agent(&method_obj.agent_name);

                    if !workload_states.is_empty() {
                        log::debug!(
                            "Sending initial UpdateWorkloadState to agent '{}' with workload states: '{:?}'",
                            method_obj.agent_name,
                            workload_states,
                        );

                        self.to_agents
                            .update_workload_state(workload_states)
                            .await
                            .unwrap_or_illegal_state();
                    } else {
                        log::debug!("No workload states to send.");
                    }
                }
                ToServer::AgentGone(method_obj) => {
                    log::debug!("Received AgentGone from '{}'", method_obj.agent_name);
                    // [impl->swdd~server-set-workload-state-unknown-on-disconnect~1]
                    self.workload_state_db
                        .mark_all_workload_state_for_agent_unknown(&method_obj.agent_name);

                    // communicate the workload execution states to other agents
                    // [impl->swdd~server-distribute-workload-state-unknown-on-disconnect~1]
                    self.to_agents
                        .update_workload_state(
                            self.workload_state_db
                                .get_workload_state_for_agent(&method_obj.agent_name),
                        )
                        .await
                        .unwrap_or_illegal_state();
                }
                // [impl->swdd~server-provides-update-desired-state-interface~1]
                ToServer::Request(Request {
                    request_id,
                    request_content,
                }) => match request_content {
                    // [impl->swdd~server-provides-interface-get-complete-state~1]
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
                            &complete_state_request,
                            &self.workload_state_db,
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
                                        common::commands::CompleteState {
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
                                let from_server_command =
                                    FromServer::UpdateWorkload(UpdateWorkload {
                                        added_workloads,
                                        deleted_workloads,
                                    });
                                self.to_agents
                                    .send(from_server_command)
                                    .await
                                    .unwrap_or_illegal_state();
                            }
                            Ok(None) => log::debug!(
                                "The current state and new state are identical -> nothing to do"
                            ),
                            Err(error_msg) => {
                                // [impl->swdd~server-continues-on-invalid-updated-state~1]
                                log::error!("Update rejected: '{error_msg}'",);
                            }
                        }
                    }
                },
                ToServer::UpdateWorkloadState(method_obj) => {
                    log::debug!(
                        "Received UpdateWorkloadState: '{:?}'",
                        method_obj.workload_states
                    );

                    // [impl->swdd~server-stores-workload-state~1]
                    self.workload_state_db
                        .insert(method_obj.workload_states.clone());

                    // [impl->swdd~server-forwards-workload-state~1]
                    self.to_agents
                        .update_workload_state(method_obj.workload_states)
                        .await
                        .unwrap_or_illegal_state();
                }
                ToServer::Stop(_method_obj) => {
                    log::debug!("Received Stop from communications server");
                    // TODO: handle the call
                    break;
                }
                unknown_message => {
                    log::warn!(
                        "Received an unknown message from communications server: '{:?}'",
                        unknown_message
                    );
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
    use std::collections::HashMap;

    use super::AnkaiosServer;
    use crate::ankaios_server::server_state::{MockServerState, UpdateStateError};
    use crate::ankaios_server::{create_from_server_channel, create_to_server_channel};
    use common::commands::{CompleteStateRequest, UpdateWorkload, UpdateWorkloadState};
    use common::objects::{DeletedWorkload, ExecutionState, State, WorkloadState};
    use common::test_utils::generate_test_workload_spec_with_param;
    use common::to_server_interface::ToServerInterface;
    use common::{commands::CompleteState, from_server_interface::FromServer};

    const AGENT_A: &str = "agent_A";
    const AGENT_B: &str = "agent_B";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const RUNTIME_NAME: &str = "runtime";
    const REQUEST_ID_A: &str = "agent_A@id1";

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-fails-on-invalid-startup-state~1]
    #[tokio::test]
    async fn utest_server_start_fail_on_invalid_startup_config() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (_to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        // contains a self cycle to workload A
        let workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            "workload A".to_string(),
            RUNTIME_NAME.to_string(),
        );

        let startup_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(workload.name.clone(), workload)]),
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
        assert!(result.is_err());

        assert!(comm_middle_ware_receiver.try_recv().is_err());
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
            "workload A".to_string(),
            RUNTIME_NAME.to_string(),
        );

        let new_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(
                    updated_workload.name.clone(),
                    updated_workload.clone(),
                )]),
                ..Default::default()
            },
            ..Default::default()
        };

        // fix new state by deleting the dependencies
        let mut fixed_state = new_state.clone();
        updated_workload.dependencies.clear();
        fixed_state.desired_state.workloads =
            HashMap::from([(updated_workload.name.clone(), updated_workload.clone())]);

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
                "workload A".to_string(),
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

        // make sure all messages are consumed
        assert!(comm_middle_ware_receiver.try_recv().is_err());

        server_task.abort();
    }

    // [utest->swdd~server-uses-async-channels~1]
    #[tokio::test]
    async fn utest_server_start_with_valid_startup_config() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (_to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let startup_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(workload.name.clone(), workload.clone())]),
                ..Default::default()
            },
            ..Default::default()
        };

        let added_workloads = vec![workload];
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

        let server_task = tokio::spawn(async move { server.start(Some(startup_state)).await });

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        let expected_from_server_command = FromServer::UpdateWorkload(UpdateWorkload {
            added_workloads,
            deleted_workloads,
        });
        assert_eq!(from_server_command, expected_from_server_command);

        server_task.abort();
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-sends-all-workloads-on-start~1]
    // [utest->swdd~agent-from-agent-field~1]
    // [utest->swdd~server-starts-without-startup-config~1]
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
        let mut seq = mockall::Sequence::new();
        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(AGENT_A.to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w1.clone()]);

        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(AGENT_B.to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w2.clone()]);
        server.server_state = mock_server_state;

        let server_task = tokio::spawn(async move { server.start(None).await });

        // first agent connects to the server
        let agent_hello_result = to_server.agent_hello(AGENT_A.to_string()).await;
        assert!(agent_hello_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![w1],
                deleted_workloads: vec![],
            }),
            from_server_command
        );

        // [utest->swdd~server-informs-a-newly-connected-agent-workload-states~1]
        // [utest->swdd~server-sends-all-workload-states-on-agent-connect~1]
        // [utest->swdd~server-starts-without-startup-config~1]
        // send update_workload_state for first agent which is then stored in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![WorkloadState {
                agent_name: AGENT_A.to_string(),
                workload_name: WORKLOAD_NAME_1.to_string(),
                execution_state: ExecutionState::ExecRunning,
            }])
            .await;
        assert!(update_workload_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    workload_name: WORKLOAD_NAME_1.to_string(),
                    agent_name: AGENT_A.to_string(),
                    execution_state: ExecutionState::ExecRunning
                },]
            }),
            from_server_command
        );

        let agent_hello_result = to_server.agent_hello(AGENT_B.to_owned()).await;
        assert!(agent_hello_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![w2],
                deleted_workloads: vec![]
            }),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    workload_name: WORKLOAD_NAME_1.to_string(),
                    agent_name: AGENT_A.to_string(),
                    execution_state: ExecutionState::ExecRunning
                }]
            }),
            from_server_command
        );

        // [utest->swdd~server-forwards-workload-state~1]
        // send update_workload_state for second agent which is then stored in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![common::objects::WorkloadState {
                agent_name: AGENT_B.to_string(),
                workload_name: WORKLOAD_NAME_2.to_string(),
                execution_state: ExecutionState::ExecSucceeded,
            }])
            .await;
        assert!(update_workload_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    workload_name: WORKLOAD_NAME_2.to_string(),
                    agent_name: AGENT_B.to_string(),
                    execution_state: ExecutionState::ExecSucceeded
                }]
            }),
            from_server_command
        );

        // send update_workload_state for first agent again which is then updated in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![WorkloadState {
                agent_name: AGENT_A.to_string(),
                workload_name: WORKLOAD_NAME_1.to_string(),
                execution_state: ExecutionState::ExecSucceeded,
            }])
            .await;
        assert!(update_workload_state_result.is_ok());

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();

        assert_eq!(
            FromServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![WorkloadState {
                    workload_name: WORKLOAD_NAME_1.to_string(),
                    agent_name: AGENT_A.to_string(),
                    execution_state: ExecutionState::ExecSucceeded
                }]
            }),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-update-desired-state-interface~1]
    // [utest->swdd~server-starts-without-startup-config~1]
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
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), w1.clone())]
                    .into_iter()
                    .collect(),
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
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

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads,
                deleted_workloads,
            }),
            from_server_command
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-update-desired-state-interface~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_sends_workloads_and_workload_states_when_requested_update_state_nothing_todo(
    ) {
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
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), w1.clone())]
                    .into_iter()
                    .collect(),
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
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

        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let update_state = CompleteState {
            desired_state: State {
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), w1.clone())]
                    .into_iter()
                    .collect(),
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
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
    // [utest->swdd~server-provides-interface-get-complete-state~1]
    // [utest->swdd~server-includes-id-in-control-interface-response~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_returns_complete_state_when_received_request_complete_state() {
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
            AGENT_A.to_owned(),
            WORKLOAD_NAME_2.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let w3 = generate_test_workload_spec_with_param(
            AGENT_B.to_owned(),
            WORKLOAD_NAME_3.to_owned(),
            RUNTIME_NAME.to_string(),
        );

        let workloads = HashMap::from([
            (w1.name.clone(), w1),
            (w2.name.clone(), w2),
            (w3.name.clone(), w3),
        ]);

        let mut configs = HashMap::new();
        configs.insert("key1".to_string(), "value1".to_string());
        configs.insert("key2".to_string(), "value2".to_string());
        configs.insert("key3".to_string(), "value3".to_string());

        let current_complete_state = CompleteState {
            desired_state: State {
                workloads,
                configs,
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };
        let request_id = format!("{AGENT_A}@my_request_id");
        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        mock_server_state
            .expect_get_complete_state_by_field_mask()
            .with(
                mockall::predicate::function(|request_compl_state| {
                    request_compl_state == &CompleteStateRequest { field_mask: vec![] }
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
            common::from_server_interface::FromServer::Response(common::commands::Response {
                request_id,
                response_content: common::commands::ResponseContent::CompleteState(Box::new(
                    current_complete_state
                ))
            })
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-provides-interface-get-complete-state~1]
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
                mockall::predicate::function(|request_compl_state| {
                    request_compl_state == &CompleteStateRequest { field_mask: vec![] }
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

        let expected_complete_state = CompleteState {
            ..Default::default()
        };

        assert_eq!(
            from_server_command,
            common::from_server_interface::FromServer::Response(common::commands::Response {
                request_id,
                response_content: common::commands::ResponseContent::CompleteState(Box::new(
                    expected_complete_state
                ))
            })
        );

        server_task.abort();
        assert!(comm_middle_ware_receiver.try_recv().is_err());
    }

    // [utest->swdd~server-uses-async-channels~1]
    // [utest->swdd~server-stores-workload-state~1]
    // [utest->swdd~server-set-workload-state-unknown-on-disconnect~1]
    // [utest->swdd~server-distribute-workload-state-unknown-on-disconnect~1]
    // [utest->swdd~server-starts-without-startup-config~1]
    #[tokio::test]
    async fn utest_server_start_distributes_workload_unknown_after_agent_gone() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
        let (to_agents, mut comm_middle_ware_receiver) =
            create_from_server_channel(common::CHANNEL_CAPACITY);

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mock_server_state = MockServerState::new();
        server.server_state = mock_server_state;

        // send update_workload_state for first agent which is then stored in the workload_state_db in ankaios server
        let update_workload_state_result = to_server
            .update_workload_state(vec![WorkloadState {
                agent_name: AGENT_A.to_string(),
                workload_name: WORKLOAD_NAME_1.to_string(),
                execution_state: ExecutionState::ExecRunning,
            }])
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
                workload_states: vec![WorkloadState {
                    workload_name: WORKLOAD_NAME_1.to_string(),
                    agent_name: AGENT_A.to_string(),
                    execution_state: ExecutionState::ExecRunning,
                }]
            }),
            from_server_command
        );

        let workload_states = server
            .workload_state_db
            .get_workload_state_for_agent(AGENT_A);

        let expected_workload_state = WorkloadState {
            workload_name: WORKLOAD_NAME_1.to_string(),
            agent_name: AGENT_A.to_string(),
            execution_state: ExecutionState::ExecUnknown,
        };
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
        updated_w1.restart = false;
        let update_state = CompleteState {
            desired_state: State {
                workloads: vec![(WORKLOAD_NAME_1.to_owned(), updated_w1.clone())]
                    .into_iter()
                    .collect(),
                configs: HashMap::default(),
                cron_jobs: HashMap::default(),
            },
            ..Default::default()
        };
        let update_mask = vec!["desiredState.workloads".to_string()];

        let added_workloads = vec![updated_w1.clone()];
        let deleted_workloads = vec![DeletedWorkload {
            agent: w1.agent.clone(),
            name: w1.name.clone(),
            dependencies: HashMap::new(),
        }];

        let mut server = AnkaiosServer::new(server_receiver, to_agents);
        let mut mock_server_state = MockServerState::new();
        let mut seq = mockall::Sequence::new();
        mock_server_state
            .expect_get_workloads_for_agent()
            .with(mockall::predicate::eq(AGENT_A.to_string()))
            .once()
            .in_sequence(&mut seq)
            .return_const(vec![w1.clone()]);

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
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![w1.clone()],
                deleted_workloads: vec![]
            }),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![w2],
                deleted_workloads: vec![]
            }),
            from_server_command
        );

        let from_server_command = comm_middle_ware_receiver.recv().await.unwrap();
        assert_eq!(
            FromServer::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![updated_w1],
                deleted_workloads: vec![DeletedWorkload {
                    agent: w1.agent.clone(),
                    name: w1.name.clone(),
                    dependencies: HashMap::new(),
                }]
            }),
            from_server_command
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
}

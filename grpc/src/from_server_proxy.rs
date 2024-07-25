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

use crate::agent_senders_map::AgentSendersMap;
use crate::ankaios_streaming::GRPCStreaming;
use crate::grpc_api::{self, from_server::FromServerEnum};
use crate::grpc_middleware_error::GrpcMiddlewareError;
use api::ank_base;
use api::ank_base::response::ResponseContent;

use async_trait::async_trait;
use common::from_server_interface::{
    FromServer, FromServerInterface, FromServerReceiver, FromServerSender,
};
use common::objects::{
    get_workloads_per_agent, DeletedWorkload, DeletedWorkloadCollection, WorkloadCollection,
    WorkloadSpec, WorkloadState,
};
use common::request_id_prepending::detach_prefix_from_request_id;

use tonic::Streaming;

pub struct GRPCFromServerStreaming {
    inner: Streaming<grpc_api::FromServer>,
}

impl GRPCFromServerStreaming {
    pub fn new(inner: Streaming<grpc_api::FromServer>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl GRPCStreaming<grpc_api::FromServer> for GRPCFromServerStreaming {
    async fn message(&mut self) -> Result<Option<grpc_api::FromServer>, tonic::Status> {
        self.inner.message().await
    }
}

// [impl->swdd~grpc-client-forwards-from-server-messages-to-agent~1]
pub async fn forward_from_proto_to_ankaios(
    grpc_streaming: &mut impl GRPCStreaming<grpc_api::FromServer>,
    agent_tx: &FromServerSender,
) -> Result<(), GrpcMiddlewareError> {
    while let Some(value) = grpc_streaming.message().await? {
        log::trace!("RESPONSE={:?}", value);

        let try_block = async {
            match value
                .from_server_enum
                .ok_or(GrpcMiddlewareError::ReceiveError(
                    "Missing AgentReply.".to_string(),
                ))? {
                FromServerEnum::UpdateWorkload(obj) => {
                    agent_tx
                        .update_workload(
                            obj.added_workloads
                                .into_iter()
                                .map(|added_workload| added_workload.try_into())
                                .collect::<Result<Vec<WorkloadSpec>, _>>()
                                .map_err(GrpcMiddlewareError::ConversionError)?,
                            obj.deleted_workloads
                                .into_iter()
                                .map(|deleted_workload| deleted_workload.try_into())
                                .collect::<Result<Vec<DeletedWorkload>, _>>()
                                .map_err(GrpcMiddlewareError::ConversionError)?,
                        )
                        .await?;
                }
                FromServerEnum::UpdateWorkloadState(obj) => {
                    agent_tx
                        .update_workload_state(
                            obj.workload_states.into_iter().map(|x| x.into()).collect(),
                        )
                        .await?;
                }
                FromServerEnum::Response(response) => {
                    // [impl->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
                    agent_tx.response(response).await?;
                }
            }
            Ok(()) as Result<(), GrpcMiddlewareError>
        }
        .await;

        if let Err::<(), GrpcMiddlewareError>(error) = try_block {
            log::debug!("Could not forward from server message: {}", error);
        }
    }

    Ok(())
}

// [impl->swdd~grpc-server-forwards-from-server-messages-to-grpc-client~1]
pub async fn forward_from_ankaios_to_proto(
    agent_senders: &AgentSendersMap,
    receiver: &mut FromServerReceiver,
) {
    while let Some(from_server_msg) = receiver.recv().await {
        match from_server_msg {
            FromServer::UpdateWorkload(method_obj) => {
                log::trace!("Received UpdateWorkload from server: {:?}.", method_obj);

                distribute_workloads_to_agents(
                    agent_senders,
                    method_obj.added_workloads,
                    method_obj.deleted_workloads,
                )
                .await;
            }
            FromServer::UpdateWorkloadState(method_obj) => {
                log::trace!("Received UpdateWorkloadState from server: {:?}", method_obj);

                distribute_workload_states_to_agents(agent_senders, method_obj.workload_states)
                    .await;
            }
            FromServer::Response(response) => {
                let (agent_name, request_id) =
                    detach_prefix_from_request_id(response.request_id.as_ref());
                if let Some(sender) = agent_senders.get(&agent_name) {
                    let response_content: Option<ResponseContent> = response.response_content;
                    log::trace!(
                        "Sending response to agent '{}': {:?}.",
                        agent_name,
                        response_content
                    );

                    let result = sender
                        .send(Ok(grpc_api::FromServer {
                            from_server_enum: Some(
                                grpc_api::from_server::FromServerEnum::Response(
                                    ank_base::Response {
                                        request_id,
                                        response_content,
                                    },
                                ),
                            ),
                        }))
                        .await;
                    if result.is_err() {
                        log::warn!("Could not send response to agent '{}'", agent_name,);
                    }
                } else {
                    log::warn!("Unknown agent with name: '{}'", agent_name);
                }
            }
            FromServer::Stop(_method_obj) => {
                log::debug!("Received Stop from server.");
                // TODO: handle the call
                break;
            }
        }
    }
}

// [impl->swdd~grpc-server-forwards-from-server-messages-to-grpc-client~1]
async fn distribute_workload_states_to_agents(
    agent_senders: &AgentSendersMap,
    workload_state_collection: Vec<WorkloadState>,
) {
    // Workload states are agent related. Sending a flattened set here is not very good for the performance ...

    for agent_name in agent_senders.get_all_agent_names() {
        // Filter the workload states as we don't want to send an agent its own updates
        let filtered_workload_states: Vec<ank_base::WorkloadState> = workload_state_collection
            .clone()
            .into_iter()
            .filter(|workload_state| workload_state.instance_name.agent_name() != agent_name)
            .map(|x| x.into())
            .collect();
        if filtered_workload_states.is_empty() {
            log::trace!(
                "Skipping sending workload states to agent '{agent_name}'. Nothing to send."
            );
            continue;
        }

        if let Some(sender) = agent_senders.get(&agent_name) {
            log::trace!(
                "Sending workload states to agent '{}': {:?}.",
                agent_name,
                filtered_workload_states
            );
            let result = sender
                .send(Ok(grpc_api::FromServer {
                    from_server_enum: Some(FromServerEnum::UpdateWorkloadState(
                        grpc_api::UpdateWorkloadState {
                            workload_states: filtered_workload_states,
                        },
                    )),
                }))
                .await;
            if result.is_err() {
                log::warn!("Could not send workload states to agent '{}'", agent_name,);
            }
        } else {
            log::info!("Skipping sending workload states to agent '{agent_name}'. Agent disappeared in the meantime.");
        }
    }
}

// [impl->swdd~grpc-server-forwards-from-server-messages-to-grpc-client~1]
async fn distribute_workloads_to_agents(
    agent_senders: &AgentSendersMap,
    added_workloads: WorkloadCollection,
    deleted_workloads: DeletedWorkloadCollection,
) {
    // [impl->swdd~grpc-server-sorts-commands-according-agents~1]
    for (agent_name, (added_workload_vector, deleted_workload_vector)) in
        get_workloads_per_agent(added_workloads, deleted_workloads)
    {
        if let Some(sender) = agent_senders.get(&agent_name) {
            log::trace!("Sending added and deleted workloads to agent '{}'.\n\tAdded workloads: {:?}.\n\tDeleted workloads: {:?}.",
                agent_name, added_workload_vector, deleted_workload_vector);
            let result = sender
                .send(Ok(grpc_api::FromServer {
                    from_server_enum: Some(FromServerEnum::UpdateWorkload(
                        grpc_api::UpdateWorkload {
                            added_workloads: added_workload_vector
                                .into_iter()
                                .map(|x| x.into())
                                .collect(),
                            deleted_workloads: deleted_workload_vector
                                .into_iter()
                                .map(|x| x.into())
                                .collect(),
                        },
                    )),
                }))
                .await;
            if result.is_err() {
                log::warn!(
                    "Could not send added and deleted workloads to agent '{}'",
                    agent_name,
                );
            }
        } else {
            log::info!(
                "Agent {} not found, workloads not sent. Waiting for agent to connect.",
                agent_name
            )
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
    extern crate serde;
    extern crate tonic;

    use super::ank_base;
    use super::{forward_from_ankaios_to_proto, forward_from_proto_to_ankaios};
    use crate::grpc_api::{self, from_server::FromServerEnum, FromServer, UpdateWorkload};
    use crate::{agent_senders_map::AgentSendersMap, from_server_proxy::GRPCStreaming};
    use api::ank_base::{response, WorkloadMap};
    use async_trait::async_trait;
    use common::from_server_interface::FromServerInterface;
    use common::objects::{
        generate_test_stored_workload_spec, generate_test_workload_spec_with_param,
    };
    use common::test_utils::*;
    use std::collections::{HashMap, LinkedList};
    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::{
        join,
        sync::mpsc::{self, Receiver, Sender},
    };

    type TestSetup = (
        Sender<common::from_server_interface::FromServer>,
        Receiver<common::from_server_interface::FromServer>,
        Sender<Result<FromServer, tonic::Status>>,
        Receiver<Result<FromServer, tonic::Status>>,
        AgentSendersMap,
    );

    const WORKLOAD_NAME: &str = "workload_1";

    fn create_test_setup(agent_name: &str) -> TestSetup {
        let (to_manager, manager_receiver) =
            mpsc::channel::<common::from_server_interface::FromServer>(common::CHANNEL_CAPACITY);
        let (agent_tx, agent_rx) = tokio::sync::mpsc::channel::<Result<FromServer, tonic::Status>>(
            common::CHANNEL_CAPACITY,
        );

        let agent_senders_map = AgentSendersMap::new();

        agent_senders_map.insert(agent_name, agent_tx.clone());

        (
            to_manager,
            manager_receiver,
            agent_tx,
            agent_rx,
            agent_senders_map,
        )
    }

    #[derive(Default, Clone)]
    struct MockGRPCFromServerStreaming {
        msgs: LinkedList<Option<grpc_api::FromServer>>,
    }
    impl MockGRPCFromServerStreaming {
        fn new(msgs: LinkedList<Option<grpc_api::FromServer>>) -> Self {
            MockGRPCFromServerStreaming { msgs }
        }
    }
    #[async_trait]
    impl GRPCStreaming<grpc_api::FromServer> for MockGRPCFromServerStreaming {
        async fn message(&mut self) -> Result<Option<grpc_api::FromServer>, tonic::Status> {
            if let Some(msg) = self.msgs.pop_front() {
                Ok(msg)
            } else {
                Err(tonic::Status::new(tonic::Code::Unknown, "test"))
            }
        }
    }

    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_ankaios_to_proto_update_workload() {
        let agent = "agent_X";
        let (to_manager, mut manager_receiver, _, mut agent_rx, agent_senders_map) =
            create_test_setup(agent);

        // As the channel capacity is big enough the await is satisfied right away
        let update_workload_result = to_manager
            .update_workload(
                vec![generate_test_workload_spec_with_param(
                    agent.into(),
                    "name".to_string(),
                    "my_runtime".into(),
                )],
                vec![generate_test_deleted_workload(
                    agent.to_string(),
                    "workload X".to_string(),
                )],
            )
            .await;
        assert!(update_workload_result.is_ok());

        let handle = forward_from_ankaios_to_proto(&agent_senders_map, &mut manager_receiver);

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle).0;

        //if this returns the test is successful
        let result = agent_rx.recv().await.unwrap().unwrap();

        assert!(matches!(
            result.from_server_enum,
            // We don't need to check teh exact object, this will be checked in the test for distribute_workloads_to_agents
            Some(FromServerEnum::UpdateWorkload(_))
        ))
    }

    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_ankaios_to_proto_update_workload_state() {
        let (to_manager, mut manager_receiver, _, mut agent_rx, agent_senders_map) =
            create_test_setup("agent_X");

        let update_workload_state_result = to_manager
            .update_workload_state(vec![
                common::objects::generate_test_workload_state_with_agent(
                    WORKLOAD_NAME,
                    "other_agent",
                    common::objects::ExecutionState::running(),
                ),
            ])
            .await;
        assert!(update_workload_state_result.is_ok());

        let handle = forward_from_ankaios_to_proto(&agent_senders_map, &mut manager_receiver);

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle).0;

        //if this returns the test is successful
        let result = agent_rx.recv().await.unwrap().unwrap();

        assert!(matches!(
            result.from_server_enum,
            // We don't need to check teh exact object, this will be checked in the test for distribute_workloads_to_agents
            Some(FromServerEnum::UpdateWorkloadState(_))
        ))
    }

    // [utest->swdd~grpc-client-forwards-from-server-messages-to-agent~1]
    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_proto_to_ankaios_handles_missing_agent_reply() {
        let (to_agent, mut agent_receiver) =
            mpsc::channel::<common::from_server_interface::FromServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCFromServerStreaming::new(LinkedList::from([
                Some(FromServer {
                    from_server_enum: None,
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = tokio::spawn(async move {
            forward_from_proto_to_ankaios(&mut mock_grpc_ex_request_streaming, &to_agent).await
        })
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = agent_receiver.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-client-forwards-from-server-messages-to-agent~1]
    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_proto_to_ankaios_handles_incorrect_added_workloads(
    ) {
        let (to_agent, mut agent_receiver) =
            mpsc::channel::<common::from_server_interface::FromServer>(common::CHANNEL_CAPACITY);

        let mut workload: grpc_api::AddedWorkload = generate_test_workload_spec_with_param(
            "agent_name".to_string(),
            "name".to_string(),
            "workload1".to_string(),
        )
        .into();

        *workload
            .dependencies
            .get_mut(&String::from("workload A"))
            .unwrap() = -1;

        // simulate the reception of an update workload grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCFromServerStreaming::new(LinkedList::from([
                Some(FromServer {
                    from_server_enum: Some(FromServerEnum::UpdateWorkload(UpdateWorkload {
                        added_workloads: vec![workload],
                        deleted_workloads: vec![],
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = tokio::spawn(async move {
            forward_from_proto_to_ankaios(&mut mock_grpc_ex_request_streaming, &to_agent).await
        })
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = agent_receiver.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-client-forwards-from-server-messages-to-agent~1]
    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_proto_to_ankaios_handles_incorrect_deleted_workloads(
    ) {
        let (to_agent, mut agent_receiver) =
            mpsc::channel::<common::from_server_interface::FromServer>(common::CHANNEL_CAPACITY);

        let workload: grpc_api::DeletedWorkload = grpc_api::DeletedWorkload {
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "name".to_string(),
                ..Default::default()
            }),
            dependencies: [("name".into(), -1)].into(),
        };

        // simulate the reception of an update workload grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCFromServerStreaming::new(LinkedList::from([
                Some(FromServer {
                    from_server_enum: Some(FromServerEnum::UpdateWorkload(UpdateWorkload {
                        added_workloads: vec![],
                        deleted_workloads: vec![workload],
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = tokio::spawn(async move {
            forward_from_proto_to_ankaios(&mut mock_grpc_ex_request_streaming, &to_agent).await
        })
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = agent_receiver.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-client-forwards-from-server-messages-to-agent~1]
    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_proto_to_ankaios_update_workload() {
        let (to_agent, mut agent_receiver) =
            mpsc::channel::<common::from_server_interface::FromServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCFromServerStreaming::new(LinkedList::from([
                Some(FromServer {
                    from_server_enum: Some(FromServerEnum::UpdateWorkload(
                        UpdateWorkload::default(),
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = tokio::spawn(async move {
            forward_from_proto_to_ankaios(&mut mock_grpc_ex_request_streaming, &to_agent).await
        })
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = agent_receiver.recv().await.unwrap();

        assert!(matches!(
            result,
            // We don't need to check teh exact object, this will be checked in the test for distribute_workloads_to_agents
            common::from_server_interface::FromServer::UpdateWorkload(_)
        ));
    }

    // [utest->swdd~grpc-client-forwards-from-server-messages-to-agent~1]
    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_proto_to_ankaios_update_workload_state() {
        let (to_agent, mut agent_receiver) =
            mpsc::channel::<common::from_server_interface::FromServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCFromServerStreaming::new(LinkedList::from([
                Some(FromServer {
                    from_server_enum: Some(FromServerEnum::UpdateWorkloadState(
                        grpc_api::UpdateWorkloadState::default(),
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = tokio::spawn(async move {
            forward_from_proto_to_ankaios(&mut mock_grpc_ex_request_streaming, &to_agent).await
        })
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = agent_receiver.recv().await.unwrap();

        assert!(matches!(
            result,
            // We don't need to check teh exact object, this will be checked in the test for distribute_workloads_to_agents
            common::from_server_interface::FromServer::UpdateWorkloadState(_)
        ));
    }

    #[tokio::test]
    async fn utest_distribute_workloads_to_agents_shall_distribute_workloads_to_existing_agents() {
        let agent_name = "agent_X";
        let (_, _, _, mut agent_rx, agent_senders) = create_test_setup(agent_name);

        join!(super::distribute_workloads_to_agents(
            &agent_senders,
            vec![generate_test_workload_spec_with_param(
                agent_name.to_string(),
                "name".to_string(),
                "workload1".to_string()
            ),],
            vec![]
        ))
        .0;

        let result = agent_rx.recv().await.unwrap().unwrap();

        // shall receive update workload from server message
        assert!(matches!(
            result.from_server_enum,
            Some(FromServerEnum::UpdateWorkload(_))
        ))
    }

    #[tokio::test]
    async fn utest_distribute_workloads_to_agents_shall_not_distribute_workloads_to_non_existing_agents(
    ) {
        let agent_name = "agent_X";
        let (_, _, _, mut agent_rx, agent_senders) = create_test_setup(agent_name);

        join!(super::distribute_workloads_to_agents(
            &agent_senders,
            vec![generate_test_workload_spec_with_param(
                "not_existing_agent".to_string(),
                "name".to_string(),
                "workload1".to_string()
            ),],
            vec![]
        ))
        .0;

        // shall not receive any from server message
        assert!(matches!(agent_rx.try_recv(), Err(TryRecvError::Empty)))
    }

    #[tokio::test]
    async fn utest_distribute_workload_states_to_agents_shall_distribute_workload_states_from_other_agents(
    ) {
        let agent_name = "agent_X";
        let (_, _, _, mut agent_rx, agent_senders) = create_test_setup(agent_name);

        join!(super::distribute_workload_states_to_agents(
            &agent_senders,
            vec![common::objects::generate_test_workload_state_with_agent(
                "workload1",
                "other_agent",
                common::objects::ExecutionState::running()
            )],
        ))
        .0;

        let result = agent_rx.recv().await.unwrap().unwrap();

        // shall receive update workload from server message
        assert!(matches!(
            result.from_server_enum,
            Some(FromServerEnum::UpdateWorkloadState(_))
        ))
    }

    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_ankaios_to_proto_complete_state() {
        let agent_name: &str = "agent_X";
        let (to_manager, mut manager_receiver, _, mut agent_rx, agent_senders_map) =
            create_test_setup(agent_name);

        let mut startup_workloads = HashMap::<String, ank_base::Workload>::new();
        startup_workloads.insert(
            String::from(WORKLOAD_NAME),
            generate_test_stored_workload_spec(agent_name.to_string(), "my_runtime".to_string())
                .into(),
        );

        let my_request_id = "my_request_id".to_owned();
        let prefixed_my_request_id = format!("{agent_name}@{my_request_id}");

        let test_complete_state = ank_base::CompleteState {
            desired_state: Some(ank_base::State {
                workloads: Some(WorkloadMap {
                    workloads: startup_workloads.clone(),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let complete_state_result = to_manager
            .complete_state(prefixed_my_request_id, test_complete_state.clone())
            .await;
        assert!(complete_state_result.is_ok());

        let handle = forward_from_ankaios_to_proto(&agent_senders_map, &mut manager_receiver);

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(to_manager);
        join!(handle).0;

        //if this returns the test is successful
        let result = agent_rx.recv().await.unwrap().unwrap();

        assert!(matches!(
            result.from_server_enum,
            Some(FromServerEnum::Response(ank_base::Response {
                request_id,
                response_content: Some(ank_base::response::ResponseContent::CompleteState(ank_base::CompleteState{
                    desired_state: Some(desired_state),
                    workload_states: _}))

            })) if request_id == my_request_id
            && desired_state == test_complete_state.desired_state.unwrap()
        ));
    }

    #[tokio::test]
    async fn utest_from_server_proxy_forward_from_proto_to_ankaios_response() {
        let agent_name = "fake_agent";
        let (to_agent, mut agent_receiver) =
            mpsc::channel::<common::from_server_interface::FromServer>(common::CHANNEL_CAPACITY);

        let mut startup_workloads = HashMap::<String, ank_base::Workload>::new();
        startup_workloads.insert(
            String::from(WORKLOAD_NAME),
            generate_test_stored_workload_spec(agent_name.to_string(), "my_runtime".to_string())
                .into(),
        );

        let my_request_id = "my_request_id".to_owned();

        let test_complete_state = ank_base::CompleteState {
            desired_state: Some(ank_base::State {
                workloads: Some(WorkloadMap {
                    workloads: startup_workloads.clone(),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let proto_complete_state = ank_base::CompleteState {
            desired_state: test_complete_state.desired_state.clone(),
            ..Default::default()
        };

        let proto_response = ank_base::Response {
            request_id: my_request_id.clone(),
            response_content: Some(response::ResponseContent::CompleteState(
                proto_complete_state,
            )),
        };

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCFromServerStreaming::new(LinkedList::from([
                Some(FromServer {
                    from_server_enum: Some(FromServerEnum::Response(proto_response)),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = tokio::spawn(async move {
            forward_from_proto_to_ankaios(&mut mock_grpc_ex_request_streaming, &to_agent).await
        })
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = agent_receiver.recv().await.unwrap();

        let expected_test_complete_state = test_complete_state.clone();

        assert!(matches!(
            result,
            common::from_server_interface::FromServer::Response(ank_base::Response {
                request_id,
                response_content: Some(response::ResponseContent::CompleteState(
                    complete_state
                ))
            }) if request_id == my_request_id &&
            complete_state.desired_state == expected_test_complete_state.desired_state &&
            complete_state.workload_states == expected_test_complete_state.workload_states
        ));
    }
}

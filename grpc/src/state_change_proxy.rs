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

use crate::ankaios_streaming::GRPCStreaming;
use crate::proxy_error::GrpcProxyError;
use api::proto;
use api::proto::state_change_request::StateChangeRequestEnum;
use api::proto::UpdateStateRequest;

use common::request_id_prepending::prepend_request_id;
use common::state_change_interface::{
    StateChangeCommand, StateChangeInterface, StateChangeReceiver,
};

use tokio::sync::mpsc::Sender;
use tonic::Streaming;

use async_trait::async_trait;

pub struct GRPCStateChangeRequestStreaming {
    inner: Streaming<proto::StateChangeRequest>,
}

impl GRPCStateChangeRequestStreaming {
    pub fn new(inner: Streaming<proto::StateChangeRequest>) -> Self {
        GRPCStateChangeRequestStreaming { inner }
    }
}

#[async_trait]
impl GRPCStreaming<proto::StateChangeRequest> for GRPCStateChangeRequestStreaming {
    async fn message(&mut self) -> Result<Option<proto::StateChangeRequest>, tonic::Status> {
        self.inner.message().await
    }
}

// [impl->swdd~grpc-agent-connection-forwards-commands-to-server~1]
pub async fn forward_from_proto_to_ankaios(
    agent_name: String,
    grpc_streaming: &mut impl GRPCStreaming<proto::StateChangeRequest>,
    sink: Sender<StateChangeCommand>,
) -> Result<(), GrpcProxyError> {
    while let Some(message) = grpc_streaming.message().await? {
        log::trace!("REQUEST={:?}", message);

        match message
            .state_change_request_enum
            .ok_or(GrpcProxyError::Receive(
                "Missing state_change_request".to_string(),
            ))? {
            StateChangeRequestEnum::UpdateState(UpdateStateRequest {
                new_state,
                update_mask,
            }) => {
                log::debug!("Received UpdateStateRequest from {}", agent_name);
                match new_state.unwrap_or_default().try_into() {
                    Ok(new_state) => {
                        sink.update_state(new_state, update_mask).await?;
                    }
                    Err(error) => {
                        return Err(GrpcProxyError::Conversion(format!(
                            "Could not convert UpdateStateRequest for forwarding: {}",
                            error
                        )));
                    }
                }
            }
            StateChangeRequestEnum::UpdateWorkloadState(update_workload_state) => {
                log::trace!("Received UpdateWorkloadState from {}", agent_name);

                sink.update_workload_state(
                    update_workload_state
                        .workload_states
                        .into_iter()
                        .map(|x| x.into())
                        .collect(),
                )
                .await?;
            }
            StateChangeRequestEnum::RequestCompleteState(request_complete_state) => {
                log::trace!("Received RequestCompleteState from {}", agent_name);

                // [impl->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
                sink.request_complete_state(
                    proto::RequestCompleteState {
                        request_id: prepend_request_id(
                            request_complete_state.request_id.as_ref(),
                            agent_name.as_ref(),
                        ),
                        field_mask: request_complete_state.field_mask,
                    }
                    .into(),
                )
                .await?;
            }
            StateChangeRequestEnum::Goodbye(_goodbye) => {
                log::trace!(
                    "Received Goodbye from {}. Stopping the control loop.",
                    agent_name
                );
                break;
            }
            unknown_message => {
                log::warn!("Wrong StateChangeRequest: {:?}", unknown_message);
            }
        }
    }
    Ok(())
}

// [impl->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
pub async fn forward_from_ankaios_to_proto(
    grpc_tx: Sender<proto::StateChangeRequest>,
    server_rx: &mut StateChangeReceiver,
) -> Result<(), GrpcProxyError> {
    while let Some(x) = server_rx.recv().await {
        match x {
            StateChangeCommand::UpdateState(method_obj) => {
                log::trace!("Received UpdateWorkload from agent");

                grpc_tx
                    .send(proto::StateChangeRequest {
                        state_change_request_enum: Some(
                            proto::state_change_request::StateChangeRequestEnum::UpdateState(
                                proto::UpdateStateRequest {
                                    new_state: Some(method_obj.state.into()),
                                    update_mask: method_obj.update_mask,
                                },
                            ),
                        ),
                    })
                    .await?;
            }
            StateChangeCommand::UpdateWorkloadState(method_obj) => {
                log::trace!("Received UpdateWorkloadState from agent");

                grpc_tx
                    .send(proto::StateChangeRequest {
                        state_change_request_enum: Some(
                            proto::state_change_request::StateChangeRequestEnum::UpdateWorkloadState(common::commands::UpdateWorkloadState {
                                workload_states: method_obj.workload_states,
                            }.into()),
                        ),
                    })
                    .await?;
            }
            StateChangeCommand::RequestCompleteState(method_obj) => {
                log::trace!("Received RequestCompleteState from agent");

                grpc_tx
                    .send(proto::StateChangeRequest {
                        state_change_request_enum: Some(
                            proto::state_change_request::StateChangeRequestEnum::RequestCompleteState(common::commands::RequestCompleteState {
                                request_id: method_obj.request_id,
                                field_mask: method_obj.field_mask
                            }.into()),
                        ),
                    })
                    .await?;
            }
            StateChangeCommand::Stop(_method_obj) => {
                log::debug!("Received Stop from agent");
                // TODO: handle the call
                break;
            }
            StateChangeCommand::AgentHello(_) => {
                panic!("AgentHello was not expected at this point.");
            }
            StateChangeCommand::AgentGone(_) => {
                panic!("AgentGone internal messages is not intended to be sent over the network");
            }
            StateChangeCommand::Goodbye(_) => {
                panic!("Goodbye was not expected at this point.");
            }
        }
    }

    grpc_tx
        .send(proto::StateChangeRequest {
            state_change_request_enum: Some(
                proto::state_change_request::StateChangeRequestEnum::Goodbye(
                    api::proto::Goodbye {},
                ),
            ),
        })
        .await?;
    grpc_tx.closed().await;

    Ok(())
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
    use api::proto;
    use std::collections::LinkedList;

    use super::{forward_from_ankaios_to_proto, forward_from_proto_to_ankaios, GRPCStreaming};
    use async_trait::async_trait;
    use common::{
        objects as ankaios,
        state_change_interface::{StateChangeCommand, StateChangeInterface},
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };
    use tokio::{join, sync::mpsc};

    use api::proto::{
        state_change_request::StateChangeRequestEnum, StateChangeRequest, UpdateStateRequest,
    };

    #[derive(Default, Clone)]
    struct MockGRPCStateChangeRequestStreaming {
        msgs: LinkedList<Option<proto::StateChangeRequest>>,
    }
    impl MockGRPCStateChangeRequestStreaming {
        fn new(msgs: LinkedList<Option<proto::StateChangeRequest>>) -> Self {
            MockGRPCStateChangeRequestStreaming { msgs }
        }
    }
    #[async_trait]
    impl GRPCStreaming<proto::StateChangeRequest> for MockGRPCStateChangeRequestStreaming {
        async fn message(&mut self) -> Result<Option<proto::StateChangeRequest>, tonic::Status> {
            if let Some(msg) = self.msgs.pop_front() {
                Ok(msg)
            } else {
                Err(tonic::Status::new(tonic::Code::Unknown, "test"))
            }
        }
    }

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_state_change_command_forward_from_ankaios_to_proto_update_workload() {
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<StateChangeRequest>(common::CHANNEL_CAPACITY);

        let input_state = generate_test_complete_state(
            "request_id".to_owned(),
            vec![generate_test_workload_spec_with_param(
                "agent_X".into(),
                "name".to_string(),
                "my_runtime".into(),
            )],
        );
        let update_mask = vec!["bla".into()];

        // As the channel capacity is big enough the await is satisfied right away
        let update_state_result = server_tx
            .update_state(input_state.clone(), update_mask.clone())
            .await;
        assert!(update_state_result.is_ok());

        tokio::spawn(async move {
            let _ = forward_from_ankaios_to_proto(grpc_tx, &mut server_rx).await;
        });

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(server_tx);

        let result = grpc_rx.recv().await.unwrap();

        let proto_state = input_state.into();

        assert!(matches!(
            result.state_change_request_enum,
            Some(StateChangeRequestEnum::UpdateState(UpdateStateRequest{new_state, update_mask}))
            if new_state == Some(proto_state) && update_mask == update_mask));
    }

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_state_change_command_forward_from_ankaios_to_proto_update_workload_state() {
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<StateChangeRequest>(common::CHANNEL_CAPACITY);

        let wl_state = common::objects::WorkloadState {
            agent_name: "other_agent".into(),
            workload_name: "workload_1".into(),
            execution_state: common::objects::ExecutionState::ExecRunning,
        };

        let update_workload_state_result = server_tx
            .update_workload_state(vec![wl_state.clone()])
            .await;
        assert!(update_workload_state_result.is_ok());

        tokio::spawn(async move {
            let _ = forward_from_ankaios_to_proto(grpc_tx, &mut server_rx).await;
        });

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(server_tx);

        let result = grpc_rx.recv().await.unwrap();

        let proto_workload_state = wl_state.into();

        assert!(matches!(
            result.state_change_request_enum,
            Some(StateChangeRequestEnum::UpdateWorkloadState(proto::UpdateWorkloadState{workload_states}))
            if workload_states == vec!(proto_workload_state)));
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_state_change_command_forward_from_proto_to_ankaios_ignores_none() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc execution request
        let mut mock_grpc_ex_request_streaming =
            MockGRPCStateChangeRequestStreaming::new(LinkedList::from([]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.into(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_err());
        assert_eq!(forward_result.unwrap_err().to_string(), String::from("StreamingError: 'status: Unknown, message: \"test\", details: [], metadata: MetadataMap { headers: {} }'"));

        // pick received execution command
        let result = server_rx.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_state_change_command_forward_from_proto_to_ankaios_handles_missing_state_change_request(
    ) {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc execution request
        let mut mock_grpc_ex_request_streaming =
            MockGRPCStateChangeRequestStreaming::new(LinkedList::from([
                Some(StateChangeRequest {
                    state_change_request_enum: None,
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.into(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_err());
        assert_eq!(
            forward_result.unwrap_err().to_string(),
            String::from("ReceiveError: 'Missing state_change_request'")
        );

        // pick received execution command
        let result = server_rx.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_state_change_command_forward_from_proto_to_ankaios_fail_on_invalid_state() {
        let agent_name = "fake_agent";
        let (server_tx, mut _server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);

        let mut ankaios_state: proto::CompleteState = generate_test_complete_state(
            "request_id".to_owned(),
            vec![generate_test_workload_spec_with_param(
                agent_name.into(),
                "name".to_string(),
                "my_runtime".into(),
            )],
        )
        .into();
        ankaios_state
            .current_state
            .as_mut()
            .unwrap()
            .workloads
            .get_mut("name")
            .unwrap()
            .update_strategy = -1;

        let ankaios_update_mask = vec!["bla".into()];

        // simulate the reception of an update workload state grpc execution request
        let mut mock_grpc_ex_request_streaming =
            MockGRPCStateChangeRequestStreaming::new(LinkedList::from([
                Some(StateChangeRequest {
                    state_change_request_enum: Some(StateChangeRequestEnum::UpdateState(
                        proto::UpdateStateRequest {
                            new_state: Some(ankaios_state),
                            update_mask: ankaios_update_mask.clone(),
                        },
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.into(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_err());
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_state_change_command_forward_from_proto_to_ankaios_update_workload() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);

        let ankaios_state = generate_test_complete_state(
            "request_id".to_owned(),
            vec![generate_test_workload_spec_with_param(
                agent_name.into(),
                "name".to_string(),
                "my_runtime".into(),
            )],
        );

        let ankaios_update_mask = vec!["bla".into()];

        // simulate the reception of an update workload state grpc execution request
        let mut mock_grpc_ex_request_streaming =
            MockGRPCStateChangeRequestStreaming::new(LinkedList::from([
                Some(StateChangeRequest {
                    state_change_request_enum: Some(StateChangeRequestEnum::UpdateState(
                        proto::UpdateStateRequest {
                            new_state: Some(ankaios_state.clone().into()),
                            update_mask: ankaios_update_mask.clone(),
                        },
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.into(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;

        assert!(forward_result.is_ok());

        // pick received execution command
        let result = server_rx.recv().await.unwrap();

        assert!(matches!(
            result,
            StateChangeCommand::UpdateState(common::commands::UpdateStateRequest{state, update_mask})
            if state == ankaios_state && update_mask == ankaios_update_mask));
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_state_change_command_forward_from_proto_to_ankaios_update_workload_state() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);

        let proto_wl_state = proto::WorkloadState {
            workload_name: "fake_workload".into(),
            agent_name: agent_name.into(),
            execution_state: ankaios::ExecutionState::ExecRunning as i32,
        };

        // simulate the reception of an update workload state grpc execution request
        let mut mock_grpc_ex_request_streaming =
            MockGRPCStateChangeRequestStreaming::new(LinkedList::from([
                Some(StateChangeRequest {
                    state_change_request_enum: Some(StateChangeRequestEnum::UpdateWorkloadState(
                        proto::UpdateWorkloadState {
                            workload_states: vec![proto_wl_state.clone()],
                        },
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.into(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;

        assert!(forward_result.is_ok());

        // pick received execution command
        let result = server_rx.recv().await.unwrap();

        assert!(matches!(
            result,
            // TODO do a proper check here ...
            StateChangeCommand::UpdateWorkloadState(common::commands::UpdateWorkloadState{workload_states})
            if workload_states == vec!(proto_wl_state.into())
        ));
    }

    #[tokio::test]
    async fn utest_state_change_command_forward_from_proto_to_ankaios_request_complete_state() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc execution request
        let mut mock_grpc_ex_request_streaming =
            MockGRPCStateChangeRequestStreaming::new(LinkedList::from([
                Some(StateChangeRequest {
                    state_change_request_enum: Some(StateChangeRequestEnum::RequestCompleteState(
                        proto::RequestCompleteState {
                            request_id: String::from("my_request_id"),
                            field_mask: vec![],
                        },
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.into(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received execution command
        let result = server_rx.recv().await.unwrap();
        // [utest->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
        let expected_prefixed_my_request_id = String::from("fake_agent@my_request_id");
        let exepected_empty_field_mask: Vec<String> = vec![];
        assert!(
            matches!(result, common::state_change_interface::StateChangeCommand::RequestCompleteState(common::commands::RequestCompleteState{
            request_id,
            field_mask
        }) if request_id == expected_prefixed_my_request_id && field_mask == exepected_empty_field_mask)
        );
    }

    #[tokio::test]
    async fn utest_state_change_command_forward_from_ankaios_to_proto_request_complete_state() {
        let (server_tx, mut server_rx) =
            mpsc::channel::<StateChangeCommand>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<StateChangeRequest>(common::CHANNEL_CAPACITY);

        let request_complete_state = common::commands::RequestCompleteState {
            request_id: "my_request_id".to_owned(),
            field_mask: vec![],
        };

        let request_complete_state_result = server_tx
            .request_complete_state(request_complete_state.clone())
            .await;
        assert!(request_complete_state_result.is_ok());

        tokio::spawn(async move {
            let _ = forward_from_ankaios_to_proto(grpc_tx, &mut server_rx).await;
        });

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(server_tx);

        let result = grpc_rx.recv().await.unwrap();

        let proto_request_complete_state: proto::RequestCompleteState =
            request_complete_state.into();

        assert!(matches!(
        result.state_change_request_enum,
        Some(StateChangeRequestEnum::RequestCompleteState(proto::RequestCompleteState{request_id, field_mask}))
        if request_id == proto_request_complete_state.request_id && field_mask == proto_request_complete_state.field_mask));
    }
}

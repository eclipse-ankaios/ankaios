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
use crate::grpc_middleware_error::GrpcMiddlewareError;

use crate::grpc_api::{self, to_server::ToServerEnum};
use api::ank_base::{
    self, request::RequestContent, CompleteStateRequest, Request, UpdateStateRequest,
};

use common::request_id_prepending::prepend_request_id;
use common::to_server_interface::{ToServer, ToServerInterface, ToServerReceiver, ToServerSender};

use tokio::sync::mpsc::Sender;
use tonic::Streaming;

use async_trait::async_trait;

pub struct GRPCToServerStreaming {
    inner: Streaming<grpc_api::ToServer>,
}

impl GRPCToServerStreaming {
    pub fn new(inner: Streaming<grpc_api::ToServer>) -> Self {
        GRPCToServerStreaming { inner }
    }
}

#[async_trait]
impl GRPCStreaming<grpc_api::ToServer> for GRPCToServerStreaming {
    async fn message(&mut self) -> Result<Option<grpc_api::ToServer>, tonic::Status> {
        self.inner.message().await
    }
}

// [impl->swdd~grpc-agent-connection-forwards-commands-to-server~1]
pub async fn forward_from_proto_to_ankaios(
    agent_name: String,
    grpc_streaming: &mut impl GRPCStreaming<grpc_api::ToServer>,
    sink: ToServerSender,
) -> Result<(), GrpcMiddlewareError> {
    while let Some(message) = grpc_streaming.message().await? {
        log::trace!("REQUEST={:?}", message);

        match message
            .to_server_enum
            .ok_or(GrpcMiddlewareError::ReceiveError(
                "Missing to_server".to_string(),
            ))? {
            ToServerEnum::Request(Request {
                request_id,
                request_content,
            }) => {
                log::debug!("Received Request from '{}'", agent_name);

                // [impl->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
                let request_id = prepend_request_id(request_id.as_ref(), agent_name.as_ref());
                match request_content.ok_or(GrpcMiddlewareError::ConversionError(format!(
                    "Request content empty for request ID: '{}'",
                    request_id
                )))? {
                    RequestContent::UpdateStateRequest(UpdateStateRequest {
                        new_state,
                        update_mask,
                    }) => {
                        log::debug!("Received UpdateStateRequest from '{}'", agent_name);
                        match new_state.unwrap_or_default().try_into() {
                            Ok(new_state) => {
                                sink.update_state(request_id, new_state, update_mask)
                                    .await?;
                            }
                            Err(error) => {
                                return Err(GrpcMiddlewareError::ConversionError(format!(
                                    "Could not convert UpdateStateRequest for forwarding: '{}'",
                                    error
                                )));
                            }
                        };
                    }
                    RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask }) => {
                        log::trace!("Received RequestCompleteState from '{}'", agent_name);
                        sink.request_complete_state(
                            request_id,
                            ank_base::CompleteStateRequest { field_mask }.into(),
                        )
                        .await?;
                    }
                }
            }

            ToServerEnum::UpdateWorkloadState(update_workload_state) => {
                log::trace!("Received UpdateWorkloadState from '{}'", agent_name);

                sink.update_workload_state(
                    update_workload_state
                        .workload_states
                        .into_iter()
                        .map(|x| x.into())
                        .collect(),
                )
                .await?;
            }

            ToServerEnum::Goodbye(_goodbye) => {
                log::trace!(
                    "Received Goodbye from '{}'. Stopping the control loop.",
                    agent_name
                );
                break;
            }
            unknown_message => {
                log::warn!("Wrong ToServer message: '{:?}'", unknown_message);
            }
        }
    }
    Ok(())
}

// [impl->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
pub async fn forward_from_ankaios_to_proto(
    grpc_tx: Sender<grpc_api::ToServer>,
    server_rx: &mut ToServerReceiver,
) -> Result<(), GrpcMiddlewareError> {
    while let Some(x) = server_rx.recv().await {
        match x {
            ToServer::Request(request) => {
                log::trace!("Received Request from agent");
                grpc_tx
                    .send(grpc_api::ToServer {
                        to_server_enum: Some(ToServerEnum::Request(request.into())),
                    })
                    .await?;
            }
            ToServer::UpdateWorkloadState(method_obj) => {
                log::trace!("Received UpdateWorkloadState from agent");

                grpc_tx
                    .send(grpc_api::ToServer {
                        to_server_enum: Some(
                            grpc_api::to_server::ToServerEnum::UpdateWorkloadState(
                                common::commands::UpdateWorkloadState {
                                    workload_states: method_obj.workload_states,
                                }
                                .into(),
                            ),
                        ),
                    })
                    .await?;
            }
            ToServer::Stop(_method_obj) => {
                log::debug!("Received Stop from agent");
                // TODO: handle the call
                break;
            }
            ToServer::AgentHello(_) => {
                panic!("AgentHello was not expected at this point.");
            }
            ToServer::AgentGone(_) => {
                panic!("AgentGone internal messages is not intended to be sent over the network");
            }
            ToServer::Goodbye(_) => {
                panic!("Goodbye was not expected at this point.");
            }
        }
    }

    grpc_tx
        .send(grpc_api::ToServer {
            to_server_enum: Some(grpc_api::to_server::ToServerEnum::Goodbye(
                crate::grpc_api::Goodbye {},
            )),
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

    use std::collections::LinkedList;

    use super::{forward_from_ankaios_to_proto, forward_from_proto_to_ankaios, GRPCStreaming};
    use async_trait::async_trait;
    use common::test_utils::generate_test_complete_state;
    use common::{
        objects::generate_test_workload_spec_with_param,
        to_server_interface::{ToServer, ToServerInterface},
    };
    use tokio::sync::mpsc;

    use crate::grpc_api::{self, to_server::ToServerEnum};
    use api::ank_base::{self, UpdateStateRequest};

    #[derive(Default, Clone)]
    struct MockGRPCToServerStreaming {
        msgs: LinkedList<Option<grpc_api::ToServer>>,
    }
    impl MockGRPCToServerStreaming {
        fn new(msgs: LinkedList<Option<grpc_api::ToServer>>) -> Self {
            MockGRPCToServerStreaming { msgs }
        }
    }
    #[async_trait]
    impl GRPCStreaming<grpc_api::ToServer> for MockGRPCToServerStreaming {
        async fn message(&mut self) -> Result<Option<grpc_api::ToServer>, tonic::Status> {
            if let Some(msg) = self.msgs.pop_front() {
                Ok(msg)
            } else {
                Err(tonic::Status::new(tonic::Code::Unknown, "test"))
            }
        }
    }

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_update_workload() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let input_state =
            generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                "agent_X".into(),
                "name".to_string(),
                "my_runtime".into(),
            )]);
        let update_mask = vec!["bla".into()];

        // As the channel capacity is big enough the await is satisfied right away
        let update_state_result = server_tx
            .update_state(
                "request_id".to_owned(),
                input_state.clone(),
                update_mask.clone(),
            )
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
            result.to_server_enum,
            Some(ToServerEnum::Request(ank_base::Request{request_id, request_content: Some(ank_base::request::RequestContent::UpdateStateRequest(UpdateStateRequest{new_state, update_mask}))}))
            if request_id == "request_id" && new_state == Some(proto_state) && update_mask == update_mask));
    }

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_update_workload_state() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let wl_state = common::objects::generate_test_workload_state_with_agent(
            "workload_1",
            "other_agent",
            common::objects::ExecutionState::running(),
        );

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
            result.to_server_enum,
            Some(ToServerEnum::UpdateWorkloadState(grpc_api::UpdateWorkloadState{workload_states}))
            if workload_states == vec!(proto_workload_state)));
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_ignores_none() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.into(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_err());
        assert_eq!(forward_result.unwrap_err().to_string(), String::from("Connection interrupted: 'status: Unknown, message: \"test\", details: [], metadata: MetadataMap { headers: {} }'"));

        // pick received from server message
        let result = server_rx.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_handles_missing_to_server() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: None,
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
            String::from("ReceiveError: 'Missing to_server'")
        );

        // pick received from server message
        let result = server_rx.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_fail_on_invalid_state() {
        let agent_name = "fake_agent";
        let (server_tx, mut _server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut ankaios_state: ank_base::CompleteState =
            generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                agent_name.into(),
                "name".to_string(),
                "my_runtime".into(),
            )])
            .into();
        *ankaios_state
            .desired_state
            .as_mut()
            .unwrap()
            .workloads
            .as_mut()
            .unwrap()
            .workloads
            .get_mut("name")
            .unwrap()
            .dependencies
            .as_mut()
            .unwrap()
            .dependencies
            .get_mut(&String::from("workload A"))
            .unwrap() = -1;

        let ankaios_update_mask = vec!["bla".into()];

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: "request_id".to_owned(),
                        request_content: Some(
                            ank_base::request::RequestContent::UpdateStateRequest(
                                ank_base::UpdateStateRequest {
                                    new_state: Some(ankaios_state),
                                    update_mask: ankaios_update_mask.clone(),
                                },
                            ),
                        ),
                    })),
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
    async fn utest_to_server_command_forward_from_proto_to_ankaios_update_workload() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let ankaios_state =
            generate_test_complete_state(vec![generate_test_workload_spec_with_param(
                agent_name.into(),
                "name".to_string(),
                "my_runtime".into(),
            )]);

        let ankaios_update_mask = vec!["bla".into()];

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: "my_request_id".to_owned(),
                        request_content: Some(
                            ank_base::request::RequestContent::UpdateStateRequest(
                                ank_base::UpdateStateRequest {
                                    new_state: Some(ankaios_state.clone().into()),
                                    update_mask: ankaios_update_mask.clone(),
                                },
                            ),
                        ),
                    })),
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

        // pick received from server message
        let result = server_rx.recv().await.unwrap();
        let expected_prefixed_my_request_id = String::from("fake_agent@my_request_id");

        assert!(matches!(
            result,
            ToServer::Request(common::commands::Request {
                request_id,
                request_content: common::commands::RequestContent::UpdateStateRequest(update_request),
            })
            if request_id == expected_prefixed_my_request_id && update_request.state == ankaios_state && update_request.update_mask == ankaios_update_mask));
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_update_workload_state() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let proto_wl_state: ank_base::WorkloadState =
            common::objects::generate_test_workload_state_with_agent(
                "fake_workload",
                agent_name,
                common::objects::ExecutionState::running(),
            )
            .into();

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::UpdateWorkloadState(
                        grpc_api::UpdateWorkloadState {
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

        // pick received from server message
        let result = server_rx.recv().await.unwrap();

        assert!(matches!(
            result,
            // TODO do a proper check here ...
            ToServer::UpdateWorkloadState(common::commands::UpdateWorkloadState{workload_states})
            if workload_states == vec!(proto_wl_state.into())
        ));
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_request_complete_state() {
        let agent_name = "fake_agent";
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: "my_request_id".to_owned(),
                        request_content: Some(
                            ank_base::request::RequestContent::CompleteStateRequest(
                                ank_base::CompleteStateRequest { field_mask: vec![] },
                            ),
                        ),
                    })),
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

        // pick received from server message
        let result = server_rx.recv().await.unwrap();
        // [utest->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
        let expected_prefixed_my_request_id = String::from("fake_agent@my_request_id");
        let exepected_empty_field_mask: Vec<String> = vec![];
        assert!(
            matches!(result, common::to_server_interface::ToServer::Request(common::commands::Request {
                request_id,
                request_content:
                    common::commands::RequestContent::CompleteStateRequest(
                        common::commands::CompleteStateRequest { field_mask },
                    ),
            }) if request_id == expected_prefixed_my_request_id && field_mask == exepected_empty_field_mask)
        );
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_request_complete_state() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let request_complete_state = common::commands::CompleteStateRequest { field_mask: vec![] };

        let request_complete_state_result = server_tx
            .request_complete_state("my_request_id".to_owned(), request_complete_state.clone())
            .await;
        assert!(request_complete_state_result.is_ok());

        tokio::spawn(async move {
            let _ = forward_from_ankaios_to_proto(grpc_tx, &mut server_rx).await;
        });

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(server_tx);

        let result = grpc_rx.recv().await.unwrap();

        assert!(matches!(
        result.to_server_enum,
        Some(ToServerEnum::Request(ank_base::Request {
            request_id,
            request_content:
                Some(ank_base::request::RequestContent::CompleteStateRequest(
                    ank_base::CompleteStateRequest { field_mask },
                )),
        }))
        if request_id == "my_request_id" && field_mask == vec![] as Vec<String>));
    }
}

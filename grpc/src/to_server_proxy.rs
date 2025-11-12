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
    self, CompleteStateRequest, LogsStopResponse, Request, UpdateStateRequest,
    request::RequestContent,
};

use common::commands::AgentLoadStatus;
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
        log::trace!("REQUEST={message:?}");

        match message
            .to_server_enum
            .ok_or(GrpcMiddlewareError::ReceiveError(
                "Missing to_server".to_string(),
            ))? {
            ToServerEnum::Request(Request {
                request_id,
                request_content,
            }) => {
                log::debug!("Received Request from '{agent_name}'");

                // [impl->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
                let request_id = prepend_request_id(request_id.as_ref(), agent_name.as_ref());
                match request_content.ok_or(GrpcMiddlewareError::ConversionError(format!(
                    "Request content empty for request ID: '{request_id}'"
                )))? {
                    RequestContent::UpdateStateRequest(update_state_request) => {
                        let UpdateStateRequest {
                            new_state,
                            update_mask,
                        } = *update_state_request;
                        log::debug!("Received UpdateStateRequest from '{agent_name}'");
                        match new_state.unwrap_or_default().try_into() {
                            Ok(new_state) => {
                                sink.update_state(request_id, new_state, update_mask)
                                    .await?;
                            }
                            Err(error) => {
                                return Err(GrpcMiddlewareError::ConversionError(format!(
                                    "Could not convert UpdateStateRequest for forwarding: '{error}'"
                                )));
                            }
                        };
                    }
                    RequestContent::CompleteStateRequest(CompleteStateRequest { field_mask }) => {
                        log::trace!("Received RequestCompleteState from '{agent_name}'");
                        sink.request_complete_state(
                            request_id,
                            ank_base::CompleteStateRequest { field_mask }.into(),
                        )
                        .await?;
                    }
                    RequestContent::LogsRequest(logs_request) => {
                        log::trace!("Received LogsRequest from '{agent_name}'");
                        sink.logs_request(request_id, logs_request).await?;
                    }
                    RequestContent::LogsCancelRequest(_logs_stop_request) => {
                        log::trace!("Received LogsCancelRequest from '{agent_name}'");
                        sink.logs_cancel_request(request_id).await?;
                    }
                }
            }

            ToServerEnum::UpdateWorkloadState(update_workload_state) => {
                log::trace!("Received UpdateWorkloadState from '{agent_name}'");

                sink.update_workload_state(
                    update_workload_state
                        .workload_states
                        .into_iter()
                        .filter_map(|x| x.try_into().ok())
                        .collect(),
                )
                .await?;
            }

            ToServerEnum::Goodbye(_goodbye) => {
                log::trace!("Received Goodbye from '{agent_name}'. Stopping the control loop.");
                sink.goodbye(agent_name).await?;
                break;
            }

            ToServerEnum::AgentLoadStatus(agent_load_status) => {
                log::trace!(
                    "Received AgentLoadStatus from {}",
                    agent_load_status.agent_name
                );
                sink.agent_load_status(agent_load_status.into()).await?;
            }

            ToServerEnum::LogEntriesResponse(log_entries_response) => {
                log::trace!("Received LogEntriesResponse from '{agent_name}'");
                if let Some(logs_response_object) = log_entries_response.log_entries_response {
                    sink.log_entries_response(
                        log_entries_response.request_id,
                        logs_response_object,
                    )
                    .await?;
                } else {
                    log::warn!(
                        "Received a LogEntriesResponse from '{agent_name}' without actual data"
                    );
                }
            }

            ToServerEnum::LogsStopResponse(logs_stop_response) => {
                log::trace!("Received LogsStopResponse from '{agent_name}'");

                if let Some(logs_stop_response_object) = logs_stop_response.logs_stop_response {
                    sink.logs_stop_response(
                        logs_stop_response.request_id,
                        logs_stop_response_object,
                    )
                    .await?;
                } else {
                    log::warn!(
                        "Received a LogsStopResponse from '{agent_name}' without actual data"
                    );
                }
            }

            ToServerEnum::AgentHello(agent_hello) => {
                log::warn!(
                    "Received unexpected AgentHello from '{}'.",
                    agent_hello.agent_name
                );
            }

            ToServerEnum::CommanderHello(_) => {
                log::warn!("Received unexpected CommanderHello.");
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

            ToServer::AgentLoadStatus(status) => {
                log::trace!("Received AgentResource from agent {}", status.agent_name);
                grpc_tx
                    .send(grpc_api::ToServer {
                        to_server_enum: Some(grpc_api::to_server::ToServerEnum::AgentLoadStatus(
                            AgentLoadStatus {
                                agent_name: status.agent_name,
                                cpu_usage: status.cpu_usage,
                                free_memory: status.free_memory,
                            }
                            .into(),
                        )),
                    })
                    .await?;
            }

            ToServer::AgentGone(_) => {
                panic!("AgentGone internal messages is not intended to be sent over the network");
            }

            ToServer::LogEntriesResponse(request_id, log_entries_response) => {
                log::trace!("Received LogEntriesResponse for '{request_id}'");
                grpc_tx
                    .send(grpc_api::ToServer {
                        to_server_enum: Some(
                            grpc_api::to_server::ToServerEnum::LogEntriesResponse(
                                grpc_api::LogEntriesResponse {
                                    request_id,
                                    log_entries_response: Some(log_entries_response),
                                },
                            ),
                        ),
                    })
                    .await?;
            }

            ToServer::LogsStopResponse(request_id, logs_stop_response) => {
                log::trace!("Received LogsStopResponse for '{request_id}'");
                grpc_tx
                    .send(grpc_api::ToServer {
                        to_server_enum: Some(grpc_api::to_server::ToServerEnum::LogsStopResponse(
                            grpc_api::LogsStopResponse {
                                request_id,
                                logs_stop_response: Some(LogsStopResponse {
                                    workload_name: logs_stop_response.workload_name,
                                }),
                            },
                        )),
                    })
                    .await?;
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
    use super::{GRPCStreaming, forward_from_ankaios_to_proto, forward_from_proto_to_ankaios};
    use crate::grpc_api::{self, to_server::ToServerEnum};

    use api::ank_base::{
        self, CpuUsageInternal, ExecutionStateInternal, FreeMemoryInternal, LogEntriesResponse,
        LogEntry, LogsRequestInternal, LogsStopResponse, WorkloadInstanceName, WorkloadNamed,
    };
    use api::test_utils::{
        generate_test_complete_state, generate_test_workload,
        generate_test_workload_state_with_agent,
    };

    use async_trait::async_trait;
    use common::to_server_interface::{ToServer, ToServerInterface};
    use std::collections::LinkedList;
    use tokio::sync::mpsc;

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

    const REQUEST_ID: &str = "request_id";
    const AGENT_A_NAME: &str = "agent_A";
    const AGENT_B_NAME: &str = "agent_B";
    const WORKLOAD_1_NAME: &str = "workload_1";
    const WORKLOAD_2_NAME: &str = "workload_2";
    const WORKLOAD_ID_1: &str = "id_1";
    const WORKLOAD_ID_2: &str = "id_2";
    const LOG_MESSAGE_1: &str = "message_1";
    const LOG_MESSAGE_2: &str = "message_2";

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_agent_resources() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let agent_load_status = common::commands::AgentLoadStatus {
            agent_name: AGENT_A_NAME.to_string(),
            cpu_usage: CpuUsageInternal { cpu_usage: 42 },
            free_memory: FreeMemoryInternal { free_memory: 42 },
        };

        let agent_resource_result = server_tx.agent_load_status(agent_load_status.clone()).await;
        assert!(agent_resource_result.is_ok());

        tokio::spawn(async move {
            let _ = forward_from_ankaios_to_proto(grpc_tx, &mut server_rx).await;
        });

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(server_tx);

        let result = grpc_rx.recv().await.unwrap();

        let expected = ToServerEnum::AgentLoadStatus(grpc_api::AgentLoadStatus {
            agent_name: AGENT_A_NAME.to_string(),
            cpu_usage: Some(ank_base::CpuUsage { cpu_usage: 42 }),
            free_memory: Some(ank_base::FreeMemory { free_memory: 42 }),
        });

        assert_eq!(result.to_server_enum, Some(expected));
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_agent_resources() {
        let agent_load_status = common::commands::AgentLoadStatus {
            agent_name: AGENT_A_NAME.to_string(),
            cpu_usage: CpuUsageInternal { cpu_usage: 42 },
            free_memory: FreeMemoryInternal { free_memory: 42 },
        };

        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::AgentLoadStatus(
                        agent_load_status.clone().into(),
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;

        assert!(forward_result.is_ok());

        let result = server_rx.recv().await.unwrap();

        let expected = ToServer::AgentLoadStatus(agent_load_status);

        assert_eq!(result, expected);
    }

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_update_workload() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let workload_named = generate_test_workload::<WorkloadNamed>().name(WORKLOAD_1_NAME);
        let input_state = generate_test_complete_state(vec![workload_named]);
        let update_mask = vec!["bla".into()];

        // As the channel capacity is big enough the await is satisfied right away
        let update_state_result = server_tx
            .update_state(
                REQUEST_ID.to_owned(),
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
            Some(ToServerEnum::Request(ank_base::Request{request_id, request_content: Some(ank_base::request::RequestContent::UpdateStateRequest(update_state_request))}))
            if request_id == REQUEST_ID && update_state_request.new_state == Some(proto_state) && update_state_request.update_mask == update_mask));
    }

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_update_workload_state() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let wl_state = generate_test_workload_state_with_agent(
            WORKLOAD_1_NAME,
            AGENT_A_NAME,
            ExecutionStateInternal::running(),
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
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_err());
        assert_eq!(
            forward_result.unwrap_err().to_string(),
            String::from("Connection interrupted: 'status: 'Unknown error', self: \"test\"'")
        );

        // pick received from server message
        let result = server_rx.recv().await;

        assert_eq!(result, None);
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_handles_missing_to_server() {
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
            AGENT_A_NAME.to_string(),
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
        let (server_tx, mut _server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let workload_named = generate_test_workload::<WorkloadNamed>().name(WORKLOAD_1_NAME);
        let agent_name = workload_named.workload.agent.clone();

        let mut ankaios_state: ank_base::CompleteState =
            generate_test_complete_state(vec![workload_named]).into();
        *ankaios_state
            .desired_state
            .as_mut()
            .unwrap()
            .workloads
            .as_mut()
            .unwrap()
            .workloads
            .get_mut(WORKLOAD_1_NAME)
            .unwrap()
            .dependencies
            .as_mut()
            .unwrap()
            .dependencies
            .get_mut(&String::from("workload_B"))
            .unwrap() = -1;

        let ankaios_update_mask = vec!["bla".into()];

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: REQUEST_ID.to_owned(),
                        request_content: Some(
                            ank_base::request::RequestContent::UpdateStateRequest(Box::new(
                                ank_base::UpdateStateRequest {
                                    new_state: Some(ankaios_state),
                                    update_mask: ankaios_update_mask.clone(),
                                },
                            )),
                        ),
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name,
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_err());
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_update_workload() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let workload_named = generate_test_workload::<WorkloadNamed>().name(WORKLOAD_1_NAME);
        let agent_name = workload_named.workload.agent.clone();

        let ankaios_state = generate_test_complete_state(vec![workload_named]);
        let ankaios_update_mask = vec!["bla".into()];

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: REQUEST_ID.to_owned(),
                        request_content: Some(
                            ank_base::request::RequestContent::UpdateStateRequest(Box::new(
                                ank_base::UpdateStateRequest {
                                    new_state: Some(ankaios_state.clone().into()),
                                    update_mask: ankaios_update_mask.clone(),
                                },
                            )),
                        ),
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            agent_name.clone(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;

        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await.unwrap();
        let expected_prefixed_my_request_id = format!("{agent_name}@{REQUEST_ID}");

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
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let proto_wl_state: ank_base::WorkloadState = generate_test_workload_state_with_agent(
            "fake_workload",
            AGENT_A_NAME,
            ExecutionStateInternal::running(),
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
            AGENT_A_NAME.to_string(),
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
            if workload_states == vec!(proto_wl_state.try_into().unwrap())
        ));
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_request_complete_state() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        // simulate the reception of an update workload state grpc from server message
        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: REQUEST_ID.to_string(),
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
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await.unwrap();
        // [utest->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
        let expected_prefixed_my_request_id = format!("{AGENT_A_NAME}@{REQUEST_ID}");
        let expected_empty_field_mask: Vec<String> = vec![];
        assert!(
            matches!(result, common::to_server_interface::ToServer::Request(common::commands::Request {
                request_id,
                request_content:
                    common::commands::RequestContent::CompleteStateRequest(
                        common::commands::CompleteStateRequest { field_mask },
                    ),
            }) if request_id == expected_prefixed_my_request_id && field_mask == expected_empty_field_mask)
        );
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_request_complete_state() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let request_complete_state = common::commands::CompleteStateRequest { field_mask: vec![] };

        let request_complete_state_result = server_tx
            .request_complete_state(REQUEST_ID.to_owned(), request_complete_state.clone())
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
        if request_id == REQUEST_ID && field_mask == vec![] as Vec<String>));
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_request_logs() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: REQUEST_ID.to_owned(),
                        request_content: Some(ank_base::request::RequestContent::LogsRequest(
                            ank_base::LogsRequest {
                                workload_names: vec![
                                    ank_base::WorkloadInstanceName {
                                        workload_name: WORKLOAD_1_NAME.to_string(),
                                        agent_name: AGENT_A_NAME.to_string(),
                                        id: WORKLOAD_ID_1.to_string(),
                                    },
                                    ank_base::WorkloadInstanceName {
                                        workload_name: WORKLOAD_2_NAME.to_string(),
                                        agent_name: AGENT_A_NAME.to_string(),
                                        id: WORKLOAD_ID_2.to_string(),
                                    },
                                ],
                                follow: Some(true),
                                tail: Some(10),
                                since: Some("since".into()),
                                until: None,
                            },
                        )),
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await.unwrap();
        // [utest->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
        let expected_prefixed_my_request_id = format!("{AGENT_A_NAME}@{REQUEST_ID}");
        let expected_workload_names: Vec<api::ank_base::WorkloadInstanceNameInternal> = vec![
            api::ank_base::WorkloadInstanceNameInternal::new(
                AGENT_A_NAME,
                WORKLOAD_1_NAME,
                WORKLOAD_ID_1,
            ),
            api::ank_base::WorkloadInstanceNameInternal::new(
                AGENT_A_NAME,
                WORKLOAD_2_NAME,
                WORKLOAD_ID_2,
            ),
        ];

        assert!(
            matches!(result, common::to_server_interface::ToServer::Request(common::commands::Request {
                request_id,
                request_content:
                    common::commands::RequestContent::LogsRequest(
                        LogsRequestInternal { workload_names, follow, tail, since, until },
                    ),
            }) if request_id == expected_prefixed_my_request_id
                   && workload_names == expected_workload_names
                   && follow && tail == 10
                   && since == Some("since".into())  && until.is_none())
        );
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_request_cancel_logs() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                        request_id: REQUEST_ID.to_string(),
                        request_content: Some(
                            ank_base::request::RequestContent::LogsCancelRequest(
                                ank_base::LogsCancelRequest {},
                            ),
                        ),
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await.unwrap();
        // [utest->swdd~agent-adds-workload-prefix-id-control-interface-request~1]
        let expected_prefixed_my_request_id = format!("{AGENT_A_NAME}@{REQUEST_ID}");

        assert!(matches!(
            result,
            common::to_server_interface::ToServer::Request(common::commands::Request {
                request_id,
                request_content: common::commands::RequestContent::LogsCancelRequest,
            }) if request_id == expected_prefixed_my_request_id
        ));
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_to_ankaios_to_proto_logs() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::LogEntriesResponse(
                        crate::LogEntriesResponse {
                            request_id: REQUEST_ID.into(),
                            log_entries_response: Some(LogEntriesResponse {
                                log_entries: vec![
                                    LogEntry {
                                        workload_name: Some(WorkloadInstanceName {
                                            workload_name: WORKLOAD_1_NAME.to_string(),
                                            agent_name: AGENT_B_NAME.to_string(),
                                            id: WORKLOAD_ID_1.to_string(),
                                        }),
                                        message: LOG_MESSAGE_1.to_string(),
                                    },
                                    LogEntry {
                                        workload_name: Some(WorkloadInstanceName {
                                            workload_name: WORKLOAD_2_NAME.to_string(),
                                            agent_name: AGENT_B_NAME.to_string(),
                                            id: WORKLOAD_ID_2.to_string(),
                                        }),
                                        message: LOG_MESSAGE_2.to_string(),
                                    },
                                ],
                            }),
                        },
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await.unwrap();

        assert!(matches!(
            result,
            common::to_server_interface::ToServer::LogEntriesResponse(
                request_id,
                ank_base::LogEntriesResponse { log_entries }
            ) if request_id == REQUEST_ID
                 && matches!(log_entries.as_slice(),
                            [ank_base::LogEntry{ workload_name: Some(ank_base::WorkloadInstanceName{ workload_name: workload_name_1, agent_name: agent_name_1, id: id_1 }), message: message_1 },
                             ank_base::LogEntry{ workload_name: Some(ank_base::WorkloadInstanceName{ workload_name: workload_name_2, agent_name: agent_name_2, id: id_2 }), message: message_2 }]
                            if workload_name_1 == WORKLOAD_1_NAME && agent_name_1 == AGENT_B_NAME && id_1 == WORKLOAD_ID_1 && message_1 == LOG_MESSAGE_1
                               && workload_name_2 == WORKLOAD_2_NAME && agent_name_2 == AGENT_B_NAME && id_2 == WORKLOAD_ID_2 && message_2 == LOG_MESSAGE_2)
        ));
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_to_ankaios_to_proto_empty_logs() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::LogEntriesResponse(
                        crate::LogEntriesResponse {
                            request_id: REQUEST_ID.into(),
                            log_entries_response: None,
                        },
                    )),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await;
        // [utest->swdd~agent-adds-workload-prefix-id-control-interface-request~1]

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_logs() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let forward_logs_result = server_tx
            .log_entries_response(
                REQUEST_ID.to_owned(),
                LogEntriesResponse {
                    log_entries: vec![
                        LogEntry {
                            workload_name: Some(WorkloadInstanceName {
                                workload_name: WORKLOAD_1_NAME.to_string(),
                                agent_name: AGENT_B_NAME.to_string(),
                                id: WORKLOAD_ID_1.to_string(),
                            }),
                            message: LOG_MESSAGE_1.to_string(),
                        },
                        LogEntry {
                            workload_name: Some(WorkloadInstanceName {
                                workload_name: WORKLOAD_2_NAME.to_string(),
                                agent_name: AGENT_B_NAME.to_string(),
                                id: WORKLOAD_ID_2.to_string(),
                            }),
                            message: LOG_MESSAGE_2.to_string(),
                        },
                    ],
                },
            )
            .await;

        assert!(forward_logs_result.is_ok());

        tokio::spawn(async move {
            let _ = forward_from_ankaios_to_proto(grpc_tx, &mut server_rx).await;
        });

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(server_tx);

        let result = grpc_rx.recv().await.unwrap();

        assert!(matches!(
            result.to_server_enum,
            Some(ToServerEnum::LogEntriesResponse(grpc_api::LogEntriesResponse {
                request_id,
                log_entries_response: Some(LogEntriesResponse { log_entries })
            })) if request_id == REQUEST_ID
                    && matches!(log_entries.as_slice(),
                                [ank_base::LogEntry{ workload_name: Some(ank_base:: WorkloadInstanceName{ workload_name: workload_name_1, agent_name: agent_name_1, id: id_1 }), message: message_1 },
                                 ank_base::LogEntry{ workload_name: Some(ank_base:: WorkloadInstanceName{ workload_name: workload_name_2, agent_name: agent_name_2, id: id_2 }), message: message_2 }]
                                if workload_name_1 == WORKLOAD_1_NAME && agent_name_1 == AGENT_B_NAME && id_1 == WORKLOAD_ID_1 && message_1 == LOG_MESSAGE_1
                                   && workload_name_2 == WORKLOAD_2_NAME && agent_name_2 == AGENT_B_NAME && id_2 == WORKLOAD_ID_2 && message_2 == LOG_MESSAGE_2)
        ));
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_logs_stop_response() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let request_id = REQUEST_ID.to_string();
        let workload_instance_name = WorkloadInstanceName {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_A_NAME.to_string(),
            id: WORKLOAD_ID_1.to_string(),
        };

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::LogsStopResponse(crate::LogsStopResponse {
                        request_id: request_id.clone(),
                        logs_stop_response: Some(LogsStopResponse {
                            workload_name: Some(workload_instance_name.clone()),
                        }),
                    })),
                }),
                None,
            ]));

        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await.unwrap();

        assert!(matches!(
            result,
            common::to_server_interface::ToServer::LogsStopResponse(
                received_request_id,
                ank_base::LogsStopResponse {
                    workload_name: received_workload_instance_name
                }
            ) if received_request_id == request_id && received_workload_instance_name == Some(workload_instance_name)
        ));
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_empty_logs_stop_response() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::LogsStopResponse(crate::LogsStopResponse {
                        request_id: REQUEST_ID.into(),
                        logs_stop_response: None,
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // pick received from server message
        let result = server_rx.recv().await;

        assert!(result.is_none());
    }

    // [utest->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_ankaios_to_proto_logs_stop_response() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);
        let (grpc_tx, mut grpc_rx) = mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        let request_id = REQUEST_ID.to_string();
        let workload_instance_name = WorkloadInstanceName {
            workload_name: WORKLOAD_1_NAME.to_string(),
            agent_name: AGENT_A_NAME.to_string(),
            id: WORKLOAD_ID_1.to_string(),
        };

        let forward_logs_result = server_tx
            .logs_stop_response(
                request_id.clone(),
                LogsStopResponse {
                    workload_name: Some(workload_instance_name.clone()),
                },
            )
            .await;

        assert!(forward_logs_result.is_ok());

        tokio::spawn(async move {
            let _ = forward_from_ankaios_to_proto(grpc_tx, &mut server_rx).await;
        });

        // The receiver in the agent receives the message and terminates the infinite waiting-loop.
        drop(server_tx);

        let result = grpc_rx.recv().await.unwrap();

        assert_eq!(
            result.to_server_enum,
            Some(ToServerEnum::LogsStopResponse(grpc_api::LogsStopResponse {
                request_id: request_id.clone(),
                logs_stop_response: Some(LogsStopResponse {
                    workload_name: Some(workload_instance_name)
                })
            }))
        );
    }

    // [utest->swdd~grpc-agent-connection-forwards-commands-to-server~1]
    #[tokio::test]
    async fn utest_to_server_command_forward_from_proto_to_ankaios_ignore_unexpected_messages() {
        let (server_tx, mut server_rx) = mpsc::channel::<ToServer>(common::CHANNEL_CAPACITY);

        let mut mock_grpc_ex_request_streaming =
            MockGRPCToServerStreaming::new(LinkedList::from([
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::AgentHello(crate::AgentHello {
                        agent_name: AGENT_A_NAME.to_string(),
                        protocol_version: common::ANKAIOS_VERSION.into(),
                    })),
                }),
                Some(grpc_api::ToServer {
                    to_server_enum: Some(ToServerEnum::CommanderHello(crate::CommanderHello {
                        protocol_version: common::ANKAIOS_VERSION.into(),
                    })),
                }),
                None,
            ]));

        // forwards from proto to ankaios
        let forward_result = forward_from_proto_to_ankaios(
            AGENT_A_NAME.to_string(),
            &mut mock_grpc_ex_request_streaming,
            server_tx,
        )
        .await;
        assert!(forward_result.is_ok());

        // assert ignored AgentHello
        let result = server_rx.recv().await;
        assert!(result.is_none());

        // assert ignored CommanderHello
        let result = server_rx.recv().await;
        assert!(result.is_none());
    }
}

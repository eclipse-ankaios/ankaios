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

use crate::commands::{self, LogsRequest};
use crate::objects::{DeletedWorkload, WorkloadSpec, WorkloadState};
use api::ank_base;
use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc::error::SendError;
#[derive(Debug)]
pub struct FromServerInterfaceError(String);

impl fmt::Display for FromServerInterfaceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FromServerInterfaceError: '{}'", self.0)
    }
}

impl From<SendError<FromServer>> for FromServerInterfaceError {
    fn from(error: SendError<FromServer>) -> Self {
        FromServerInterfaceError(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FromServer {
    ServerHello(commands::ServerHello),
    UpdateWorkload(commands::UpdateWorkload),
    UpdateWorkloadState(commands::UpdateWorkloadState),
    Response(ank_base::Response),
    Stop(commands::Stop),
    LogsRequest(String, LogsRequest),
    LogsCancelRequest(String),
    ServerGone,
}

// [impl->swdd~from-server-channel~1]
#[async_trait]
pub trait FromServerInterface {
    async fn server_hello(
        &self,
        agent_name: Option<String>,
        added_workloads: Vec<WorkloadSpec>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn update_workload(
        &self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn update_workload_state(
        &self,
        workload_running: Vec<WorkloadState>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn response(&self, response: ank_base::Response) -> Result<(), FromServerInterfaceError>;
    async fn complete_state(
        &self,
        request_id: String,
        complete_state: ank_base::CompleteState,
    ) -> Result<(), FromServerInterfaceError>;
    async fn update_state_success(
        &self,
        request_id: String,
        added_workloads: Vec<String>,
        deleted_workloads: Vec<String>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn logs_request(
        &self,
        request_id: String,
        logs_request: ank_base::LogsRequest,
    ) -> Result<(), FromServerInterfaceError>;
    async fn logs_request_accepted(
        &self,
        request_id: String,
        logs_request: ank_base::LogsRequest,
    ) -> Result<(), FromServerInterfaceError>;
    async fn log_entries_response(
        &self,
        request_id: String,
        logs_response: ank_base::LogEntriesResponse,
    ) -> Result<(), FromServerInterfaceError>;
    async fn logs_stop_response(
        &self,
        request_id: String,
        logs_stop_response: ank_base::LogsStopResponse,
    ) -> Result<(), FromServerInterfaceError>;
    async fn logs_cancel_request(&self, request_id: String)
        -> Result<(), FromServerInterfaceError>;
    async fn logs_cancel_request_accepted(
        &self,
        request_id: String,
    ) -> Result<(), FromServerInterfaceError>;
    async fn error(
        &self,
        request_id: String,
        message: String,
    ) -> Result<(), FromServerInterfaceError>;
    async fn stop(&self) -> Result<(), FromServerInterfaceError>;
}

pub type FromServerSender = tokio::sync::mpsc::Sender<FromServer>;
pub type FromServerReceiver = tokio::sync::mpsc::Receiver<FromServer>;

#[async_trait]
impl FromServerInterface for FromServerSender {
    async fn server_hello(
        &self,
        // This is a workaround for not having a request-response model dedicated for the communication middleware
        agent_name: Option<String>,
        added_workloads: Vec<WorkloadSpec>,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::ServerHello(commands::ServerHello {
                agent_name,
                added_workloads,
            }))
            .await?)
    }

    async fn update_workload(
        &self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::UpdateWorkload(commands::UpdateWorkload {
                added_workloads,
                deleted_workloads,
            }))
            .await?)
    }

    async fn update_workload_state(
        &self,
        workload_states: Vec<WorkloadState>,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::UpdateWorkloadState(
                commands::UpdateWorkloadState { workload_states },
            ))
            .await?)
    }

    async fn response(&self, response: ank_base::Response) -> Result<(), FromServerInterfaceError> {
        Ok(self.send(FromServer::Response(response)).await?)
    }

    async fn complete_state(
        &self,
        request_id: String,
        complete_state: api::ank_base::CompleteState,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::Response(ank_base::Response {
                request_id,
                response_content: ank_base::response::ResponseContent::CompleteState(
                    complete_state,
                )
                .into(),
            }))
            .await?)
    }

    async fn update_state_success(
        &self,
        request_id: String,
        added_workloads: Vec<String>,
        deleted_workloads: Vec<String>,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::Response(ank_base::Response {
                request_id,
                response_content: ank_base::response::ResponseContent::UpdateStateSuccess(
                    ank_base::UpdateStateSuccess {
                        added_workloads,
                        deleted_workloads,
                    },
                )
                .into(),
            }))
            .await?)
    }

    async fn logs_request(
        &self,
        request_id: String,
        logs_request: ank_base::LogsRequest,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::LogsRequest(request_id, logs_request.into()))
            .await?;
        Ok(())
    }

    async fn logs_request_accepted(
        &self,
        request_id: String,
        logs_request: ank_base::LogsRequest,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(ank_base::Response {
            request_id,
            response_content: ank_base::response::ResponseContent::LogsRequestAccepted(
                ank_base::LogsRequestAccepted {
                    workload_names: logs_request.workload_names,
                },
            )
            .into(),
        }))
        .await?;
        Ok(())
    }

    async fn log_entries_response(
        &self,
        request_id: String,
        logs_response: ank_base::LogEntriesResponse,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(ank_base::Response {
            request_id,
            response_content: ank_base::response::ResponseContent::LogEntriesResponse(
                logs_response,
            )
            .into(),
        }))
        .await?;
        Ok(())
    }

    async fn logs_stop_response(
        &self,
        request_id: String,
        logs_stop_response: ank_base::LogsStopResponse,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(ank_base::Response {
            request_id,
            response_content: ank_base::response::ResponseContent::LogsStopResponse(
                logs_stop_response,
            )
            .into(),
        }))
        .await?;
        Ok(())
    }

    async fn logs_cancel_request(
        &self,
        request_id: String,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::LogsCancelRequest(request_id)).await?;
        Ok(())
    }

    async fn logs_cancel_request_accepted(
        &self,
        request_id: String,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(ank_base::Response {
            request_id,
            response_content: ank_base::response::ResponseContent::LogsCancelAccepted(
                ank_base::LogsCancelAccepted {},
            )
            .into(),
        }))
        .await?;
        Ok(())
    }

    async fn error(
        &self,
        request_id: String,
        message: String,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::Response(ank_base::Response {
                request_id,
                response_content: ank_base::response::ResponseContent::Error(ank_base::Error {
                    message,
                })
                .into(),
            }))
            .await?)
    }

    async fn stop(&self) -> Result<(), FromServerInterfaceError> {
        Ok(self.send(FromServer::Stop(commands::Stop {})).await?)
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
    use super::ank_base;
    use crate::{
        commands,
        from_server_interface::{FromServer, FromServerInterface},
        objects::{
            generate_test_workload_spec, generate_test_workload_state, ExecutionState,
            WorkloadInstanceName,
        },
        test_utils::{generate_test_complete_state, generate_test_deleted_workload},
    };

    use super::{FromServerReceiver, FromServerSender};

    const TEST_CHANNEL_CAPA: usize = 5;
    const WORKLOAD_NAME_1: &str = "X";
    const WORKLOAD_NAME_2: &str = "Y";
    const AGENT_NAME: &str = "agent_A";
    const REQUEST_ID: &str = "emkw489ejf89ml";

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_workload() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let added_workloads = vec![generate_test_workload_spec()];
        let deleted_workloads = vec![generate_test_deleted_workload(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME_1.to_string(),
        )];
        assert!(tx
            .update_workload(added_workloads.clone(), deleted_workloads.clone())
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::UpdateWorkload(commands::UpdateWorkload {
                added_workloads,
                deleted_workloads,
            })
        )
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_workload_state() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let workload_state =
            generate_test_workload_state(WORKLOAD_NAME_1, ExecutionState::running());
        assert!(tx
            .update_workload_state(vec![workload_state.clone()])
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::UpdateWorkloadState(commands::UpdateWorkloadState {
                workload_states: vec![workload_state],
            })
        )
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_complete_state() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let complete_state: ank_base::CompleteState =
            generate_test_complete_state(vec![generate_test_workload_spec()]).into();
        assert!(tx
            .complete_state(REQUEST_ID.to_string(), complete_state.clone())
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID.to_string(),
                response_content: Some(ank_base::response::ResponseContent::CompleteState(
                    complete_state
                )),
            })
        )
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_state_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let added_workloads = vec!["some_name".to_string(), "some_other_name".to_string()];
        let deleted_workloads = vec!["some_name_1".to_string(), "some_other_name_1".to_string()];
        assert!(tx
            .update_state_success(
                REQUEST_ID.to_string(),
                added_workloads.clone(),
                deleted_workloads.clone()
            )
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID.to_string(),
                response_content: Some(ank_base::response::ResponseContent::UpdateStateSuccess(
                    ank_base::UpdateStateSuccess {
                        added_workloads,
                        deleted_workloads,
                    },
                )),
            })
        )
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_error() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let error = ank_base::Error {
            message: "error".to_string(),
        };
        assert!(tx
            .error(REQUEST_ID.to_string(), error.message.clone())
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID.to_string(),
                response_content: Some(ank_base::response::ResponseContent::Error(error)),
            })
        )
    }

    #[tokio::test]
    async fn utest_logs_request_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        assert!(tx
            .logs_request(
                REQUEST_ID.to_string(),
                ank_base::LogsRequest {
                    workload_names: vec![
                        ank_base::WorkloadInstanceName {
                            workload_name: WORKLOAD_NAME_1.into(),
                            agent_name: AGENT_NAME.into(),
                            id: "1".into()
                        },
                        ank_base::WorkloadInstanceName {
                            workload_name: WORKLOAD_NAME_2.into(),
                            agent_name: AGENT_NAME.into(),
                            id: "2".into()
                        }
                    ],
                    follow: Some(true),
                    tail: Some(10),
                    since: None,
                    until: None
                }
            )
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::LogsRequest(
                REQUEST_ID.into(),
                commands::LogsRequest {
                    workload_names: vec![
                        WorkloadInstanceName::new(AGENT_NAME, WORKLOAD_NAME_1, "1"),
                        WorkloadInstanceName::new(AGENT_NAME, WORKLOAD_NAME_2, "2")
                    ],
                    follow: true,
                    tail: 10,
                    since: None,
                    until: None
                }
            )
        )
    }

    #[tokio::test]
    async fn utest_logs_request_fail() {
        let (tx, _): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        assert!(tx
            .logs_request(
                REQUEST_ID.to_string(),
                ank_base::LogsRequest {
                    workload_names: vec![ank_base::WorkloadInstanceName {
                        workload_name: WORKLOAD_NAME_1.into(),
                        agent_name: AGENT_NAME.into(),
                        id: "1".into()
                    }],
                    follow: Some(true),
                    tail: Some(10),
                    since: None,
                    until: None
                }
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn utest_logs_response_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        assert!(tx
            .log_entries_response(
                REQUEST_ID.into(),
                ank_base::LogEntriesResponse {
                    log_entries: vec![
                        ank_base::LogEntry {
                            workload_name: Some(ank_base::WorkloadInstanceName {
                                workload_name: WORKLOAD_NAME_1.into(),
                                agent_name: AGENT_NAME.into(),
                                id: "1".into()
                            }),
                            message: "message_1".into()
                        },
                        ank_base::LogEntry {
                            workload_name: Some(ank_base::WorkloadInstanceName {
                                workload_name: WORKLOAD_NAME_2.into(),
                                agent_name: AGENT_NAME.into(),
                                id: "2".into()
                            }),
                            message: "message_2".into()
                        }
                    ]
                }
            )
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID.into(),
                response_content: Some(ank_base::response::ResponseContent::LogEntriesResponse(
                    ank_base::LogEntriesResponse {
                        log_entries: vec![
                            ank_base::LogEntry {
                                workload_name: Some(ank_base::WorkloadInstanceName {
                                    workload_name: WORKLOAD_NAME_1.into(),
                                    agent_name: AGENT_NAME.into(),
                                    id: "1".into()
                                }),
                                message: "message_1".into()
                            },
                            ank_base::LogEntry {
                                workload_name: Some(ank_base::WorkloadInstanceName {
                                    workload_name: WORKLOAD_NAME_2.into(),
                                    agent_name: AGENT_NAME.into(),
                                    id: "2".into()
                                }),
                                message: "message_2".into()
                            }
                        ]
                    }
                ))
            })
        )
    }

    #[tokio::test]
    async fn utest_logs_response_fail() {
        let (tx, _): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        assert!(tx
            .log_entries_response(
                REQUEST_ID.into(),
                ank_base::LogEntriesResponse {
                    log_entries: vec![ank_base::LogEntry {
                        workload_name: Some(ank_base::WorkloadInstanceName {
                            workload_name: WORKLOAD_NAME_1.into(),
                            agent_name: AGENT_NAME.into(),
                            id: "1".into()
                        }),
                        message: "message_1".into()
                    }]
                }
            )
            .await
            .is_err());
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_logs_stop_response_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let workload_instance_name = ank_base::WorkloadInstanceName {
            workload_name: WORKLOAD_NAME_1.into(),
            agent_name: AGENT_NAME.into(),
            id: "1".into(),
        };

        assert!(tx
            .logs_stop_response(
                REQUEST_ID.to_string(),
                ank_base::LogsStopResponse {
                    workload_name: Some(workload_instance_name.clone()),
                }
            )
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await,
            Some(FromServer::Response(ank_base::Response {
                request_id: REQUEST_ID.to_string(),
                response_content: Some(ank_base::response::ResponseContent::LogsStopResponse(
                    ank_base::LogsStopResponse {
                        workload_name: Some(workload_instance_name),
                    }
                )),
            }))
        );
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_logs_stop_response_fail() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        rx.close();

        assert!(tx
            .logs_stop_response(
                REQUEST_ID.to_string(),
                ank_base::LogsStopResponse {
                    workload_name: Some(ank_base::WorkloadInstanceName {
                        workload_name: WORKLOAD_NAME_1.into(),
                        agent_name: AGENT_NAME.into(),
                        id: "1".into(),
                    }),
                }
            )
            .await
            .is_err());
    }
}

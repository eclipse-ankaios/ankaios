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

use crate::commands;
use ankaios_api::ank_base::{
    AlteredFields, CompleteState, CompleteStateResponse, DeletedWorkload, Error,
    EventsCancelAccepted, LogEntriesResponse, LogsCancelAccepted, LogsRequest, LogsRequestAccepted,
    LogsStopResponse, Response, ResponseContent, UpdateStateSuccess, WorkloadNamed,
    WorkloadStateSpec,
};

use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc::{self, error::SendError};

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
    Response(Response),
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
        added_workloads: Vec<WorkloadNamed>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn update_workload(
        &self,
        added_workloads: Vec<WorkloadNamed>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn update_workload_state(
        &self,
        workload_running: Vec<WorkloadStateSpec>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn response(&self, response: Response) -> Result<(), FromServerInterfaceError>;
    async fn complete_state(
        &self,
        request_id: String,
        complete_state: CompleteState,
        altered_fields: Option<AlteredFields>,
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
        logs_request: LogsRequest,
    ) -> Result<(), FromServerInterfaceError>;
    async fn logs_request_accepted(
        &self,
        request_id: String,
        logs_request: LogsRequest,
    ) -> Result<(), FromServerInterfaceError>;
    async fn log_entries_response(
        &self,
        request_id: String,
        logs_response: LogEntriesResponse,
    ) -> Result<(), FromServerInterfaceError>;
    async fn logs_stop_response(
        &self,
        request_id: String,
        logs_stop_response: LogsStopResponse,
    ) -> Result<(), FromServerInterfaceError>;
    async fn logs_cancel_request(&self, request_id: String)
    -> Result<(), FromServerInterfaceError>;
    async fn logs_cancel_request_accepted(
        &self,
        request_id: String,
    ) -> Result<(), FromServerInterfaceError>;
    async fn event_cancel_request_accepted(
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

pub type FromServerSender = mpsc::Sender<FromServer>;
pub type FromServerReceiver = mpsc::Receiver<FromServer>;

#[async_trait]
impl FromServerInterface for FromServerSender {
    async fn server_hello(
        &self,
        // This is a workaround for not having a request-response model dedicated for the communication middleware
        agent_name: Option<String>,
        added_workloads: Vec<WorkloadNamed>,
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
        added_workloads: Vec<WorkloadNamed>,
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
        workload_states: Vec<WorkloadStateSpec>,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::UpdateWorkloadState(
                commands::UpdateWorkloadState { workload_states },
            ))
            .await?)
    }

    async fn response(&self, response: Response) -> Result<(), FromServerInterfaceError> {
        Ok(self.send(FromServer::Response(response)).await?)
    }

    async fn complete_state(
        &self,
        request_id: String,
        complete_state: CompleteState,
        altered_fields: Option<AlteredFields>,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::Response(Response {
                request_id,
                response_content: ResponseContent::CompleteStateResponse(Box::new(
                    CompleteStateResponse {
                        complete_state: Some(complete_state),
                        altered_fields,
                    },
                ))
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
            .send(FromServer::Response(Response {
                request_id,
                response_content: ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                    added_workloads,
                    deleted_workloads,
                })
                .into(),
            }))
            .await?)
    }

    async fn logs_request(
        &self,
        request_id: String,
        logs_request: LogsRequest,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::LogsRequest(request_id, logs_request))
            .await?;
        Ok(())
    }

    async fn logs_request_accepted(
        &self,
        request_id: String,
        logs_request: LogsRequest,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(Response {
            request_id,
            response_content: ResponseContent::LogsRequestAccepted(LogsRequestAccepted {
                workload_names: logs_request.workload_names,
            })
            .into(),
        }))
        .await?;
        Ok(())
    }

    async fn log_entries_response(
        &self,
        request_id: String,
        logs_response: LogEntriesResponse,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(Response {
            request_id,
            response_content: ResponseContent::LogEntriesResponse(logs_response).into(),
        }))
        .await?;
        Ok(())
    }

    async fn logs_stop_response(
        &self,
        request_id: String,
        logs_stop_response: LogsStopResponse,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(Response {
            request_id,
            response_content: ResponseContent::LogsStopResponse(logs_stop_response).into(),
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
        self.send(FromServer::Response(Response {
            request_id,
            response_content: ResponseContent::LogsCancelAccepted(LogsCancelAccepted {}).into(),
        }))
        .await?;
        Ok(())
    }

    async fn event_cancel_request_accepted(
        &self,
        request_id: String,
    ) -> Result<(), FromServerInterfaceError> {
        self.send(FromServer::Response(Response {
            request_id,
            response_content: ResponseContent::EventsCancelAccepted(EventsCancelAccepted {}).into(),
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
            .send(FromServer::Response(Response {
                request_id,
                response_content: ResponseContent::Error(Error { message }).into(),
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
    use super::{FromServerReceiver, FromServerSender};
    use crate::{
        commands,
        from_server_interface::{FromServer, FromServerInterface},
    };

    use ankaios_api::ank_base::{
        CompleteState, CompleteStateResponse, Error, ExecutionStateSpec, LogEntriesResponse,
        LogEntry, LogsRequest, LogsStopResponse, Response, ResponseContent, UpdateStateSuccess,
        WorkloadInstanceName,
    };
    use ankaios_api::test_utils::{
        fixtures, generate_test_complete_state, generate_test_deleted_workload_with_params,
        generate_test_workload_named, generate_test_workload_state,
    };

    use tokio::sync::mpsc;

    const LOG_MESSAGE_1: &str = "message_1";
    const LOG_MESSAGE_2: &str = "message_2";

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_workload() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let added_workloads = vec![generate_test_workload_named()];
        let deleted_workloads = vec![generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0].to_string(),
            fixtures::WORKLOAD_NAMES[0].to_string(),
        )];
        assert!(
            tx.update_workload(added_workloads.clone(), deleted_workloads.clone())
                .await
                .is_ok()
        );

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
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload_state = generate_test_workload_state(
            fixtures::WORKLOAD_NAMES[0],
            ExecutionStateSpec::running(),
        );
        assert!(
            tx.update_workload_state(vec![workload_state.clone()])
                .await
                .is_ok()
        );

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
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let complete_state: CompleteState =
            generate_test_complete_state(vec![generate_test_workload_named()]).into();
        assert!(
            tx.complete_state(
                fixtures::REQUEST_ID.to_string(),
                complete_state.clone(),
                None
            )
            .await
            .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::CompleteStateResponse(Box::new(
                    CompleteStateResponse {
                        complete_state: Some(complete_state.clone()),
                        altered_fields: Default::default(),
                    }
                ))),
            })
        )
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_state_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let added_workloads = vec!["some_name".to_string(), "some_other_name".to_string()];
        let deleted_workloads = vec!["some_name_1".to_string(), "some_other_name_1".to_string()];
        assert!(
            tx.update_state_success(
                fixtures::REQUEST_ID.to_string(),
                added_workloads.clone(),
                deleted_workloads.clone()
            )
            .await
            .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::UpdateStateSuccess(UpdateStateSuccess {
                    added_workloads,
                    deleted_workloads,
                },)),
            })
        )
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_error() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let error = Error {
            message: "error".to_string(),
        };
        assert!(
            tx.error(fixtures::REQUEST_ID.to_string(), error.message.clone())
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::Error(error)),
            })
        )
    }

    #[tokio::test]
    async fn utest_logs_request_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        assert!(
            tx.logs_request(
                fixtures::REQUEST_ID.to_string(),
                LogsRequest {
                    workload_names: vec![
                        WorkloadInstanceName {
                            workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                            agent_name: fixtures::AGENT_NAMES[0].into(),
                            id: fixtures::WORKLOAD_IDS[0].into()
                        },
                        WorkloadInstanceName {
                            workload_name: fixtures::WORKLOAD_NAMES[1].into(),
                            agent_name: fixtures::AGENT_NAMES[0].into(),
                            id: fixtures::WORKLOAD_IDS[1].into()
                        }
                    ],
                    follow: Some(true),
                    tail: Some(10),
                    since: None,
                    until: None
                }
            )
            .await
            .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::LogsRequest(
                fixtures::REQUEST_ID.into(),
                LogsRequest {
                    workload_names: vec![
                        WorkloadInstanceName {
                            agent_name: fixtures::AGENT_NAMES[0].into(),
                            workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                            id: fixtures::WORKLOAD_IDS[0].into()
                        },
                        WorkloadInstanceName {
                            agent_name: fixtures::AGENT_NAMES[0].into(),
                            workload_name: fixtures::WORKLOAD_NAMES[1].into(),
                            id: fixtures::WORKLOAD_IDS[1].into()
                        }
                    ],
                    follow: Some(true),
                    tail: Some(10),
                    since: None,
                    until: None
                }
            )
        )
    }

    #[tokio::test]
    async fn utest_logs_request_fail() {
        let (tx, _): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        assert!(
            tx.logs_request(
                fixtures::REQUEST_ID.to_string(),
                LogsRequest {
                    workload_names: vec![WorkloadInstanceName {
                        workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                        agent_name: fixtures::AGENT_NAMES[0].into(),
                        id: fixtures::WORKLOAD_IDS[0].into()
                    }],
                    follow: Some(true),
                    tail: Some(10),
                    since: None,
                    until: None
                }
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn utest_logs_response_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        assert!(
            tx.log_entries_response(
                fixtures::REQUEST_ID.into(),
                LogEntriesResponse {
                    log_entries: vec![
                        LogEntry {
                            workload_name: Some(WorkloadInstanceName {
                                workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                                agent_name: fixtures::AGENT_NAMES[0].into(),
                                id: fixtures::WORKLOAD_IDS[0].into()
                            }),
                            message: LOG_MESSAGE_1.into()
                        },
                        LogEntry {
                            workload_name: Some(WorkloadInstanceName {
                                workload_name: fixtures::WORKLOAD_NAMES[1].into(),
                                agent_name: fixtures::AGENT_NAMES[0].into(),
                                id: fixtures::WORKLOAD_IDS[1].into()
                            }),
                            message: LOG_MESSAGE_2.into()
                        }
                    ]
                }
            )
            .await
            .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.into(),
                response_content: Some(ResponseContent::LogEntriesResponse(LogEntriesResponse {
                    log_entries: vec![
                        LogEntry {
                            workload_name: Some(WorkloadInstanceName {
                                workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                                agent_name: fixtures::AGENT_NAMES[0].into(),
                                id: fixtures::WORKLOAD_IDS[0].into()
                            }),
                            message: LOG_MESSAGE_1.into()
                        },
                        LogEntry {
                            workload_name: Some(WorkloadInstanceName {
                                workload_name: fixtures::WORKLOAD_NAMES[1].into(),
                                agent_name: fixtures::AGENT_NAMES[0].into(),
                                id: fixtures::WORKLOAD_IDS[1].into()
                            }),
                            message: LOG_MESSAGE_2.into()
                        }
                    ]
                }))
            })
        )
    }

    #[tokio::test]
    async fn utest_logs_response_fail() {
        let (tx, _): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        assert!(
            tx.log_entries_response(
                fixtures::REQUEST_ID.into(),
                LogEntriesResponse {
                    log_entries: vec![LogEntry {
                        workload_name: Some(WorkloadInstanceName {
                            workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                            agent_name: fixtures::AGENT_NAMES[0].into(),
                            id: fixtures::WORKLOAD_IDS[0].into()
                        }),
                        message: LOG_MESSAGE_1.into()
                    }]
                }
            )
            .await
            .is_err()
        );
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_logs_stop_response_success() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload_instance_name = WorkloadInstanceName {
            workload_name: fixtures::WORKLOAD_NAMES[0].into(),
            agent_name: fixtures::AGENT_NAMES[0].into(),
            id: fixtures::WORKLOAD_IDS[0].into(),
        };

        assert!(
            tx.logs_stop_response(
                fixtures::REQUEST_ID.to_string(),
                LogsStopResponse {
                    workload_name: Some(workload_instance_name.clone()),
                }
            )
            .await
            .is_ok()
        );

        assert_eq!(
            rx.recv().await,
            Some(FromServer::Response(Response {
                request_id: fixtures::REQUEST_ID.to_string(),
                response_content: Some(ResponseContent::LogsStopResponse(LogsStopResponse {
                    workload_name: Some(workload_instance_name),
                })),
            }))
        );
    }

    // [utest->swdd~from-server-channel~1]
    #[tokio::test]
    async fn utest_logs_stop_response_fail() {
        let (tx, mut rx): (FromServerSender, FromServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        rx.close();

        assert!(
            tx.logs_stop_response(
                fixtures::REQUEST_ID.to_string(),
                LogsStopResponse {
                    workload_name: Some(WorkloadInstanceName {
                        workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                        agent_name: fixtures::AGENT_NAMES[0].into(),
                        id: fixtures::WORKLOAD_IDS[0].into(),
                    }),
                }
            )
            .await
            .is_err()
        );
    }
}

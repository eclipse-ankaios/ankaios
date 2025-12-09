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

use crate::commands::{self, AgentLoadStatus};
use crate::std_extensions::UnreachableResult;
use ankaios_api::ank_base::{
    CompleteStateRequestSpec, CompleteStateSpec, LogEntriesResponse, LogsCancelRequestSpec,
    LogsRequest, LogsRequestSpec, LogsStopResponse, RequestContentSpec, RequestSpec,
    UpdateStateRequestSpec, WorkloadStateSpec,
};

use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc::{self, error::SendError};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Clone)]
pub enum ToServer {
    AgentHello(commands::AgentHello),
    AgentLoadStatus(AgentLoadStatus),
    AgentGone(commands::AgentGone),
    Request(RequestSpec),
    UpdateWorkloadState(commands::UpdateWorkloadState),
    Stop(commands::Stop),
    Goodbye(commands::Goodbye),
    LogEntriesResponse(String, LogEntriesResponse),
    LogsStopResponse(String, LogsStopResponse),
}

#[derive(Debug)]
pub struct ToServerError(String);

impl From<SendError<ToServer>> for ToServerError {
    fn from(error: SendError<ToServer>) -> Self {
        ToServerError(error.to_string())
    }
}

impl fmt::Display for ToServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ToServerError: '{}'", self.0)
    }
}

// [impl->swdd~to-server-channel~1]
#[async_trait]
pub trait ToServerInterface {
    async fn agent_hello(&self, agent_name: String) -> Result<(), ToServerError>;
    async fn agent_load_status(&self, agent_resource: AgentLoadStatus)
    -> Result<(), ToServerError>;
    async fn agent_gone(&self, agent_name: String) -> Result<(), ToServerError>;
    async fn update_state(
        &self,
        request_id: String,
        new_state: CompleteStateSpec,
        update_mask: Vec<String>,
    ) -> Result<(), ToServerError>;
    async fn update_workload_state(
        &self,
        workload_running: Vec<WorkloadStateSpec>,
    ) -> Result<(), ToServerError>;
    async fn request_complete_state(
        &self,
        request_id: String,
        request_complete_state: CompleteStateRequestSpec,
    ) -> Result<(), ToServerError>;
    async fn logs_request(
        &self,
        request_id: String,
        logs_request: LogsRequest,
    ) -> Result<(), ToServerError>;
    async fn logs_cancel_request(&self, request_id: String) -> Result<(), ToServerError>;
    async fn log_entries_response(
        &self,
        request_id: String,
        logs_response: LogEntriesResponse,
    ) -> Result<(), ToServerError>;
    async fn logs_stop_response(
        &self,
        request_id: String,
        logs_stop_response: LogsStopResponse,
    ) -> Result<(), ToServerError>;
    async fn goodbye(&self, connection_name: String) -> Result<(), ToServerError>;
    async fn stop(&self) -> Result<(), ToServerError>;
}

pub type ToServerSender = mpsc::Sender<ToServer>;
pub type ToServerReceiver = mpsc::Receiver<ToServer>;

#[async_trait]
impl ToServerInterface for ToServerSender {
    async fn agent_hello(&self, agent_name: String) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::AgentHello(commands::AgentHello { agent_name }))
            .await?)
    }

    async fn agent_load_status(
        &self,
        agent_load_status: AgentLoadStatus,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::AgentLoadStatus(agent_load_status))
            .await?)
    }

    async fn agent_gone(&self, agent_name: String) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::AgentGone(commands::AgentGone { agent_name }))
            .await?)
    }

    async fn update_state(
        &self,
        request_id: String,
        new_state: CompleteStateSpec,
        update_mask: Vec<String>,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::Request(RequestSpec {
                request_id,
                request_content: RequestContentSpec::UpdateStateRequest(Box::new(
                    UpdateStateRequestSpec {
                        new_state,
                        update_mask,
                    },
                )),
            }))
            .await?)
    }

    async fn update_workload_state(
        &self,
        workload_running: Vec<WorkloadStateSpec>,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::UpdateWorkloadState(
                commands::UpdateWorkloadState {
                    workload_states: workload_running,
                },
            ))
            .await?)
    }

    async fn request_complete_state(
        &self,
        request_id: String,
        request_complete_state: CompleteStateRequestSpec,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::Request(RequestSpec {
                request_id,
                request_content: RequestContentSpec::CompleteStateRequest(
                    CompleteStateRequestSpec {
                        field_mask: request_complete_state.field_mask,
                    },
                ),
            }))
            .await?)
    }

    async fn logs_request(
        &self,
        request_id: String,
        logs_request: LogsRequest,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::Request(RequestSpec {
                request_id,
                request_content: RequestContentSpec::LogsRequest(
                    LogsRequestSpec::try_from(logs_request).unwrap_or_unreachable(),
                ),
            }))
            .await?)
    }

    async fn logs_cancel_request(&self, request_id: String) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::Request(RequestSpec {
                request_id,
                request_content: RequestContentSpec::LogsCancelRequest(LogsCancelRequestSpec {}),
            }))
            .await?)
    }

    async fn log_entries_response(
        &self,
        request_id: String,
        logs_response: LogEntriesResponse,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::LogEntriesResponse(request_id, logs_response))
            .await?)
    }

    async fn logs_stop_response(
        &self,
        request_id: String,
        logs_stop_response: LogsStopResponse,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::LogsStopResponse(request_id, logs_stop_response))
            .await?)
    }

    async fn goodbye(&self, connection_name: String) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::Goodbye(commands::Goodbye { connection_name }))
            .await?)
    }

    async fn stop(&self) -> Result<(), ToServerError> {
        Ok(self.send(ToServer::Stop(commands::Stop {})).await?)
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
    use super::{ToServerReceiver, ToServerSender};
    use crate::{
        commands::{self, AgentLoadStatus},
        to_server_interface::{ToServer, ToServerInterface},
    };
    use ankaios_api::ank_base::{
        CompleteStateRequestSpec, ExecutionStateSpec,
        LogEntriesResponse, LogEntry, LogsCancelRequestSpec, LogsRequestSpec, LogsStopResponse,
        RequestContentSpec, RequestSpec, UpdateStateRequestSpec, WorkloadInstanceName,
        WorkloadInstanceNameSpec,
    };
    use ankaios_api::test_utils::{
        generate_test_complete_state, generate_test_workload_named, generate_test_workload_state,
        fixtures,
    };
    use tokio::sync::mpsc;

    const FIELD_MASK: &str = "desiredState.bla_bla";

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_agent_hello() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        assert!(
            tx.agent_hello(fixtures::AGENT_NAMES[0].to_string())
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::AgentHello(commands::AgentHello {
                agent_name: fixtures::AGENT_NAMES[0].to_string()
            })
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_agent_load_status() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        assert!(
            tx.agent_load_status(AgentLoadStatus {
                agent_name: fixtures::AGENT_NAMES[0].to_string(),
                cpu_usage: fixtures::CPU_USAGE_SPEC,
                free_memory: fixtures::FREE_MEMORY_SPEC,
            })
            .await
            .is_ok()
        );

        assert_eq!(
            rx.recv().await,
            Some(ToServer::AgentLoadStatus(AgentLoadStatus {
                agent_name: fixtures::AGENT_NAMES[0].to_string(),
                cpu_usage: fixtures::CPU_USAGE_SPEC,
                free_memory: fixtures::FREE_MEMORY_SPEC,
            }))
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_agent_gone() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        assert!(
            tx.agent_gone(fixtures::AGENT_NAMES[0].to_string())
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::AgentGone(commands::AgentGone {
                agent_name: fixtures::AGENT_NAMES[0].to_string()
            })
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_state() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload1 = generate_test_workload_named();
        let complete_state = generate_test_complete_state(vec![workload1]);
        assert!(
            tx.update_state(
                fixtures::REQUEST_ID.to_string(),
                complete_state.clone(),
                vec![FIELD_MASK.to_string()]
            )
            .await
            .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::Request(RequestSpec {
                request_id: fixtures::REQUEST_ID.to_string(),
                request_content: RequestContentSpec::UpdateStateRequest(Box::new(
                    UpdateStateRequestSpec {
                        new_state: complete_state,
                        update_mask: vec![FIELD_MASK.to_string()]
                    },
                )),
            })
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_workload_state() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let workload_state =
            generate_test_workload_state(fixtures::WORKLOAD_NAMES[0], ExecutionStateSpec::running());
        assert!(
            tx.update_workload_state(vec![workload_state.clone()])
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::UpdateWorkloadState(commands::UpdateWorkloadState {
                workload_states: vec![workload_state],
            })
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_request_complete_state() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let complete_state_request = CompleteStateRequestSpec {
            field_mask: vec![FIELD_MASK.to_string()],
        };
        let request_content =
            RequestContentSpec::CompleteStateRequest(complete_state_request.clone());
        assert!(
            tx.request_complete_state(fixtures::REQUEST_ID.to_string(), complete_state_request)
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::Request(RequestSpec {
                request_id: fixtures::REQUEST_ID.to_string(),
                request_content
            })
        )
    }

    #[tokio::test]
    async fn utest_to_server_send_logs_request() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let logs_request = LogsRequestSpec {
            workload_names: vec![WorkloadInstanceNameSpec::new(
                fixtures::AGENT_NAMES[0],
                fixtures::WORKLOAD_NAMES[0],
                "id",
            )],
            follow: true,
            tail: 10,
            since: None,
            until: None,
        };
        let request_content = RequestContentSpec::LogsRequest(logs_request.clone());
        assert!(
            tx.logs_request(fixtures::REQUEST_ID.into(), logs_request.into())
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::Request(RequestSpec {
                request_id: fixtures::REQUEST_ID.to_string(),
                request_content
            })
        )
    }

    #[tokio::test]
    async fn utest_to_server_send_logs_cancel_request() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let request_content = RequestContentSpec::LogsCancelRequest(LogsCancelRequestSpec {});
        assert!(
            tx.logs_cancel_request(fixtures::REQUEST_ID.into())
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::Request(RequestSpec {
                request_id: fixtures::REQUEST_ID.to_string(),
                request_content
            })
        )
    }

    #[tokio::test]
    async fn utest_to_server_send_logs_response() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let logs_response = LogEntriesResponse {
            log_entries: vec![LogEntry {
                workload_name: Some(WorkloadInstanceName {
                    agent_name: fixtures::AGENT_NAMES[0].into(),
                    workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                    id: "id".into(),
                }),
                message: "message".into(),
            }],
        };

        assert!(
            tx.log_entries_response(fixtures::REQUEST_ID.into(), logs_response.clone())
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::LogEntriesResponse(fixtures::REQUEST_ID.to_string(), logs_response)
        );
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_logs_stop_response() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            mpsc::channel(fixtures::TEST_CHANNEL_CAP);

        let response_content = LogsStopResponse {
            workload_name: Some(WorkloadInstanceName {
                agent_name: fixtures::AGENT_NAMES[0].into(),
                workload_name: fixtures::WORKLOAD_NAMES[0].into(),
                id: "id".into(),
            }),
        };

        assert!(
            tx.logs_stop_response(fixtures::REQUEST_ID.into(), response_content.clone())
                .await
                .is_ok()
        );

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::LogsStopResponse(fixtures::REQUEST_ID.to_string(), response_content)
        )
    }
}

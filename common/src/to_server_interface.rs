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

use crate::{
    commands::{self, RequestContent},
    objects::CompleteState,
};
use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc::error::SendError;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ToServer {
    AgentHello(commands::AgentHello),
    AgentLoadStatus(commands::AgentLoadStatus),
    AgentGone(commands::AgentGone),
    Request(commands::Request),
    UpdateWorkloadState(commands::UpdateWorkloadState),
    Stop(commands::Stop),
    Goodbye(commands::Goodbye),
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
    async fn agent_load_status(
        &self,
        agent_resource: commands::AgentLoadStatus,
    ) -> Result<(), ToServerError>;
    async fn agent_gone(&self, agent_name: String) -> Result<(), ToServerError>;
    async fn update_state(
        &self,
        request_id: String,
        state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<(), ToServerError>;
    async fn update_workload_state(
        &self,
        workload_running: Vec<crate::objects::WorkloadState>,
    ) -> Result<(), ToServerError>;
    async fn request_complete_state(
        &self,
        request_id: String,
        request_complete_state: commands::CompleteStateRequest,
    ) -> Result<(), ToServerError>;
    async fn stop(&self) -> Result<(), ToServerError>;
}

pub type ToServerSender = tokio::sync::mpsc::Sender<ToServer>;
pub type ToServerReceiver = tokio::sync::mpsc::Receiver<ToServer>;

#[async_trait]
impl ToServerInterface for ToServerSender {
    async fn agent_hello(&self, agent_name: String) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::AgentHello(commands::AgentHello { agent_name }))
            .await?)
    }

    async fn agent_load_status(
        &self,
        agent_load_status: commands::AgentLoadStatus,
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
        state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::Request(commands::Request {
                request_id,
                request_content: commands::RequestContent::UpdateStateRequest(Box::new(
                    commands::UpdateStateRequest { state, update_mask },
                )),
            }))
            .await?)
    }

    async fn update_workload_state(
        &self,
        workload_running: Vec<crate::objects::WorkloadState>,
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
        request_complete_state: commands::CompleteStateRequest,
    ) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::Request(commands::Request {
                request_id,
                request_content: RequestContent::CompleteStateRequest(
                    commands::CompleteStateRequest {
                        field_mask: request_complete_state.field_mask,
                    },
                ),
            }))
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
    use crate::{
        commands::{self, AgentLoadStatus, RequestContent},
        objects::{
            generate_test_workload_spec, generate_test_workload_state, CpuUsage, ExecutionState,
            FreeMemory,
        },
        test_utils::generate_test_complete_state,
        to_server_interface::{ToServer, ToServerInterface},
    };

    use super::{ToServerReceiver, ToServerSender};

    const TEST_CHANNEL_CAPA: usize = 5;
    const WORKLOAD_NAME: &str = "X";
    const AGENT_NAME: &str = "agent_A";
    const REQUEST_ID: &str = "emkw489ejf89ml";
    const FIELD_MASK: &str = "desiredState.bla_bla";
    const CPU_USAGE: CpuUsage = CpuUsage { cpu_usage: 42 };
    const FREE_MEMORY: FreeMemory = FreeMemory { free_memory: 42 };

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_agent_hello() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        assert!(tx.agent_hello(AGENT_NAME.to_string()).await.is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::AgentHello(commands::AgentHello {
                agent_name: AGENT_NAME.to_string()
            })
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_agent_load_status() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        assert!(tx
            .agent_load_status(AgentLoadStatus {
                agent_name: AGENT_NAME.to_string(),
                cpu_usage: CPU_USAGE.clone(),
                free_memory: FREE_MEMORY.clone(),
            })
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await,
            Some(ToServer::AgentLoadStatus(AgentLoadStatus {
                agent_name: AGENT_NAME.to_string(),
                cpu_usage: CPU_USAGE.clone(),
                free_memory: FREE_MEMORY.clone(),
            }))
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_agent_gone() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        assert!(tx.agent_gone(AGENT_NAME.to_string()).await.is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::AgentGone(commands::AgentGone {
                agent_name: AGENT_NAME.to_string()
            })
        )
    }

    // [utest->swdd~to-server-channel~1]
    #[tokio::test]
    async fn utest_to_server_send_update_state() {
        let (tx, mut rx): (ToServerSender, ToServerReceiver) =
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let workload1 = generate_test_workload_spec();
        let complete_state = generate_test_complete_state(vec![workload1]);
        assert!(tx
            .update_state(
                REQUEST_ID.to_string(),
                complete_state.clone(),
                vec![FIELD_MASK.to_string()]
            )
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::Request(commands::Request {
                request_id: REQUEST_ID.to_string(),
                request_content: commands::RequestContent::UpdateStateRequest(Box::new(
                    commands::UpdateStateRequest {
                        state: complete_state,
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
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let workload_state = generate_test_workload_state(WORKLOAD_NAME, ExecutionState::running());
        assert!(tx
            .update_workload_state(vec![workload_state.clone()])
            .await
            .is_ok());

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
            tokio::sync::mpsc::channel(TEST_CHANNEL_CAPA);

        let complete_state_request = commands::CompleteStateRequest {
            field_mask: vec![FIELD_MASK.to_string()],
        };
        let request_content = RequestContent::CompleteStateRequest(complete_state_request.clone());
        assert!(tx
            .request_complete_state(REQUEST_ID.to_string(), complete_state_request)
            .await
            .is_ok());

        assert_eq!(
            rx.recv().await.unwrap(),
            ToServer::Request(commands::Request {
                request_id: REQUEST_ID.to_string(),
                request_content
            })
        )
    }
}

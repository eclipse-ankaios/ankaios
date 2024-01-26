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

use crate::commands::{self, RequestContent};
use api::proto;
use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc::error::SendError;

// [impl->swdd~to-server-channel~1]
#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ToServer {
    AgentHello(commands::AgentHello),
    AgentGone(commands::AgentGone),
    Request(commands::Request),
    UpdateWorkloadState(commands::UpdateWorkloadState),
    Stop(commands::Stop),
    Goodbye(commands::Goodbye),
}

impl TryFrom<proto::ToServer> for ToServer {
    type Error = String;

    fn try_from(item: proto::ToServer) -> Result<Self, Self::Error> {
        use proto::to_server::ToServerEnum;
        let to_server = item.to_server_enum.ok_or("ToServer is None.".to_string())?;

        Ok(match to_server {
            ToServerEnum::AgentHello(protobuf) => ToServer::AgentHello(protobuf.into()),
            ToServerEnum::UpdateWorkloadState(protobuf) => {
                ToServer::UpdateWorkloadState(protobuf.into())
            }
            ToServerEnum::Request(protobuf) => ToServer::Request(protobuf.try_into()?),
            ToServerEnum::Goodbye(_) => ToServer::Goodbye(commands::Goodbye {}),
        })
    }
}

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

#[async_trait]
pub trait ToServerInterface {
    async fn agent_hello(&self, agent_name: String) -> Result<(), ToServerError>;
    async fn agent_gone(&self, agent_name: String) -> Result<(), ToServerError>;
    async fn update_state(
        &self,
        request_id: String,
        state: commands::CompleteState,
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

    async fn agent_gone(&self, agent_name: String) -> Result<(), ToServerError> {
        Ok(self
            .send(ToServer::AgentGone(commands::AgentGone { agent_name }))
            .await?)
    }

    async fn update_state(
        &self,
        request_id: String,
        state: commands::CompleteState,
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

#[cfg(feature = "test_utils")]
pub fn generate_test_failed_update_workload_state(
    agent_name: &str,
    workload_name: &str,
) -> ToServer {
    ToServer::UpdateWorkloadState(commands::UpdateWorkloadState {
        workload_states: vec![crate::objects::WorkloadState {
            workload_name: workload_name.to_string(),
            agent_name: agent_name.to_string(),
            execution_state: crate::objects::ExecutionState::ExecFailed,
        }],
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use api::proto::{self, to_server::ToServerEnum};

    use crate::{
        commands::{AgentHello, CompleteStateRequest, Request, RequestContent, UpdateStateRequest},
        to_server_interface::ToServer,
    };

    #[test]
    fn utest_convert_proto_to_server_agent_hello() {
        let agent_name = "agent_A".to_string();

        let proto_request = proto::ToServer {
            to_server_enum: Some(ToServerEnum::AgentHello(proto::AgentHello {
                agent_name: agent_name.clone(),
            })),
        };

        let ankaios_command = ToServer::AgentHello(AgentHello { agent_name });

        assert_eq!(ToServer::try_from(proto_request), Ok(ankaios_command));
    }

    #[test]
    fn utest_convert_proto_to_server_update_workload_state() {
        let proto_request = proto::ToServer {
            to_server_enum: Some(ToServerEnum::UpdateWorkloadState(
                proto::UpdateWorkloadState {
                    workload_states: vec![],
                },
            )),
        };

        let ankaios_command = ToServer::UpdateWorkloadState(crate::commands::UpdateWorkloadState {
            workload_states: vec![],
        });

        assert_eq!(ToServer::try_from(proto_request), Ok(ankaios_command));
    }

    #[test]
    fn utest_convert_proto_to_server_update_state() {
        let proto_request = proto::ToServer {
            to_server_enum: Some(ToServerEnum::Request(proto::Request {
                request_id: "request_id".to_owned(),
                request_content: Some(proto::request::RequestContent::UpdateStateRequest(
                    proto::UpdateStateRequest {
                        update_mask: vec!["test_update_mask_field".to_owned()],
                        new_state: Some(proto::CompleteState {
                            current_state: Some(proto::State {
                                workloads: HashMap::from([(
                                    "test_workload".to_owned(),
                                    proto::Workload {
                                        agent: "test_agent".to_owned(),
                                        ..Default::default()
                                    },
                                )]),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                    },
                )),
            })),
        };

        let ankaios_command = ToServer::Request(Request {
            request_id: "request_id".to_owned(),
            request_content: RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                update_mask: vec!["test_update_mask_field".to_owned()],
                state: crate::commands::CompleteState {
                    current_state: crate::objects::State {
                        workloads: HashMap::from([(
                            "test_workload".to_owned(),
                            crate::objects::WorkloadSpec {
                                name: "test_workload".to_owned(),
                                agent: "test_agent".to_owned(),
                                ..Default::default()
                            },
                        )]),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            })),
        });

        assert_eq!(ToServer::try_from(proto_request), Ok(ankaios_command));
    }

    #[test]
    fn utest_convert_proto_to_server_update_state_fails() {
        let proto_request = proto::ToServer {
            to_server_enum: Some(proto::to_server::ToServerEnum::Request(proto::Request {
                request_id: "requeset_id".to_owned(),
                request_content: Some(proto::request::RequestContent::UpdateStateRequest(
                    proto::UpdateStateRequest {
                        update_mask: vec!["test_update_mask_field".to_owned()],
                        new_state: Some(proto::CompleteState {
                            current_state: Some(proto::State {
                                workloads: HashMap::from([(
                                    "test_workload".to_owned(),
                                    proto::Workload {
                                        agent: "test_agent".to_owned(),
                                        dependencies: vec![("other_workload".into(), -1)]
                                            .into_iter()
                                            .collect(),
                                        ..Default::default()
                                    },
                                )]),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                    },
                )),
            })),
        };

        assert!(ToServer::try_from(proto_request).is_err(),);
    }

    #[test]
    fn utest_convert_proto_to_server_request_complete_state() {
        let request_id = "42".to_string();
        let field_mask = vec!["1".to_string()];

        let proto_request = proto::ToServer {
            to_server_enum: Some(proto::to_server::ToServerEnum::Request(proto::Request {
                request_id: request_id.clone(),
                request_content: Some(proto::request::RequestContent::CompleteStateRequest(
                    proto::CompleteStateRequest {
                        field_mask: field_mask.clone(),
                    },
                )),
            })),
        };

        let ankaios_command = ToServer::Request(Request {
            request_id,
            request_content: RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask,
            }),
        });

        assert_eq!(ToServer::try_from(proto_request), Ok(ankaios_command));
    }
}

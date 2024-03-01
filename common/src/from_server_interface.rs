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
use crate::objects::{CompleteState, DeletedWorkload, WorkloadSpec, WorkloadState};
use api::proto;
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

// [impl->swdd~from-server-channel~1]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromServer {
    UpdateWorkload(commands::UpdateWorkload),
    UpdateWorkloadState(commands::UpdateWorkloadState),
    Response(commands::Response),
    Stop(commands::Stop),
}

impl TryFrom<FromServer> for proto::FromServer {
    type Error = &'static str;

    fn try_from(item: FromServer) -> Result<Self, Self::Error> {
        match item {
            FromServer::UpdateWorkload(ankaios) => Ok(proto::FromServer {
                from_server_enum: Some(proto::from_server::FromServerEnum::UpdateWorkload(
                    proto::UpdateWorkload {
                        added_workloads: ankaios
                            .added_workloads
                            .into_iter()
                            .map(|x| x.into())
                            .collect(),
                        deleted_workloads: ankaios
                            .deleted_workloads
                            .into_iter()
                            .map(|x| x.into())
                            .collect(),
                    },
                )),
            }),
            FromServer::UpdateWorkloadState(ankaios) => Ok(proto::FromServer {
                from_server_enum: Some(proto::from_server::FromServerEnum::UpdateWorkloadState(
                    proto::UpdateWorkloadState {
                        workload_states: ankaios
                            .workload_states
                            .iter()
                            .map(|x| x.to_owned().into())
                            .collect(),
                    },
                )),
            }),
            FromServer::Response(ankaios) => Ok(proto::FromServer {
                from_server_enum: Some(proto::from_server::FromServerEnum::Response(
                    proto::Response {
                        request_id: ankaios.request_id,
                        response_content: Some(ankaios.response_content.into()),
                    },
                )),
            }),
            FromServer::Stop(_) => Err("Stop command not implemented in proto"),
        }
    }
}

#[async_trait]
pub trait FromServerInterface {
    async fn update_workload(
        &self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn update_workload_state(
        &self,
        workload_running: Vec<WorkloadState>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn response(&self, response: commands::Response) -> Result<(), FromServerInterfaceError>;
    async fn complete_state(
        &self,
        request_id: String,
        complete_state: CompleteState,
    ) -> Result<(), FromServerInterfaceError>;
    async fn success(&self, request_id: String) -> Result<(), FromServerInterfaceError>;
    async fn update_state_success(
        &self,
        request_id: String,
        added_workloads: Vec<String>,
        deleted_workloads: Vec<String>,
    ) -> Result<(), FromServerInterfaceError>;
    async fn error(
        &self,
        request_id: String,
        error: commands::Error,
    ) -> Result<(), FromServerInterfaceError>;
    async fn stop(&self) -> Result<(), FromServerInterfaceError>;
}

pub type FromServerSender = tokio::sync::mpsc::Sender<FromServer>;
pub type FromServerReceiver = tokio::sync::mpsc::Receiver<FromServer>;

#[async_trait]
impl FromServerInterface for FromServerSender {
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

    async fn response(&self, response: commands::Response) -> Result<(), FromServerInterfaceError> {
        Ok(self.send(FromServer::Response(response)).await?)
    }

    async fn complete_state(
        &self,
        request_id: String,
        complete_state: CompleteState,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::Response(commands::Response {
                request_id,
                response_content: commands::ResponseContent::CompleteState(Box::new(
                    complete_state,
                )),
            }))
            .await?)
    }

    async fn success(&self, request_id: String) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::Response(commands::Response {
                request_id,
                response_content: commands::ResponseContent::Success,
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
            .send(FromServer::Response(commands::Response {
                request_id,
                response_content: commands::ResponseContent::UpdateStateSuccess(
                    commands::UpdateStateSuccess {
                        added_workloads,
                        deleted_workloads,
                    },
                ),
            }))
            .await?)
    }

    async fn error(
        &self,
        request_id: String,
        error: commands::Error,
    ) -> Result<(), FromServerInterfaceError> {
        Ok(self
            .send(FromServer::Response(commands::Response {
                request_id,
                response_content: commands::ResponseContent::Error(error),
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
    use crate::{
        commands,
        from_server_interface::FromServer,
        objects::{self, WorkloadInstanceName, WorkloadSpec},
        test_utils::{generate_test_deleted_workload, generate_test_proto_deleted_workload},
    };

    use api::proto::{self, from_server::FromServerEnum, AddedWorkload};

    #[test]
    fn utest_convert_from_server_to_proto_update_workload() {
        let instance_name = WorkloadInstanceName::builder()
            .workload_name("test_workload")
            .build();
        let test_ex_com = FromServer::UpdateWorkload(commands::UpdateWorkload {
            added_workloads: vec![WorkloadSpec {
                instance_name,
                runtime: "tes_runtime".to_owned(),
                ..Default::default()
            }],
            deleted_workloads: vec![generate_test_deleted_workload(
                "agent".to_string(),
                "workload X".to_string(),
            )],
        });
        let expected_ex_com = Ok(proto::FromServer {
            from_server_enum: Some(FromServerEnum::UpdateWorkload(proto::UpdateWorkload {
                added_workloads: vec![AddedWorkload {
                    instance_name: Some(proto::WorkloadInstanceName {
                        workload_name: "test_workload".to_owned(),
                        ..Default::default()
                    }),
                    runtime: "tes_runtime".to_owned(),
                    ..Default::default()
                }],
                deleted_workloads: vec![generate_test_proto_deleted_workload()],
            })),
        });

        assert_eq!(proto::FromServer::try_from(test_ex_com), expected_ex_com);
    }

    #[test]
    fn utest_convert_from_server_to_proto_update_workload_state() {
        let workload_state = crate::objects::generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            crate::objects::ExecutionState::running(),
        );

        let test_ex_com = FromServer::UpdateWorkloadState(commands::UpdateWorkloadState {
            workload_states: vec![workload_state.clone()],
        });
        let expected_ex_com = Ok(proto::FromServer {
            from_server_enum: Some(FromServerEnum::UpdateWorkloadState(
                proto::UpdateWorkloadState {
                    workload_states: vec![workload_state.into()],
                },
            )),
        });

        assert_eq!(proto::FromServer::try_from(test_ex_com), expected_ex_com);
    }

    #[test]
    fn utest_convert_from_server_to_proto_complete_state() {
        let test_ex_com = FromServer::Response(commands::Response {
            request_id: "req_id".to_owned(),
            response_content: commands::ResponseContent::CompleteState(Box::default()),
        });

        let expected_ex_com = Ok(proto::FromServer {
            from_server_enum: Some(proto::from_server::FromServerEnum::Response(
                proto::Response {
                    request_id: "req_id".to_owned(),
                    response_content: Some(proto::response::ResponseContent::CompleteState(
                        proto::CompleteState {
                            desired_state: Some(api::proto::State {
                                format_version: "v0.1".into(),
                                ..Default::default()
                            }),
                            startup_state: Some(api::proto::State {
                                format_version: "v0.1".into(),
                                ..Default::default()
                            }),
                            workload_states: vec![],
                        },
                    )),
                },
            )),
        });

        assert_eq!(proto::FromServer::try_from(test_ex_com), expected_ex_com);
    }
}

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
use crate::objects::{DeletedWorkload, WorkloadSpec, WorkloadState};
use api::proto;
use async_trait::async_trait;
use std::fmt;
use tokio::sync::mpsc::error::SendError;
#[derive(Debug)]
pub struct ExecutionCommandError(String);

impl fmt::Display for ExecutionCommandError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ExecutionCommandError: '{}'", self.0)
    }
}

impl From<SendError<ExecutionCommand>> for ExecutionCommandError {
    fn from(error: SendError<ExecutionCommand>) -> Self {
        ExecutionCommandError(error.to_string())
    }
}

// [impl->swdd~execution-command-channel~1]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionCommand {
    UpdateWorkload(commands::UpdateWorkload),
    UpdateWorkloadState(commands::UpdateWorkloadState),
    CompleteState(Box<commands::CompleteState>), // Boxed to avoid clippy warning "large size difference between variants"
    Stop(commands::Stop),
}

impl TryFrom<ExecutionCommand> for proto::ExecutionRequest {
    type Error = &'static str;

    fn try_from(item: ExecutionCommand) -> Result<Self, Self::Error> {
        match item {
            ExecutionCommand::UpdateWorkload(ankaios) => Ok(proto::ExecutionRequest {
                execution_request_enum: Some(
                    proto::execution_request::ExecutionRequestEnum::UpdateWorkload(
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
                    ),
                ),
            }),
            ExecutionCommand::UpdateWorkloadState(ankaios) => Ok(proto::ExecutionRequest {
                execution_request_enum: Some(
                    proto::execution_request::ExecutionRequestEnum::UpdateWorkloadState(
                        proto::UpdateWorkloadState {
                            workload_states: ankaios
                                .workload_states
                                .iter()
                                .map(|x| x.to_owned().into())
                                .collect(),
                        },
                    ),
                ),
            }),
            ExecutionCommand::CompleteState(ankaios) => Ok(proto::ExecutionRequest {
                execution_request_enum: Some(
                    proto::execution_request::ExecutionRequestEnum::CompleteState(
                        proto::CompleteState {
                            request_id: ankaios.request_id,
                            startup_state: Some(ankaios.startup_state.into()),
                            current_state: Some(ankaios.current_state.into()),
                            workload_states: ankaios
                                .workload_states
                                .iter()
                                .map(|x| x.to_owned().into())
                                .collect(),
                        },
                    ),
                ),
            }),
            ExecutionCommand::Stop(_) => Err("Stop command not implemented in proto"),
        }
    }
}

#[async_trait]
pub trait ExecutionInterface {
    async fn update_workload(
        &self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Result<(), ExecutionCommandError>;
    async fn update_workload_state(
        &self,
        workload_running: Vec<WorkloadState>,
    ) -> Result<(), ExecutionCommandError>;
    async fn complete_state(
        &self,
        complete_state: commands::CompleteState,
    ) -> Result<(), ExecutionCommandError>;
    async fn stop(&self) -> Result<(), ExecutionCommandError>;
}

pub type ExecutionSender = tokio::sync::mpsc::Sender<ExecutionCommand>;
pub type ExecutionReceiver = tokio::sync::mpsc::Receiver<ExecutionCommand>;

#[async_trait]
impl ExecutionInterface for ExecutionSender {
    async fn update_workload(
        &self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
    ) -> Result<(), ExecutionCommandError> {
        Ok(self
            .send(ExecutionCommand::UpdateWorkload(commands::UpdateWorkload {
                added_workloads,
                deleted_workloads,
            }))
            .await?)
    }

    async fn update_workload_state(
        &self,
        workload_states: Vec<WorkloadState>,
    ) -> Result<(), ExecutionCommandError> {
        Ok(self
            .send(ExecutionCommand::UpdateWorkloadState(
                commands::UpdateWorkloadState { workload_states },
            ))
            .await?)
    }

    async fn complete_state(
        &self,
        complete_state: commands::CompleteState,
    ) -> Result<(), ExecutionCommandError> {
        Ok(self
            .send(ExecutionCommand::CompleteState(Box::new(complete_state)))
            .await?)
    }

    async fn stop(&self) -> Result<(), ExecutionCommandError> {
        Ok(self.send(ExecutionCommand::Stop(commands::Stop {})).await?)
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
        execution_interface::ExecutionCommand,
        objects::{ExecutionState, RuntimeWorkload, WorkloadSpec, WorkloadState},
        test_utils::{generate_test_deleted_workload, generate_test_proto_deleted_workload},
    };

    use api::proto::{
        self, execution_request::ExecutionRequestEnum, AddedWorkload, ExecutionRequest,
    };

    #[test]
    fn utest_convert_execution_command_to_proto_update_workload() {
        let test_ex_com = ExecutionCommand::UpdateWorkload(commands::UpdateWorkload {
            added_workloads: vec![WorkloadSpec {
                workload: RuntimeWorkload {
                    name: "test_workload".to_owned(),
                    ..Default::default()
                },
                runtime: "tes_runtime".to_owned(),
                ..Default::default()
            }],
            deleted_workloads: vec![generate_test_deleted_workload(
                "agent X".to_string(),
                "workload X".to_string(),
            )],
        });
        let expected_ex_com = Ok(ExecutionRequest {
            execution_request_enum: Some(ExecutionRequestEnum::UpdateWorkload(
                proto::UpdateWorkload {
                    added_workloads: vec![AddedWorkload {
                        name: "test_workload".to_owned(),
                        runtime: "tes_runtime".to_owned(),
                        ..Default::default()
                    }],
                    deleted_workloads: vec![generate_test_proto_deleted_workload()],
                },
            )),
        });

        assert_eq!(
            proto::ExecutionRequest::try_from(test_ex_com),
            expected_ex_com
        );
    }

    #[test]
    fn utest_convert_execution_command_to_proto_update_workload_state() {
        let test_ex_com = ExecutionCommand::UpdateWorkloadState(commands::UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                agent_name: "test_agent".to_owned(),
                workload_name: "test_workload".to_owned(),
                execution_state: ExecutionState::ExecRunning,
            }],
        });
        let expected_ex_com = Ok(ExecutionRequest {
            execution_request_enum: Some(ExecutionRequestEnum::UpdateWorkloadState(
                proto::UpdateWorkloadState {
                    workload_states: vec![api::proto::WorkloadState {
                        agent_name: "test_agent".to_owned(),
                        workload_name: "test_workload".to_owned(),
                        execution_state: ExecutionState::ExecRunning as i32,
                    }],
                },
            )),
        });

        assert_eq!(
            proto::ExecutionRequest::try_from(test_ex_com),
            expected_ex_com
        );
    }

    #[test]
    fn utest_convert_execution_command_to_proto_complete_state() {
        let test_ex_com = ExecutionCommand::CompleteState(Box::new(commands::CompleteState {
            request_id: "req_id".to_owned(),
            ..Default::default()
        }));
        let expected_ex_com = Ok(ExecutionRequest {
            execution_request_enum: Some(ExecutionRequestEnum::CompleteState(
                proto::CompleteState {
                    request_id: "req_id".to_owned(),
                    current_state: Some(api::proto::State::default()),
                    startup_state: Some(api::proto::State::default()),
                    workload_states: vec![],
                },
            )),
        });

        assert_eq!(
            proto::ExecutionRequest::try_from(test_ex_com),
            expected_ex_com
        );
    }
}

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
use api::proto;
use async_trait::async_trait;

// [impl->swdd~state-change-command-channel~1]
#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StateChangeCommand {
    AgentHello(commands::AgentHello),
    AgentGone(commands::AgentGone),
    UpdateState(commands::UpdateStateRequest),
    UpdateWorkloadState(commands::UpdateWorkloadState),
    RequestCompleteState(commands::RequestCompleteState),
    Stop(commands::Stop),
}

impl TryFrom<proto::StateChangeRequest> for StateChangeCommand {
    type Error = String;

    fn try_from(item: proto::StateChangeRequest) -> Result<Self, Self::Error> {
        use proto::state_change_request::StateChangeRequestEnum;

        Ok(match item.state_change_request_enum.unwrap() {
            StateChangeRequestEnum::AgentHello(protobuf) => {
                StateChangeCommand::AgentHello(protobuf.into())
            }
            StateChangeRequestEnum::UpdateWorkloadState(protobuf) => {
                StateChangeCommand::UpdateWorkloadState(protobuf.into())
            }
            StateChangeRequestEnum::UpdateState(protobuf) => {
                StateChangeCommand::UpdateState(protobuf.try_into()?)
            }
            StateChangeRequestEnum::RequestCompleteState(protobuf) => {
                StateChangeCommand::RequestCompleteState(protobuf.into())
            }
        })
    }
}

#[async_trait]
pub trait StateChangeInterface {
    async fn agent_hello(&self, agent_name: String);
    async fn agent_gone(&self, agent_name: String);
    async fn update_state(&self, state: commands::CompleteState, update_mask: Vec<String>);
    async fn update_workload_state(&self, workload_running: Vec<crate::objects::WorkloadState>);
    async fn request_complete_state(&self, request_complete_state: commands::RequestCompleteState);
    async fn stop(&self);
}

pub type StateChangeSender = tokio::sync::mpsc::Sender<StateChangeCommand>;
pub type StateChangeReceiver = tokio::sync::mpsc::Receiver<StateChangeCommand>;

#[async_trait]
impl StateChangeInterface for StateChangeSender {
    async fn agent_hello(&self, agent_name: String) {
        self.send(StateChangeCommand::AgentHello(commands::AgentHello {
            agent_name,
        }))
        .await
        .unwrap();
    }

    async fn agent_gone(&self, agent_name: String) {
        self.send(StateChangeCommand::AgentGone(commands::AgentGone {
            agent_name,
        }))
        .await
        .unwrap();
    }

    async fn update_state(&self, state: commands::CompleteState, update_mask: Vec<String>) {
        self.send(StateChangeCommand::UpdateState(
            commands::UpdateStateRequest { state, update_mask },
        ))
        .await
        .unwrap();
    }

    async fn update_workload_state(&self, workload_running: Vec<crate::objects::WorkloadState>) {
        self.send(StateChangeCommand::UpdateWorkloadState(
            commands::UpdateWorkloadState {
                workload_states: workload_running,
            },
        ))
        .await
        .unwrap();
    }

    async fn request_complete_state(&self, request_complete_state: commands::RequestCompleteState) {
        self.send(StateChangeCommand::RequestCompleteState(
            commands::RequestCompleteState {
                request_id: request_complete_state.request_id,
                field_mask: request_complete_state.field_mask,
            },
        ))
        .await
        .unwrap();
    }

    async fn stop(&self) {
        self.send(StateChangeCommand::Stop(commands::Stop {}))
            .await
            .unwrap();
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
) -> StateChangeCommand {
    StateChangeCommand::UpdateWorkloadState(commands::UpdateWorkloadState {
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

    use api::proto::{self, state_change_request::StateChangeRequestEnum};

    use crate::{
        commands::{AgentHello, RequestCompleteState},
        state_change_interface::StateChangeCommand,
    };

    #[test]
    fn utest_convert_proto_state_change_request_agent_hello() {
        let agent_name = "agent_A".to_string();

        let proto_request = proto::StateChangeRequest {
            state_change_request_enum: Some(StateChangeRequestEnum::AgentHello(
                proto::AgentHello {
                    agent_name: agent_name.clone(),
                },
            )),
        };

        let ankaios_command = StateChangeCommand::AgentHello(AgentHello { agent_name });

        assert_eq!(
            StateChangeCommand::try_from(proto_request),
            Ok(ankaios_command)
        );
    }

    #[test]
    fn utest_convert_proto_state_change_request_update_workload_state() {
        let proto_request = proto::StateChangeRequest {
            state_change_request_enum: Some(StateChangeRequestEnum::UpdateWorkloadState(
                proto::UpdateWorkloadState {
                    workload_states: vec![],
                },
            )),
        };

        let ankaios_command =
            StateChangeCommand::UpdateWorkloadState(crate::commands::UpdateWorkloadState {
                workload_states: vec![],
            });

        assert_eq!(
            StateChangeCommand::try_from(proto_request),
            Ok(ankaios_command)
        );
    }

    #[test]
    fn utest_convert_proto_state_change_request_update_state() {
        let proto_request = proto::StateChangeRequest {
            state_change_request_enum: Some(StateChangeRequestEnum::UpdateState(
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
        };

        let ankaios_command =
            StateChangeCommand::UpdateState(crate::commands::UpdateStateRequest {
                update_mask: vec!["test_update_mask_field".to_owned()],
                state: crate::commands::CompleteState {
                    current_state: crate::objects::State {
                        workloads: HashMap::from([(
                            "test_workload".to_owned(),
                            crate::objects::WorkloadSpec {
                                agent: "test_agent".to_owned(),
                                workload: crate::objects::RuntimeWorkload {
                                    name: "test_workload".to_owned(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        )]),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            });

        assert_eq!(
            StateChangeCommand::try_from(proto_request),
            Ok(ankaios_command)
        );
    }

    #[test]
    fn utest_convert_proto_state_change_request_update_state_fails() {
        let proto_request = proto::StateChangeRequest {
            state_change_request_enum: Some(StateChangeRequestEnum::UpdateState(
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
        };

        assert!(StateChangeCommand::try_from(proto_request).is_err(),);
    }

    #[test]
    fn utest_convert_proto_state_change_request_request_complete_state() {
        let request_id = "42".to_string();
        let field_mask = vec!["1".to_string()];

        let proto_request = proto::StateChangeRequest {
            state_change_request_enum: Some(StateChangeRequestEnum::RequestCompleteState(
                proto::RequestCompleteState {
                    request_id: request_id.clone(),
                    field_mask: field_mask.clone(),
                },
            )),
        };

        let ankaios_command = StateChangeCommand::RequestCompleteState(RequestCompleteState {
            request_id,
            field_mask,
        });

        assert_eq!(
            StateChangeCommand::try_from(proto_request),
            Ok(ankaios_command)
        );
    }
}

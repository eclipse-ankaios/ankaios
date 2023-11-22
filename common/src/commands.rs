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

use crate::objects::{DeletedWorkload, State, WorkloadSpec, WorkloadState};
use api::proto;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentHello {
    pub agent_name: String,
}

impl From<proto::AgentHello> for AgentHello {
    fn from(item: proto::AgentHello) -> Self {
        AgentHello {
            agent_name: item.agent_name,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentGone {
    pub agent_name: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UpdateStateRequest {
    pub state: CompleteState,
    pub update_mask: Vec<String>,
}

impl TryFrom<proto::UpdateStateRequest> for UpdateStateRequest {
    type Error = String;

    fn try_from(item: proto::UpdateStateRequest) -> Result<Self, Self::Error> {
        Ok(UpdateStateRequest {
            state: item.new_state.unwrap_or_default().try_into()?,
            update_mask: item.update_mask,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateWorkloadState {
    pub workload_states: Vec<crate::objects::WorkloadState>,
}

impl From<UpdateWorkloadState> for proto::UpdateWorkloadState {
    fn from(item: UpdateWorkloadState) -> Self {
        proto::UpdateWorkloadState {
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl From<proto::UpdateWorkloadState> for UpdateWorkloadState {
    fn from(item: proto::UpdateWorkloadState) -> Self {
        UpdateWorkloadState {
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCompleteState {
    pub request_id: String,
    pub field_mask: Vec<String>,
}

impl RequestCompleteState {
    pub fn prefix_request_id(&mut self, prefix: &str) {
        self.request_id = format!("{}{}", prefix, self.request_id);
    }
}

impl From<RequestCompleteState> for proto::RequestCompleteState {
    fn from(item: RequestCompleteState) -> Self {
        proto::RequestCompleteState {
            request_id: item.request_id,
            field_mask: item.field_mask,
        }
    }
}

impl From<proto::RequestCompleteState> for RequestCompleteState {
    fn from(item: proto::RequestCompleteState) -> Self {
        RequestCompleteState {
            request_id: item.request_id,
            field_mask: item.field_mask,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateWorkload {
    pub added_workloads: Vec<WorkloadSpec>,
    pub deleted_workloads: Vec<DeletedWorkload>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct CompleteState {
    pub request_id: String,
    pub startup_state: State,
    pub current_state: State,
    pub workload_states: Vec<WorkloadState>,
}

impl From<CompleteState> for proto::CompleteState {
    fn from(item: CompleteState) -> proto::CompleteState {
        proto::CompleteState {
            request_id: item.request_id,
            startup_state: Some(proto::State::from(item.startup_state)),
            current_state: Some(proto::State::from(item.current_state)),
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl TryFrom<proto::CompleteState> for CompleteState {
    type Error = String;

    fn try_from(item: proto::CompleteState) -> Result<Self, Self::Error> {
        Ok(CompleteState {
            request_id: item.request_id,
            startup_state: item.startup_state.unwrap_or_default().try_into()?,
            current_state: item.current_state.unwrap_or_default().try_into()?,
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Goodbye {}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Stop {}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use api::proto;

    use crate::{
        commands::{RequestCompleteState, UpdateWorkloadState},
        objects::{ExecutionState, WorkloadState},
    };

    #[test]
    fn utest_converts_to_proto_update_workload_state() {
        let ankaios_update_wl_state = UpdateWorkloadState {
            workload_states: vec![WorkloadState {
                workload_name: "john".to_string(),
                agent_name: "doe".to_string(),
                execution_state: ExecutionState::ExecRunning,
            }],
        };

        let proto_update_wl_state = proto::UpdateWorkloadState {
            workload_states: vec![proto::WorkloadState {
                workload_name: "john".to_string(),
                agent_name: "doe".to_string(),
                execution_state: proto::ExecutionState::ExecRunning.into(),
            }],
        };

        assert_eq!(
            proto::UpdateWorkloadState::from(ankaios_update_wl_state),
            proto_update_wl_state
        );
    }

    #[test]
    fn utest_converts_to_proto_request_complete_state() {
        let ankaios_request_complete_state = RequestCompleteState {
            request_id: "42".to_string(),
            field_mask: vec!["1".to_string(), "2".to_string()],
        };

        let proto_request_complete_state = proto::RequestCompleteState {
            request_id: "42".to_string(),
            field_mask: vec!["1".to_string(), "2".to_string()],
        };

        assert_eq!(
            proto::RequestCompleteState::from(ankaios_request_complete_state),
            proto_request_complete_state
        );
    }

    #[test]
    fn utest_request_complete_state_prefix_request_id() {
        let mut ankaios_request_complete_state = RequestCompleteState {
            request_id: "42".to_string(),
            field_mask: vec!["1".to_string(), "2".to_string()],
        };

        ankaios_request_complete_state.prefix_request_id("prefix@");

        assert_eq!("prefix@42", ankaios_request_complete_state.request_id);
    }
}

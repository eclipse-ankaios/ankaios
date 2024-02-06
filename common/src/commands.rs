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

use std::fmt::Display;

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

impl From<UpdateStateRequest> for proto::UpdateStateRequest {
    fn from(value: UpdateStateRequest) -> Self {
        Self {
            new_state: Some(value.state.into()),
            update_mask: value.update_mask,
        }
    }
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
pub struct Request {
    pub request_id: String,
    pub request_content: RequestContent,
}

impl From<Request> for proto::Request {
    fn from(value: Request) -> Self {
        Self {
            request_id: value.request_id,
            request_content: Some(value.request_content.into()),
        }
    }
}

impl Request {
    pub fn prefix_request_id(&mut self, prefix: &str) {
        self.request_id = format!("{}{}", prefix, self.request_id);
    }
}

impl TryFrom<proto::Request> for Request {
    type Error = String;
    fn try_from(value: proto::Request) -> Result<Request, Self::Error> {
        Ok(Request {
            request_id: value.request_id,
            request_content: value
                .request_content
                .ok_or_else(|| "Received Request without content".to_string())?
                .try_into()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestContent {
    CompleteStateRequest(CompleteStateRequest),
    UpdateStateRequest(Box<UpdateStateRequest>),
}

impl From<RequestContent> for proto::request::RequestContent {
    fn from(value: RequestContent) -> Self {
        match value {
            RequestContent::CompleteStateRequest(content) => {
                proto::request::RequestContent::CompleteStateRequest(content.into())
            }
            RequestContent::UpdateStateRequest(content) => {
                proto::request::RequestContent::UpdateStateRequest((*content).into())
            }
        }
    }
}

impl TryFrom<proto::request::RequestContent> for RequestContent {
    type Error = String;
    fn try_from(value: proto::request::RequestContent) -> Result<Self, Self::Error> {
        Ok(match value {
            proto::request::RequestContent::UpdateStateRequest(value) => {
                RequestContent::UpdateStateRequest(Box::new(value.try_into()?))
            }
            proto::request::RequestContent::CompleteStateRequest(value) => {
                RequestContent::CompleteStateRequest(value.into())
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteStateRequest {
    pub field_mask: Vec<String>,
}

impl From<CompleteStateRequest> for proto::CompleteStateRequest {
    fn from(item: CompleteStateRequest) -> Self {
        proto::CompleteStateRequest {
            field_mask: item.field_mask,
        }
    }
}

impl From<proto::CompleteStateRequest> for CompleteStateRequest {
    fn from(item: proto::CompleteStateRequest) -> Self {
        CompleteStateRequest {
            field_mask: item.field_mask,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateWorkload {
    pub added_workloads: Vec<WorkloadSpec>,
    pub deleted_workloads: Vec<DeletedWorkload>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub request_id: String,
    pub response_content: ResponseContent,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResponseContent {
    Success,
    Error(Error),
    CompleteState(Box<CompleteState>),
}

impl From<ResponseContent> for proto::response::ResponseContent {
    fn from(value: ResponseContent) -> Self {
        match value {
            ResponseContent::Success => {
                proto::response::ResponseContent::Success(proto::Success {})
            }

            ResponseContent::Error(message) => {
                proto::response::ResponseContent::Error(proto::Error {
                    message: message.message,
                })
            }
            ResponseContent::CompleteState(complete_state) => {
                proto::response::ResponseContent::CompleteState((*complete_state).into())
            }
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Error {
    pub message: String,
}

impl From<proto::Error> for Error {
    fn from(value: proto::Error) -> Self {
        Self {
            message: value.message,
        }
    }
}

impl From<Error> for proto::Error {
    fn from(value: Error) -> Self {
        proto::Error {
            message: value.message,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ApiVersion {
    pub major: u32,
    pub minor: u32,
}

impl Default for ApiVersion {
    fn default() -> Self {
        Self { major: 1, minor: 0 }
    }
}

impl From<ApiVersion> for proto::ApiVersion {
    fn from(item: ApiVersion) -> proto::ApiVersion {
        proto::ApiVersion {
            major: item.major,
            minor: item.minor,
        }
    }
}

impl TryFrom<proto::ApiVersion> for ApiVersion {
    type Error = String;

    fn try_from(item: proto::ApiVersion) -> Result<Self, Self::Error> {
        Ok(ApiVersion {
            major: item.major,
            minor: item.minor,
        })
    }
}

impl Display for ApiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}.{}'", self.major, self.minor)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct CompleteState {
    pub format_version: ApiVersion,
    pub startup_state: State,
    pub desired_state: State,
    pub workload_states: Vec<WorkloadState>,
}

impl From<CompleteState> for proto::CompleteState {
    fn from(item: CompleteState) -> proto::CompleteState {
        proto::CompleteState {
            format_version: Some(proto::ApiVersion::from(item.format_version)),
            startup_state: Some(proto::State::from(item.startup_state)),
            desired_state: Some(proto::State::from(item.desired_state)),
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl TryFrom<proto::CompleteState> for CompleteState {
    type Error = String;

    fn try_from(item: proto::CompleteState) -> Result<Self, Self::Error> {
        Ok(CompleteState {
            format_version: item.format_version.unwrap_or_default().try_into()?,
            startup_state: item.startup_state.unwrap_or_default().try_into()?,
            desired_state: item.desired_state.unwrap_or_default().try_into()?,
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        })
    }
}

impl CompleteState {
    pub fn is_compatible_format(format_version: &ApiVersion) -> bool {
        format_version.major == ApiVersion::default().major
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
        commands::{CompleteStateRequest, Request, RequestContent, UpdateWorkloadState},
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
        let ankaios_request_complete_state = Request {
            request_id: "42".to_string(),
            request_content: RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec!["1".to_string(), "2".to_string()],
            }),
        };

        let proto_request_complete_state = proto::Request {
            request_id: "42".to_string(),
            request_content: Some(proto::request::RequestContent::CompleteStateRequest(
                proto::CompleteStateRequest {
                    field_mask: vec!["1".to_string(), "2".to_string()],
                },
            )),
        };

        assert_eq!(
            proto::Request::from(ankaios_request_complete_state),
            proto_request_complete_state
        );
    }

    #[test]
    fn utest_request_complete_state_prefix_request_id() {
        let mut ankaios_request_complete_state = Request {
            request_id: "42".to_string(),
            request_content: RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec!["1".to_string(), "2".to_string()],
            }),
        };

        ankaios_request_complete_state.prefix_request_id("prefix@");

        assert_eq!("prefix@42", ankaios_request_complete_state.request_id);
    }
}

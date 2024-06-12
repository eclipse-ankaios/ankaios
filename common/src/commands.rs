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

use crate::objects::{CompleteState, DeletedWorkload, WorkloadSpec};
use api::ank_base;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentHello {
    pub agent_name: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentGone {
    pub agent_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateWorkloadState {
    pub workload_states: Vec<crate::objects::WorkloadState>,
}



#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub request_id: String,
    pub request_content: RequestContent,
}

impl From<Request> for ank_base::Request {
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

impl TryFrom<ank_base::Request> for Request {
    type Error = String;
    fn try_from(value: ank_base::Request) -> Result<Request, Self::Error> {
        Ok(Request {
            request_id: value.request_id,
            request_content: value
                .request_content
                .ok_or_else(|| "Request has no content".to_string())?
                .try_into()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestContent {
    CompleteStateRequest(CompleteStateRequest),
    UpdateStateRequest(Box<UpdateStateRequest>),
}

impl From<RequestContent> for ank_base::request::RequestContent {
    fn from(value: RequestContent) -> Self {
        match value {
            RequestContent::CompleteStateRequest(content) => {
                ank_base::request::RequestContent::CompleteStateRequest(content.into())
            }
            RequestContent::UpdateStateRequest(content) => {
                ank_base::request::RequestContent::UpdateStateRequest((*content).into())
            }
        }
    }
}

impl TryFrom<ank_base::request::RequestContent> for RequestContent {
    type Error = String;
    fn try_from(value: ank_base::request::RequestContent) -> Result<Self, Self::Error> {
        Ok(match value {
            ank_base::request::RequestContent::UpdateStateRequest(value) => {
                RequestContent::UpdateStateRequest(Box::new(value.try_into()?))
            }
            ank_base::request::RequestContent::CompleteStateRequest(value) => {
                RequestContent::CompleteStateRequest(value.into())
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteStateRequest {
    pub field_mask: Vec<String>,
}

impl From<CompleteStateRequest> for ank_base::CompleteStateRequest {
    fn from(item: CompleteStateRequest) -> Self {
        ank_base::CompleteStateRequest {
            field_mask: item.field_mask,
        }
    }
}

impl From<ank_base::CompleteStateRequest> for CompleteStateRequest {
    fn from(item: ank_base::CompleteStateRequest) -> Self {
        CompleteStateRequest {
            field_mask: item.field_mask,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UpdateStateRequest {
    pub state: CompleteState,
    pub update_mask: Vec<String>,
}

impl From<UpdateStateRequest> for ank_base::UpdateStateRequest {
    fn from(value: UpdateStateRequest) -> Self {
        Self {
            new_state: Some(value.state.into()),
            update_mask: value.update_mask,
        }
    }
}

impl TryFrom<ank_base::UpdateStateRequest> for UpdateStateRequest {
    type Error = String;

    fn try_from(item: ank_base::UpdateStateRequest) -> Result<Self, Self::Error> {
        Ok(UpdateStateRequest {
            state: item.new_state.unwrap_or_default().try_into()?,
            update_mask: item.update_mask,
        })
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

impl From<Response> for ank_base::Response {
    fn from(value: Response) -> Self {
        Self {
            request_id: value.request_id,
            response_content: Some(value.response_content.into()),
        }
    }
}

impl TryFrom<ank_base::Response> for Response {
    type Error = String;

    fn try_from(value: ank_base::Response) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: value.request_id,
            response_content: value
                .response_content
                .ok_or_else(|| "Response has no content".to_string())?
                .try_into()?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResponseContent {
    Error(Error),
    CompleteState(Box<CompleteState>),
    UpdateStateSuccess(UpdateStateSuccess),
}

impl From<ResponseContent> for ank_base::response::ResponseContent {
    fn from(value: ResponseContent) -> Self {
        match value {
            ResponseContent::Error(error) => {
                ank_base::response::ResponseContent::Error(error.into())
            }
            ResponseContent::CompleteState(complete_state) => {
                ank_base::response::ResponseContent::CompleteState((*complete_state).into())
            }
            ResponseContent::UpdateStateSuccess(update_state_success) => {
                ank_base::response::ResponseContent::UpdateStateSuccess(update_state_success.into())
            }
        }
    }
}

impl TryFrom<ank_base::response::ResponseContent> for ResponseContent {
    type Error = String;

    fn try_from(value: ank_base::response::ResponseContent) -> Result<Self, String> {
        match value {
            ank_base::response::ResponseContent::Error(error) => {
                Ok(ResponseContent::Error(error.into()))
            }
            ank_base::response::ResponseContent::CompleteState(complete_state) => Ok(
                ResponseContent::CompleteState(Box::new(complete_state.try_into()?)),
            ),
            ank_base::response::ResponseContent::UpdateStateSuccess(update_state_success) => Ok(
                ResponseContent::UpdateStateSuccess(update_state_success.into()),
            ),
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Error {
    pub message: String,
}

impl From<ank_base::Error> for Error {
    fn from(value: ank_base::Error) -> Self {
        Self {
            message: value.message,
        }
    }
}

impl From<Error> for ank_base::Error {
    fn from(value: Error) -> Self {
        ank_base::Error {
            message: value.message,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateStateSuccess {
    pub added_workloads: Vec<String>,
    pub deleted_workloads: Vec<String>,
}

impl From<UpdateStateSuccess> for ank_base::UpdateStateSuccess {
    fn from(value: UpdateStateSuccess) -> Self {
        Self {
            added_workloads: value.added_workloads,
            deleted_workloads: value.deleted_workloads,
        }
    }
}

impl From<ank_base::UpdateStateSuccess> for UpdateStateSuccess {
    fn from(value: ank_base::UpdateStateSuccess) -> Self {
        Self {
            added_workloads: value.added_workloads,
            deleted_workloads: value.deleted_workloads,
        }
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
    use crate::objects::ConfigHash;

    mod ank_base {
        pub use api::ank_base::{
            execution_state::ExecutionStateEnum, request::RequestContent,
            response::ResponseContent, CompleteState, CompleteStateRequest, Error, ExecutionState,
            Request, Response, Running, State, UpdateStateRequest, UpdateStateSuccess, Workload,
            WorkloadInstanceName, WorkloadState,
        };
    }

    mod ankaios {
        pub use crate::{
            commands::{
                CompleteStateRequest, Error, Request, RequestContent, Response, ResponseContent,
                UpdateStateRequest, UpdateStateSuccess,
            },
            objects::{
                CompleteState, ExecutionState, State, StoredWorkloadSpec, WorkloadInstanceName,
                WorkloadState,
            },
        };
    }

    const REQUEST_ID: &str = "request_id";
    const FIELD_1: &str = "field_1";
    const FIELD_2: &str = "field_2";
    const AGENT_NAME: &str = "agent_1";
    const WORKLOAD_NAME_1: &str = "workload_name_1";
    const WORKLOAD_NAME_2: &str = "workload_name_2";
    const WORKLOAD_NAME_3: &str = "workload_name_3";
    const HASH: &str = "hash_1";
    const ERROR_MESSAGE: &str = "error_message";

    macro_rules! complete_state_request {
        ($expression:ident) => {{
            $expression::Request {
                request_id: REQUEST_ID.into(),
                request_content: $expression::RequestContent::CompleteStateRequest(
                    $expression::CompleteStateRequest {
                        field_mask: vec![FIELD_1.into(), FIELD_2.into()],
                    },
                )
                .into(),
            }
        }};
    }

    macro_rules! update_state_request {
        ($expression:ident) => {{
            $expression::Request {
                request_id: REQUEST_ID.into(),
                request_content: update_state_request_enum!($expression).into(),
            }
        }};
    }

    macro_rules! update_state_request_enum {
        (ank_base) => {
            ank_base::RequestContent::UpdateStateRequest(ank_base::UpdateStateRequest {
                new_state: complete_state!(ank_base).into(),
                update_mask: vec![FIELD_1.into(), FIELD_2.into()],
            })
        };
        (ankaios) => {
            ankaios::RequestContent::UpdateStateRequest(Box::new(ankaios::UpdateStateRequest {
                state: complete_state!(ankaios),
                update_mask: vec![FIELD_1.into(), FIELD_2.into()],
            }))
        };
    }

    macro_rules! error_response {
        ($expression:ident) => {{
            $expression::Response {
                request_id: REQUEST_ID.into(),
                response_content: $expression::ResponseContent::Error($expression::Error {
                    message: ERROR_MESSAGE.into(),
                })
                .into(),
            }
        }};
    }

    macro_rules! complete_state_response {
        ($expression:ident) => {{
            $expression::Response {
                request_id: REQUEST_ID.into(),
                response_content: $expression::ResponseContent::CompleteState(
                    complete_state!($expression).into(),
                )
                .into(),
            }
        }};
    }

    macro_rules! complete_state {
        ($expression:ident) => {
            $expression::CompleteState {
                startup_state: $expression::State {
                    api_version: "v0.1".into(),
                    workloads: vec![("startup".into(), workload!($expression))]
                        .into_iter()
                        .collect(),
                }
                .into(),
                desired_state: $expression::State {
                    api_version: "v0.1".into(),
                    workloads: vec![("desired".into(), workload!($expression))]
                        .into_iter()
                        .collect(),
                }
                .into(),
                workload_states: vec![workload_state!($expression)],
            }
        };
    }

    macro_rules! workload {
        (ank_base) => {
            ank_base::Workload {
                ..Default::default()
            }
        };
        (ankaios) => {
            ankaios::StoredWorkloadSpec {
                ..Default::default()
            }
        };
    }

    macro_rules! workload_state {
        (ankaios) => {{
            struct HashableString(String);

            impl ConfigHash for HashableString {
                fn hash_config(&self) -> String {
                    self.0.clone()
                }
            }
            ankaios::WorkloadState {
                instance_name: ankaios::WorkloadInstanceName::builder()
                    .workload_name(WORKLOAD_NAME_1)
                    .config(&HashableString(HASH.into()))
                    .agent_name(AGENT_NAME)
                    .build(),
                execution_state: ankaios::ExecutionState::running(),
            }
        }};
        (ank_base) => {
            ank_base::WorkloadState {
                instance_name: ank_base::WorkloadInstanceName {
                    workload_name: WORKLOAD_NAME_1.into(),
                    agent_name: AGENT_NAME.into(),
                    id: HASH.into(),
                }
                .into(),
                execution_state: ank_base::ExecutionState {
                    execution_state_enum: ank_base::ExecutionStateEnum::Running(
                        ank_base::Running::Ok.into(),
                    )
                    .into(),
                    ..Default::default()
                }
                .into(),
            }
        };
    }

    macro_rules! update_state_success_response {
        ($expression:ident) => {{
            $expression::Response {
                request_id: REQUEST_ID.into(),
                response_content: $expression::ResponseContent::UpdateStateSuccess(
                    $expression::UpdateStateSuccess {
                        added_workloads: vec![WORKLOAD_NAME_1.into()],
                        deleted_workloads: vec![WORKLOAD_NAME_2.into(), WORKLOAD_NAME_3.into()],
                    },
                )
                .into(),
            }
        }};
    }

    #[test]
    fn utest_converts_to_proto_complete_state_request() {
        let ankaios_request_complete_state = complete_state_request!(ankaios);
        let proto_request_complete_state = complete_state_request!(ank_base);

        assert_eq!(
            ank_base::Request::from(ankaios_request_complete_state),
            proto_request_complete_state
        );
    }

    #[test]
    fn utest_converts_from_proto_complete_state_request() {
        let proto_request_complete_state = complete_state_request!(ank_base);
        let ankaios_request_complete_state = complete_state_request!(ankaios);

        assert_eq!(
            ankaios::Request::try_from(proto_request_complete_state).unwrap(),
            ankaios_request_complete_state
        );
    }

    #[test]
    fn utest_converts_to_proto_update_state_request() {
        let ankaios_request_complete_state = update_state_request!(ankaios);
        let proto_request_complete_state = update_state_request!(ank_base);

        assert_eq!(
            ank_base::Request::from(ankaios_request_complete_state),
            proto_request_complete_state
        );
    }

    #[test]
    fn utest_converts_from_proto_update_state_request() {
        let proto_request_complete_state = update_state_request!(ank_base);
        let ankaios_request_complete_state = update_state_request!(ankaios);

        assert_eq!(
            ankaios::Request::try_from(proto_request_complete_state).unwrap(),
            ankaios_request_complete_state
        );
    }

    #[test]
    fn utest_converts_from_proto_update_state_request_with_empty_states() {
        let mut proto_request_complete_state = update_state_request!(ank_base);
        let mut ankaios_request_complete_state = update_state_request!(ankaios);

        let ank_base::RequestContent::UpdateStateRequest(proto_request_content) =
            proto_request_complete_state
                .request_content
                .as_mut()
                .unwrap()
        else {
            unreachable!()
        };
        proto_request_content.new_state = Some(ank_base::CompleteState {
            startup_state: Some(ank_base::State {
                api_version: "v0.1".into(),
                ..Default::default()
            }),
            desired_state: Some(ank_base::State {
                api_version: "v0.1".into(),
                ..Default::default()
            }),
            ..Default::default()
        });

        let ankaios::RequestContent::UpdateStateRequest(ankaios_request_content) =
            &mut ankaios_request_complete_state.request_content
        else {
            unreachable!()
        };
        ankaios_request_content.state = ankaios::CompleteState {
            ..Default::default()
        };

        assert_eq!(
            ankaios::Request::try_from(proto_request_complete_state).unwrap(),
            ankaios_request_complete_state
        );
    }

    #[test]
    fn utest_converts_from_proto_update_state_request_inner_state_with_empty_states() {
        let mut proto_request_complete_state = update_state_request!(ank_base);
        let mut ankaios_request_complete_state = update_state_request!(ankaios);

        let ank_base::RequestContent::UpdateStateRequest(proto_request_content) =
            proto_request_complete_state
                .request_content
                .as_mut()
                .unwrap()
        else {
            unreachable!()
        };
        proto_request_content
            .new_state
            .as_mut()
            .unwrap()
            .startup_state = Some(ank_base::State {
            api_version: "v0.1".into(),
            ..Default::default()
        });
        proto_request_content
            .new_state
            .as_mut()
            .unwrap()
            .desired_state = Some(ank_base::State {
            api_version: "v0.1".into(),
            ..Default::default()
        });

        let ankaios::RequestContent::UpdateStateRequest(ankaios_request_content) =
            &mut ankaios_request_complete_state.request_content
        else {
            unreachable!()
        };
        ankaios_request_content.state.startup_state = Default::default();
        ankaios_request_content.state.desired_state = Default::default();

        assert_eq!(
            ankaios::Request::try_from(proto_request_complete_state).unwrap(),
            ankaios_request_complete_state
        );
    }

    #[test]
    fn utest_converts_from_proto_update_state_request_fails_invalid_startup_state() {
        let mut proto_request_complete_state = update_state_request!(ank_base);

        let ank_base::RequestContent::UpdateStateRequest(proto_request_content) =
            proto_request_complete_state
                .request_content
                .as_mut()
                .unwrap()
        else {
            unreachable!()
        };
        proto_request_content
            .new_state
            .as_mut()
            .unwrap()
            .startup_state
            .as_mut()
            .unwrap()
            .workloads
            .insert(
                WORKLOAD_NAME_1.into(),
                ank_base::Workload {
                    dependencies: vec![("dependency".into(), -1)].into_iter().collect(),
                    ..Default::default()
                },
            );

        assert!(ankaios::Request::try_from(proto_request_complete_state).is_err());
    }

    #[test]
    fn utest_converts_from_proto_update_state_request_fails_invalid_current_state() {
        let mut proto_request_complete_state = update_state_request!(ank_base);

        let ank_base::RequestContent::UpdateStateRequest(proto_request_content) =
            proto_request_complete_state
                .request_content
                .as_mut()
                .unwrap()
        else {
            unreachable!()
        };
        proto_request_content
            .new_state
            .as_mut()
            .unwrap()
            .desired_state
            .as_mut()
            .unwrap()
            .workloads
            .insert(
                WORKLOAD_NAME_1.into(),
                ank_base::Workload {
                    dependencies: vec![("dependency".into(), -1)].into_iter().collect(),
                    ..Default::default()
                },
            );

        assert!(ankaios::Request::try_from(proto_request_complete_state).is_err());
    }

    #[test]
    fn utest_converts_from_proto_request_fails_empty_request_content() {
        let proto_request = ank_base::Request {
            request_id: REQUEST_ID.into(),
            request_content: None,
        };

        assert_eq!(
            ankaios::Request::try_from(proto_request).unwrap_err(),
            "Request has no content"
        );
    }

    #[test]
    fn utest_converts_to_proto_error_response() {
        let ankaios_error_response = error_response!(ankaios);
        let proto_error_response = error_response!(ank_base);

        assert_eq!(
            ank_base::Response::from(ankaios_error_response),
            proto_error_response
        );
    }

    #[test]
    fn utest_converts_from_proto_error_response() {
        let proto_error_response = error_response!(ank_base);
        let ankaios_error_response = error_response!(ankaios);

        assert_eq!(
            ankaios::Response::try_from(proto_error_response).unwrap(),
            ankaios_error_response,
        );
    }

    #[test]
    fn utest_converts_to_proto_complete_state_response() {
        let ankaios_complete_state_response = complete_state_response!(ankaios);
        let proto_complete_state_response = complete_state_response!(ank_base);

        assert_eq!(
            ank_base::Response::from(ankaios_complete_state_response),
            proto_complete_state_response
        );
    }

    #[test]
    fn utest_converts_from_proto_complete_state_response() {
        let proto_complete_state_response = complete_state_response!(ank_base);
        let ankaios_complete_state_response = complete_state_response!(ankaios);

        assert_eq!(
            ankaios::Response::try_from(proto_complete_state_response).unwrap(),
            ankaios_complete_state_response
        );
    }

    #[test]
    fn utest_converts_from_proto_complete_state_response_with_empty_states() {
        let mut proto_complete_state_response = complete_state_response!(ank_base);
        let mut ankaios_complete_state_response = complete_state_response!(ankaios);

        let ank_base::ResponseContent::CompleteState(proto_content) = proto_complete_state_response
            .response_content
            .as_mut()
            .unwrap()
        else {
            unreachable!()
        };
        proto_content.startup_state = Some(ank_base::State {
            api_version: "v0.1".into(),
            ..Default::default()
        });
        proto_content.desired_state = Some(ank_base::State {
            api_version: "v0.1".into(),
            ..Default::default()
        });

        let ankaios::ResponseContent::CompleteState(ankaios_content) =
            &mut ankaios_complete_state_response.response_content
        else {
            unreachable!()
        };
        ankaios_content.startup_state = Default::default();
        ankaios_content.desired_state = Default::default();

        assert_eq!(
            ankaios::Response::try_from(proto_complete_state_response).unwrap(),
            ankaios_complete_state_response
        );
    }

    #[test]
    fn utest_converts_from_proto_complete_state_response_fails_invalid_startup_state() {
        let mut proto_complete_state_response = complete_state_response!(ank_base);

        let ank_base::ResponseContent::CompleteState(proto_request_content) =
            proto_complete_state_response
                .response_content
                .as_mut()
                .unwrap()
        else {
            unreachable!()
        };

        proto_request_content
            .startup_state
            .as_mut()
            .unwrap()
            .workloads
            .insert(
                WORKLOAD_NAME_1.into(),
                ank_base::Workload {
                    dependencies: vec![("dependency".into(), -1)].into_iter().collect(),
                    ..Default::default()
                },
            );

        assert!(ankaios::Response::try_from(proto_complete_state_response).is_err());
    }

    #[test]
    fn utest_converts_to_proto_update_state_success_response() {
        let ankaios_complete_state_response = update_state_success_response!(ankaios);
        let proto_complete_state_response = update_state_success_response!(ank_base);

        assert_eq!(
            ank_base::Response::from(ankaios_complete_state_response),
            proto_complete_state_response
        );
    }

    #[test]
    fn utest_converts_from_proto_update_state_success_response() {
        let proto_complete_state_response = update_state_success_response!(ank_base);
        let ankaios_complete_state_response = update_state_success_response!(ankaios);

        assert_eq!(
            ankaios::Response::try_from(proto_complete_state_response).unwrap(),
            ankaios_complete_state_response
        );
    }

    #[test]
    fn utest_converts_from_proto_reponse_fails_empty_request_content() {
        let proto_response = ank_base::Response {
            request_id: REQUEST_ID.into(),
            response_content: None,
        };

        assert_eq!(
            ankaios::Response::try_from(proto_response).unwrap_err(),
            "Response has no content"
        );
    }
    #[test]
    fn utest_request_complete_state_prefix_request_id() {
        let mut ankaios_request_complete_state = ankaios::Request {
            request_id: "42".to_string(),
            request_content: ankaios::RequestContent::CompleteStateRequest(
                ankaios::CompleteStateRequest {
                    field_mask: vec!["1".to_string(), "2".to_string()],
                },
            ),
        };

        ankaios_request_complete_state.prefix_request_id("prefix@");

        assert_eq!("prefix@42", ankaios_request_complete_state.request_id);
    }
}

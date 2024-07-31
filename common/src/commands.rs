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
    use std::collections::HashMap;

    mod ank_base {
        pub use api::ank_base::{
            request::RequestContent, CompleteState, CompleteStateRequest, Dependencies, Request,
            RestartPolicy, State, Tag, Tags, UpdateStateRequest, Workload, WorkloadMap,
        };
    }

    mod ankaios {
        pub use crate::{
            commands::{CompleteStateRequest, Request, RequestContent, UpdateStateRequest},
            objects::{
                generate_test_workload_states_map_with_data, CompleteState, ExecutionState,
                RestartPolicy, State, StoredWorkloadSpec, Tag,
            },
        };
    }

    const REQUEST_ID: &str = "request_id";
    const FIELD_1: &str = "field_1";
    const FIELD_2: &str = "field_2";
    const AGENT_NAME: &str = "agent_1";
    const WORKLOAD_NAME_1: &str = "workload_name_1";
    const RUNTIME: &str = "my_favorite_runtime";
    const RUNTIME_CONFIG: &str = "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n";
    const HASH: &str = "hash_1";

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

    macro_rules! complete_state {
        (ankaios) => {
            ankaios::CompleteState {
                desired_state: ankaios::State {
                    api_version: "v0.1".into(),
                    workloads: HashMap::from([("desired".into(), workload!(ankaios))]),
                }
                .into(),
                workload_states: workload_states_map!(ankaios),
            }
        };
        (ank_base) => {
            ank_base::CompleteState {
                desired_state: Some(ank_base::State {
                    api_version: "v0.1".into(),
                    workloads: Some(ank_base::WorkloadMap {
                        workloads: HashMap::from([("desired".to_string(), workload!(ank_base))]),
                    }),
                }),
                workload_states: workload_states_map!(ank_base),
            }
        };
    }

    macro_rules! workload {
        (ank_base) => {
            ank_base::Workload {
                agent: Some(AGENT_NAME.to_string()),
                dependencies: None,
                restart_policy: Some(ank_base::RestartPolicy::Always.into()),
                runtime: Some(RUNTIME.to_string()),
                runtime_config: Some(RUNTIME_CONFIG.to_string()),
                tags: Some(ank_base::Tags {
                    tags: vec![ank_base::Tag {
                        key: "key".into(),
                        value: "value".into(),
                    }],
                }),
                control_interface_access: Default::default(),
            }
        };
        (ankaios) => {
            ankaios::StoredWorkloadSpec {
                agent: AGENT_NAME.to_string(),
                tags: vec![ankaios::Tag {
                    key: "key".into(),
                    value: "value".into(),
                }],
                dependencies: HashMap::new(),
                restart_policy: ankaios::RestartPolicy::Always,
                runtime: RUNTIME.to_string(),
                runtime_config: RUNTIME_CONFIG.to_string(),
                control_interface_access: Default::default(),
            }
        };
    }

    macro_rules! workload_states_map {
        (ankaios) => {{
            ankaios::generate_test_workload_states_map_with_data(
                AGENT_NAME,
                WORKLOAD_NAME_1,
                HASH,
                ankaios::ExecutionState::running(),
            )
        }};
        (ank_base) => {
            ankaios::generate_test_workload_states_map_with_data(
                AGENT_NAME,
                WORKLOAD_NAME_1,
                HASH,
                ankaios::ExecutionState::running(),
            )
            .into()
        };
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
            desired_state: Some(ank_base::State {
                api_version: "v0.1".into(),
                workloads: Some(ank_base::WorkloadMap {
                    workloads: HashMap::new(),
                }),
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
            .desired_state = Some(ank_base::State {
            api_version: "v0.1".into(),
            workloads: Some(ank_base::WorkloadMap {
                workloads: HashMap::new(),
            }),
        });

        let ankaios::RequestContent::UpdateStateRequest(ankaios_request_content) =
            &mut ankaios_request_complete_state.request_content
        else {
            unreachable!()
        };
        ankaios_request_content.state.desired_state = Default::default();

        assert_eq!(
            ankaios::Request::try_from(proto_request_complete_state).unwrap(),
            ankaios_request_complete_state
        );
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
            .as_mut()
            .unwrap()
            .workloads
            .insert(
                WORKLOAD_NAME_1.into(),
                ank_base::Workload {
                    dependencies: Some(ank_base::Dependencies {
                        dependencies: HashMap::from([("dependency".into(), -1)]),
                    }),
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

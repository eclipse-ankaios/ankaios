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

use api::ank_base::{
    self, CompleteStateRequest, CpuUsageInternal, DeletedWorkload, FreeMemoryInternal,
    LogsRequestInternal, UpdateStateRequestInternal, WorkloadNamed, WorkloadStateInternal,
};
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
    pub workload_states: Vec<WorkloadStateInternal>,
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
    pub fn prefix_id(prefix: &str, request_id: &String) -> String {
        format!("{prefix}{request_id}")
    }
    pub fn prefix_request_id(&mut self, prefix: &str) {
        self.request_id = Self::prefix_id(prefix, &self.request_id);
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
    UpdateStateRequest(Box<UpdateStateRequestInternal>),
    LogsRequest(LogsRequestInternal),
    LogsCancelRequest,
}

impl From<RequestContent> for ank_base::request::RequestContent {
    fn from(value: RequestContent) -> Self {
        match value {
            RequestContent::CompleteStateRequest(content) => {
                ank_base::request::RequestContent::CompleteStateRequest(content)
            }
            RequestContent::UpdateStateRequest(content) => {
                ank_base::request::RequestContent::UpdateStateRequest(Box::new((*content).into()))
            }
            // TODO: tests are missing for the next two cases
            RequestContent::LogsRequest(logs_request) => {
                ank_base::request::RequestContent::LogsRequest(logs_request.into())
            }
            RequestContent::LogsCancelRequest => {
                ank_base::request::RequestContent::LogsCancelRequest(ank_base::LogsCancelRequest {})
            }
        }
    }
}

impl TryFrom<ank_base::request::RequestContent> for RequestContent {
    type Error = String;
    fn try_from(value: ank_base::request::RequestContent) -> Result<Self, Self::Error> {
        Ok(match value {
            ank_base::request::RequestContent::UpdateStateRequest(value) => {
                RequestContent::UpdateStateRequest(Box::new((*value).try_into()?))
            }
            ank_base::request::RequestContent::CompleteStateRequest(value) => {
                RequestContent::CompleteStateRequest(value)
            }
            // TODO: tests are missing for the next two cases
            ank_base::request::RequestContent::LogsRequest(logs_request) => {
                // MARK #313 LogsRequest -> LogsRequestInternal
                RequestContent::LogsRequest(LogsRequestInternal {
                    workload_names: logs_request
                        .workload_names
                        .into_iter()
                        .map(|name| name.try_into())
                        .collect::<Result<_, _>>()?,
                    follow: logs_request.follow.unwrap_or(false),
                    tail: logs_request.tail.unwrap_or(-1),
                    since: logs_request.since,
                    until: logs_request.until,
                })
            }
            ank_base::request::RequestContent::LogsCancelRequest(_logs_stop_request) => {
                RequestContent::LogsCancelRequest
            }
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ServerHello {
    pub agent_name: Option<String>,
    pub added_workloads: Vec<WorkloadNamed>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateWorkload {
    pub added_workloads: Vec<WorkloadNamed>,
    pub deleted_workloads: Vec<DeletedWorkload>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Goodbye {
    pub connection_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Stop {}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentLoadStatus {
    pub agent_name: String,
    pub cpu_usage: CpuUsageInternal,
    pub free_memory: FreeMemoryInternal,
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
    use api::CURRENT_API_VERSION;
    use api::ank_base::{
        CompleteState, CompleteStateInternal, CompleteStateRequest, ConfigMappings,
        ConfigMappingsInternal,
    };
    use std::collections::HashMap;

    mod ank_base {
        pub use api::ank_base::{
            Dependencies, File, Files, LogsRequest, Request, RequestContent, RestartPolicy, State,
            UpdateStateRequest, Workload, WorkloadInstanceName, WorkloadMap, file::FileContent,
        };
    }

    mod ankaios {
        pub use crate::commands::{Request, RequestContent};
        pub use api::ank_base::{
            ExecutionStateInternal, FileContentInternal, FileInternal, FilesInternal,
            LogsRequestInternal, RestartPolicy, StateInternal, TagsInternal,
            UpdateStateRequestInternal, WorkloadInstanceNameInternal, WorkloadInternal,
            WorkloadMapInternal,
        };
        pub use api::test_utils::{
            generate_test_agent_map, generate_test_workload_states_map_with_data,
        };
    }

    const REQUEST_ID: &str = "request_id";
    const FIELD_1: &str = "field_1";
    const FIELD_2: &str = "field_2";
    const AGENT_NAME: &str = "agent_1";
    const WORKLOAD_NAME_1: &str = "workload_name_1";
    const WORKLOAD_NAME_2: &str = "workload_name_2";
    const INSTANCE_ID_1: &str = "instance_id_1";
    const INSTANCE_ID_2: &str = "instance_id_2";
    const RUNTIME: &str = "my_favorite_runtime";
    const RUNTIME_CONFIG: &str = "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n";
    const HASH: &str = "hash_1";

    macro_rules! complete_state_request {
        ($expression:ident) => {{
            $expression::Request {
                request_id: REQUEST_ID.into(),
                request_content: $expression::RequestContent::CompleteStateRequest(
                    CompleteStateRequest {
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
            ank_base::RequestContent::UpdateStateRequest(Box::new(ank_base::UpdateStateRequest {
                new_state: complete_state!(ank_base).into(),
                update_mask: vec![FIELD_1.into(), FIELD_2.into()],
            }))
        };
        (ankaios) => {
            ankaios::RequestContent::UpdateStateRequest(Box::new(
                ankaios::UpdateStateRequestInternal {
                    new_state: complete_state!(ankaios),
                    update_mask: vec![FIELD_1.into(), FIELD_2.into()],
                },
            ))
        };
    }

    macro_rules! logs_request {
        (ank_base) => {{
            ank_base::Request {
                request_id: REQUEST_ID.into(),
                request_content: ank_base::RequestContent::LogsRequest(ank_base::LogsRequest {
                    workload_names: vec![
                        workload_instance_name!(ank_base, 1),
                        workload_instance_name!(ank_base, 2),
                    ],
                    follow: Some(true),
                    tail: Some(10),
                    since: None,
                    until: None,
                })
                .into(),
            }
        }};
        (ankaios) => {{
            ankaios::Request {
                request_id: REQUEST_ID.into(),
                request_content: ankaios::RequestContent::LogsRequest(
                    ankaios::LogsRequestInternal {
                        workload_names: vec![
                            workload_instance_name!(ankaios, 1),
                            workload_instance_name!(ankaios, 2),
                        ],
                        follow: true,
                        tail: 10,
                        since: None,
                        until: None,
                    },
                )
                .into(),
            }
        }};
    }

    macro_rules! workload_instance_name {
        (ank_base, $number:expr) => {
            ank_base::WorkloadInstanceName {
                agent_name: AGENT_NAME.into(),
                workload_name: workload_name!($number).into(),
                id: instance_id!($number).into(),
            }
        };
        (ankaios, $number:expr) => {
            ankaios::WorkloadInstanceNameInternal {
                workload_name: workload_name!($number).to_owned(),
                agent_name: AGENT_NAME.to_owned(),
                id: instance_id!($number).to_owned(),
            }
        };
    }

    macro_rules! workload_name {
        ($number:literal) => {
            [WORKLOAD_NAME_1, WORKLOAD_NAME_2][$number - 1]
        };
    }

    macro_rules! instance_id {
        ($number:literal) => {
            [INSTANCE_ID_1, INSTANCE_ID_2][$number - 1]
        };
    }

    macro_rules! logs_cancel_request {
        (ank_base) => {
            ank_base::Request {
                request_id: REQUEST_ID.into(),
                request_content: ank_base::RequestContent::LogsCancelRequest(
                    api::ank_base::LogsCancelRequest {},
                )
                .into(),
            }
        };
        (ankaios) => {
            ankaios::Request {
                request_id: REQUEST_ID.into(),
                request_content: ankaios::RequestContent::LogsCancelRequest,
            }
        };
    }

    macro_rules! complete_state {
        (ank_base) => {
            CompleteState {
                desired_state: Some(ank_base::State {
                    api_version: CURRENT_API_VERSION.into(),
                    workloads: Some(ank_base::WorkloadMap {
                        workloads: HashMap::from([(
                            "workload_name".to_string(),
                            workload!(ank_base),
                        )]),
                    }),
                    configs: Some(Default::default()),
                }),
                workload_states: workload_states_map!(ank_base),
                agents: agent_map!(ank_base),
            }
        };
        (ankaios) => {
            CompleteStateInternal {
                desired_state: ankaios::StateInternal {
                    api_version: CURRENT_API_VERSION.into(),
                    workloads: ankaios::WorkloadMapInternal {
                        workloads: HashMap::from([(
                            "workload_name".to_string(),
                            workload!(ankaios),
                        )]),
                    },
                    configs: Default::default(),
                }
                .into(),
                workload_states: workload_states_map!(ankaios),
                agents: agent_map!(ankaios),
            }
        };
    }

    macro_rules! workload {
        (ank_base) => {
            ank_base::Workload {
                agent: Some(AGENT_NAME.to_string()),
                dependencies: Some(ank_base::Dependencies::default()),
                restart_policy: Some(ank_base::RestartPolicy::Always.into()),
                runtime: Some(RUNTIME.to_string()),
                runtime_config: Some(RUNTIME_CONFIG.to_string()),
                tags: Some(api::ank_base::Tags {
                    tags: HashMap::from([("key".into(), "value".into())]),
                }),
                control_interface_access: Some(Default::default()),
                configs: Some(ConfigMappings {
                    configs: [
                        ("ref1".into(), "config_1".into()),
                        ("ref2".into(), "config_2".into()),
                    ]
                    .into(),
                }),
                files: Some(ank_base::Files {
                    files: vec![
                        ank_base::File {
                            mount_point: "/file.json".to_string(),
                            file_content: Some(ank_base::FileContent::Data("text data".into())),
                        },
                        ank_base::File {
                            mount_point: "/binary_file".to_string(),
                            file_content: Some(ank_base::FileContent::BinaryData(
                                "base64_data".into(),
                            )),
                        },
                    ],
                }),
            }
        };
        (ankaios) => {
            ankaios::WorkloadInternal {
                agent: AGENT_NAME.to_string(),
                tags: ankaios::TagsInternal {
                    tags: HashMap::from([("key".into(), "value".into())]),
                },
                dependencies: Default::default(),
                restart_policy: ankaios::RestartPolicy::Always,
                runtime: RUNTIME.to_string(),
                runtime_config: RUNTIME_CONFIG.to_string(),
                control_interface_access: Default::default(),
                configs: ConfigMappingsInternal {
                    configs: HashMap::from([
                        ("ref1".into(), "config_1".into()),
                        ("ref2".into(), "config_2".into()),
                    ]),
                },
                files: ankaios::FilesInternal {
                    files: vec![
                        ankaios::FileInternal {
                            mount_point: "/file.json".to_string(),
                            file_content: ankaios::FileContentInternal::Data {
                                data: "text data".into(),
                            },
                        },
                        ankaios::FileInternal {
                            mount_point: "/binary_file".to_string(),
                            file_content: ankaios::FileContentInternal::BinaryData {
                                binary_data: "base64_data".into(),
                            },
                        },
                    ],
                },
            }
        };
    }

    macro_rules! workload_states_map {
        (ank_base) => {
            Some(
                ankaios::generate_test_workload_states_map_with_data(
                    AGENT_NAME,
                    WORKLOAD_NAME_1,
                    HASH,
                    ankaios::ExecutionStateInternal::running(),
                )
                .into(),
            )
        };
        (ankaios) => {{
            ankaios::generate_test_workload_states_map_with_data(
                AGENT_NAME,
                WORKLOAD_NAME_1,
                HASH,
                ankaios::ExecutionStateInternal::running(),
            )
        }};
    }

    macro_rules! agent_map {
        (ank_base) => {
            Some(ankaios::generate_test_agent_map(AGENT_NAME).into())
        };
        (ankaios) => {{ ankaios::generate_test_agent_map(AGENT_NAME) }};
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
        proto_request_content.new_state = Some(CompleteState {
            desired_state: Some(ank_base::State {
                api_version: CURRENT_API_VERSION.into(),
                workloads: Some(ank_base::WorkloadMap {
                    workloads: HashMap::new(),
                }),
                configs: Some(Default::default()),
            }),
            workload_states: Some(Default::default()),
            agents: Some(Default::default()),
        });

        let ankaios::RequestContent::UpdateStateRequest(ankaios_request_content) =
            &mut ankaios_request_complete_state.request_content
        else {
            unreachable!()
        };
        ankaios_request_content.new_state = CompleteStateInternal {
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
            api_version: CURRENT_API_VERSION.into(),
            workloads: Some(ank_base::WorkloadMap {
                workloads: HashMap::new(),
            }),
            configs: Some(Default::default()),
        });

        let ankaios::RequestContent::UpdateStateRequest(ankaios_request_content) =
            &mut ankaios_request_complete_state.request_content
        else {
            unreachable!()
        };
        ankaios_request_content.new_state.desired_state = Default::default();

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
    fn utest_converts_from_proto_logs_request() {
        let proto_logs_request = logs_request!(ank_base);
        let ankaios_logs_request = logs_request!(ankaios);
        assert_eq!(
            ankaios::Request::try_from(proto_logs_request).unwrap(),
            ankaios_logs_request
        );
    }

    #[test]
    fn utest_converts_from_proto_logs_cancel_request() {
        let proto_logs_cancel_request = logs_cancel_request!(ank_base);
        let ankaios_logs_cancel_request = logs_cancel_request!(ankaios);
        assert_eq!(
            ankaios::Request::try_from(proto_logs_cancel_request).unwrap(),
            ankaios_logs_cancel_request
        );
    }

    #[test]
    fn utest_converts_to_proto_logs_cancel_request() {
        let proto_logs_cancel_request = logs_cancel_request!(ank_base);
        let ankaios_logs_cancel_request = logs_cancel_request!(ankaios);
        assert_eq!(
            ank_base::Request::from(ankaios_logs_cancel_request),
            proto_logs_cancel_request
        );
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
    fn utest_prefix_id() {
        let request_id = "42".to_string();
        let prefix = "prefix@";
        let prefixed_request_id = ankaios::Request::prefix_id(prefix, &request_id);

        assert_eq!("prefix@42", prefixed_request_id);
    }

    #[test]
    fn utest_request_complete_state_prefix_request_id() {
        let mut ankaios_request_complete_state = ankaios::Request {
            request_id: "42".to_string(),
            request_content: ankaios::RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec!["1".to_string(), "2".to_string()],
            }),
        };

        ankaios_request_complete_state.prefix_request_id("prefix@");

        assert_eq!("prefix@42", ankaios_request_complete_state.request_id);
    }
}

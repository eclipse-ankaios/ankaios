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
    CpuUsageSpec, DeletedWorkload, FreeMemorySpec, WorkloadNamed, WorkloadStateSpec,
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
    pub workload_states: Vec<WorkloadStateSpec>,
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
    pub cpu_usage: CpuUsageSpec,
    pub free_memory: FreeMemorySpec,
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
    use api::ank_base::{CompleteStateRequestSpec, RequestContentSpec, RequestSpec};

    // The following commented imports and constants are kept for reference
    // for when the macros bellow will be used instead of fixtures.

    // use api::CURRENT_API_VERSION;
    // use api::ank_base::{
    //     CompleteState, CompleteStateRequest, ConfigMappings, Dependencies, File, FileContent,
    //     Files, LogsCancelRequest, LogsRequest, Request, RequestContent, RestartPolicy, State, Tags,
    //     UpdateStateRequest, Workload, WorkloadInstanceName, WorkloadMap,
    // };
    // use api::ank_base::{
    //     CompleteStateSpec, CompleteStateRequestSpec, ConfigMappingsSpec,
    //     ExecutionStateSpec, FileContentSpec, FileSpec, FilesSpec,
    //     LogsCancelRequestSpec, LogsRequestSpec, RequestContentSpec, RequestSpec,
    //     StateSpec, TagsSpec, UpdateStateRequestSpec, WorkloadInstanceNameSpec,
    //     WorkloadSpec, WorkloadMapSpec,
    // };
    // pub use api::test_utils::{
    //     generate_test_agent_map, generate_test_workload_states_map_with_data,
    // };
    // use std::collections::HashMap;

    // const REQUEST_ID: &str = "request_id";
    // const FIELD_1: &str = "field_1";
    // const FIELD_2: &str = "field_2";
    // const AGENT_NAME: &str = "agent_1";
    // const WORKLOAD_NAME_1: &str = "workload_name_1";
    // const WORKLOAD_NAME_2: &str = "workload_name_2";
    // const INSTANCE_ID_1: &str = "instance_id_1";
    // const INSTANCE_ID_2: &str = "instance_id_2";
    // const RUNTIME: &str = "my_favorite_runtime";
    // const RUNTIME_CONFIG: &str = "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n";
    // const HASH: &str = "hash_1";

    #[allow(unused_macros)]
    macro_rules! update_state_request {
        (ank_base) => {{
            Request {
                request_id: REQUEST_ID.into(),
                request_content: RequestContent::UpdateStateRequest(Box::new(UpdateStateRequest {
                    new_state: complete_state!(ank_base).into(),
                    update_mask: vec![FIELD_1.into(), FIELD_2.into()],
                }))
                .into(),
            }
        }};
        (ankaios) => {{
            RequestSpec {
                request_id: REQUEST_ID.into(),
                request_content: RequestContentSpec::UpdateStateRequest(Box::new(
                    UpdateStateRequestSpec {
                        new_state: complete_state!(ankaios),
                        update_mask: vec![FIELD_1.into(), FIELD_2.into()],
                    },
                ))
                .into(),
            }
        }};
    }

    #[allow(unused_macros)]
    macro_rules! workload_instance_name {
        (ank_base, $number:expr) => {
            WorkloadInstanceName {
                agent_name: AGENT_NAME.into(),
                workload_name: workload_name!($number).into(),
                id: instance_id!($number).into(),
            }
        };
        (ankaios, $number:expr) => {
            WorkloadInstanceNameSpec {
                workload_name: workload_name!($number).to_owned(),
                agent_name: AGENT_NAME.to_owned(),
                id: instance_id!($number).to_owned(),
            }
        };
    }

    #[allow(unused_macros)]
    macro_rules! workload_name {
        ($number:literal) => {
            [WORKLOAD_NAME_1, WORKLOAD_NAME_2][$number - 1]
        };
    }

    #[allow(unused_macros)]
    macro_rules! instance_id {
        ($number:literal) => {
            [INSTANCE_ID_1, INSTANCE_ID_2][$number - 1]
        };
    }

    #[allow(unused_macros)]
    macro_rules! logs_cancel_request {
        (ank_base) => {
            Request {
                request_id: REQUEST_ID.into(),
                request_content: RequestContent::LogsCancelRequest(LogsCancelRequest {}).into(),
            }
        };
        (ankaios) => {
            RequestSpec {
                request_id: REQUEST_ID.into(),
                request_content: RequestContentSpec::LogsCancelRequest(LogsCancelRequestSpec {}),
            }
        };
    }

    #[allow(unused_macros)]
    macro_rules! complete_state {
        (ank_base) => {
            CompleteState {
                desired_state: Some(State {
                    api_version: CURRENT_API_VERSION.into(),
                    workloads: Some(WorkloadMap {
                        workloads: HashMap::from([(
                            "workload_name".to_string(),
                            workload!(ank_base),
                        )]),
                    }),
                    configs: Some(Default::default()),
                }),
                workload_states: workload_states_map!(ank_base),
                agents: Some(generate_test_agent_map(AGENT_NAME).into()),
            }
        };
        (ankaios) => {
            CompleteStateSpec {
                desired_state: StateSpec {
                    api_version: CURRENT_API_VERSION.into(),
                    workloads: WorkloadMapSpec {
                        workloads: HashMap::from([(
                            "workload_name".to_string(),
                            workload!(ankaios),
                        )]),
                    },
                    configs: Default::default(),
                }
                .into(),
                workload_states: workload_states_map!(ankaios),
                agents: generate_test_agent_map(AGENT_NAME),
            }
        };
    }

    #[allow(unused_macros)]
    macro_rules! workload {
        (ank_base) => {
            Workload {
                agent: Some(AGENT_NAME.to_string()),
                dependencies: Some(Dependencies::default()),
                restart_policy: Some(RestartPolicy::Always.into()),
                runtime: Some(RUNTIME.to_string()),
                runtime_config: Some(RUNTIME_CONFIG.to_string()),
                tags: Some(Tags {
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
                files: Some(Files {
                    files: vec![
                        File {
                            mount_point: "/file.json".to_string(),
                            file_content: Some(FileContent::Data("text data".into())),
                        },
                        File {
                            mount_point: "/binary_file".to_string(),
                            file_content: Some(FileContent::BinaryData("base64_data".into())),
                        },
                    ],
                }),
            }
        };
        (ankaios) => {
            WorkloadSpec {
                agent: AGENT_NAME.to_string(),
                tags: TagsSpec {
                    tags: HashMap::from([("key".into(), "value".into())]),
                },
                dependencies: Default::default(),
                restart_policy: RestartPolicy::Always,
                runtime: RUNTIME.to_string(),
                runtime_config: RUNTIME_CONFIG.to_string(),
                control_interface_access: Default::default(),
                configs: ConfigMappingsSpec {
                    configs: HashMap::from([
                        ("ref1".into(), "config_1".into()),
                        ("ref2".into(), "config_2".into()),
                    ]),
                },
                files: FilesSpec {
                    files: vec![
                        FileSpec {
                            mount_point: "/file.json".to_string(),
                            file_content: FileContentSpec::Data {
                                data: "text data".into(),
                            },
                        },
                        FileSpec {
                            mount_point: "/binary_file".to_string(),
                            file_content: FileContentSpec::BinaryData {
                                binary_data: "base64_data".into(),
                            },
                        },
                    ],
                },
            }
        };
    }

    #[allow(unused_macros)]
    macro_rules! workload_states_map {
        (ank_base) => {
            Some(
                generate_test_workload_states_map_with_data(
                    AGENT_NAME,
                    WORKLOAD_NAME_1,
                    HASH,
                    ExecutionStateSpec::running(),
                )
                .into(),
            )
        };
        (ankaios) => {{
            generate_test_workload_states_map_with_data(
                AGENT_NAME,
                WORKLOAD_NAME_1,
                HASH,
                ExecutionStateSpec::running(),
            )
        }};
    }

    #[test]
    fn utest_prefix_id() {
        let request_id = "42".to_string();
        let prefix = "prefix@";
        let prefixed_request_id = RequestSpec::prefix_id(prefix, &request_id);

        assert_eq!("prefix@42", prefixed_request_id);
    }

    #[test]
    fn utest_request_complete_state_prefix_request_id() {
        let mut ankaios_request_complete_state = RequestSpec {
            request_id: "42".to_string(),
            request_content: RequestContentSpec::CompleteStateRequest(CompleteStateRequestSpec {
                field_mask: vec!["1".to_string(), "2".to_string()],
            }),
        };

        ankaios_request_complete_state.prefix_request_id("prefix@");

        assert_eq!("prefix@42", ankaios_request_complete_state.request_id);
    }
}

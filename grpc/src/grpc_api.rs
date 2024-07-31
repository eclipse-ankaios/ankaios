// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use common::commands;
use common::from_server_interface;
use common::objects;
use common::to_server_interface;
use std::collections::HashMap;

// [impl->swdd~grpc-delegate-workflow-to-external-library~1]
tonic::include_proto!("grpc_api"); // The string specified here must match the proto package name

impl From<AgentHello> for commands::AgentHello {
    fn from(item: AgentHello) -> Self {
        commands::AgentHello {
            agent_name: item.agent_name,
        }
    }
}

impl From<commands::UpdateWorkloadState> for UpdateWorkloadState {
    fn from(item: commands::UpdateWorkloadState) -> Self {
        UpdateWorkloadState {
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl From<UpdateWorkloadState> for commands::UpdateWorkloadState {
    fn from(item: UpdateWorkloadState) -> Self {
        commands::UpdateWorkloadState {
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl TryFrom<from_server_interface::FromServer> for FromServer {
    type Error = &'static str;

    fn try_from(item: from_server_interface::FromServer) -> Result<Self, Self::Error> {
        match item {
            from_server_interface::FromServer::UpdateWorkload(ankaios) => Ok(FromServer {
                from_server_enum: Some(from_server::FromServerEnum::UpdateWorkload(
                    UpdateWorkload {
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
            from_server_interface::FromServer::UpdateWorkloadState(ankaios) => Ok(FromServer {
                from_server_enum: Some(from_server::FromServerEnum::UpdateWorkloadState(
                    UpdateWorkloadState {
                        workload_states: ankaios
                            .workload_states
                            .iter()
                            .map(|x| x.to_owned().into())
                            .collect(),
                    },
                )),
            }),
            from_server_interface::FromServer::Response(response) => Ok(FromServer {
                from_server_enum: Some(from_server::FromServerEnum::Response(response)),
            }),
            from_server_interface::FromServer::Stop(_) => {
                Err("Stop command not implemented in proto")
            }
        }
    }
}

impl TryFrom<DeletedWorkload> for objects::DeletedWorkload {
    type Error = String;

    fn try_from(deleted_workload: DeletedWorkload) -> Result<Self, Self::Error> {
        Ok(objects::DeletedWorkload {
            instance_name: deleted_workload
                .instance_name
                .ok_or("No instance name")?
                .into(),
            dependencies: deleted_workload
                .dependencies
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<HashMap<String, objects::DeleteCondition>, String>>()?,
        })
    }
}

impl From<objects::DeletedWorkload> for DeletedWorkload {
    fn from(value: objects::DeletedWorkload) -> Self {
        DeletedWorkload {
            instance_name: super::ank_base::WorkloadInstanceName::from(value.instance_name).into(),
            dependencies: value
                .dependencies
                .into_iter()
                .map(|(k, v)| (k, v as i32))
                .collect(),
        }
    }
}

impl TryFrom<AddedWorkload> for objects::WorkloadSpec {
    type Error = String;

    fn try_from(workload: AddedWorkload) -> Result<Self, String> {
        Ok(objects::WorkloadSpec {
            dependencies: workload
                .dependencies
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<HashMap<String, objects::AddCondition>, String>>()?,
            restart_policy: workload.restart_policy.try_into()?,
            runtime: workload.runtime,
            instance_name: workload.instance_name.ok_or("No instance name")?.into(),
            tags: workload.tags.into_iter().map(|x| x.into()).collect(),
            runtime_config: workload.runtime_config,
            control_interface_access: workload
                .control_interface_access
                .unwrap_or_default()
                .try_into()?,
        })
    }
}

impl From<objects::WorkloadSpec> for AddedWorkload {
    fn from(workload: objects::WorkloadSpec) -> Self {
        AddedWorkload {
            instance_name: super::ank_base::WorkloadInstanceName::from(workload.instance_name)
                .into(),
            dependencies: workload
                .dependencies
                .into_iter()
                .map(|(k, v)| (k, v as i32))
                .collect(),
            restart_policy: workload.restart_policy as i32,
            runtime: workload.runtime,
            runtime_config: workload.runtime_config,
            tags: workload.tags.into_iter().map(|x| x.into()).collect(),
            control_interface_access: workload.control_interface_access.into(),
        }
    }
}

impl TryFrom<ToServer> for to_server_interface::ToServer {
    type Error = String;

    fn try_from(item: ToServer) -> Result<Self, Self::Error> {
        use to_server::ToServerEnum;
        let to_server = item.to_server_enum.ok_or("ToServer is None.".to_string())?;

        Ok(match to_server {
            ToServerEnum::AgentHello(protobuf) => {
                to_server_interface::ToServer::AgentHello(protobuf.into())
            }
            ToServerEnum::UpdateWorkloadState(protobuf) => {
                to_server_interface::ToServer::UpdateWorkloadState(protobuf.into())
            }
            ToServerEnum::Request(protobuf) => {
                to_server_interface::ToServer::Request(protobuf.try_into()?)
            }
            ToServerEnum::Goodbye(_) => {
                to_server_interface::ToServer::Goodbye(commands::Goodbye {})
            }
        })
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
fn generate_test_proto_delete_dependencies() -> HashMap<String, i32> {
    HashMap::from([(
        String::from("workload A"),
        DeleteCondition::DelCondNotPendingNorRunning.into(),
    )])
}

#[cfg(test)]
pub fn generate_test_proto_deleted_workload() -> DeletedWorkload {
    let instance_name = common::objects::WorkloadInstanceName::builder()
        .agent_name("agent")
        .workload_name("workload X")
        .config(&String::from("config"))
        .build();

    DeletedWorkload {
        instance_name: Some(instance_name.into()),
        dependencies: generate_test_proto_delete_dependencies(),
    }
}

#[cfg(test)]
pub fn generate_test_failed_update_workload_state(
    agent_name: &str,
    workload_name: &str,
) -> common::to_server_interface::ToServer {
    use common::objects::ExecutionState;

    common::to_server_interface::ToServer::UpdateWorkloadState(commands::UpdateWorkloadState {
        workload_states: vec![common::objects::generate_test_workload_state_with_agent(
            workload_name,
            agent_name,
            ExecutionState::failed("additional_info"),
        )],
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        from_server::FromServerEnum, generate_test_proto_deleted_workload, to_server::ToServerEnum,
        AddedWorkload, AgentHello, DeletedWorkload, FromServer, ToServer, UpdateWorkload,
        UpdateWorkloadState,
    };

    use api::ank_base::{self, Dependencies};
    use common::{
        objects::{generate_test_workload_spec, ConfigHash},
        test_utils::{self, generate_test_deleted_workload},
    };

    mod ankaios {
        pub use common::{
            commands::*, from_server_interface::FromServer, objects::*,
            to_server_interface::ToServer,
        };
    }

    ///////////////////////////////////////////////////////////////////////////
    // ToServer tests
    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn utest_convert_proto_to_server_agent_hello() {
        let agent_name = "agent_A".to_string();

        let proto_request = ToServer {
            to_server_enum: Some(ToServerEnum::AgentHello(AgentHello {
                agent_name: agent_name.clone(),
            })),
        };

        let ankaios_command = ankaios::ToServer::AgentHello(ankaios::AgentHello { agent_name });

        assert_eq!(
            ankaios::ToServer::try_from(proto_request),
            Ok(ankaios_command)
        );
    }

    #[test]
    fn utest_convert_proto_to_server_update_workload_state() {
        let proto_request = ToServer {
            to_server_enum: Some(ToServerEnum::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![],
            })),
        };

        let ankaios_command =
            ankaios::ToServer::UpdateWorkloadState(ankaios::UpdateWorkloadState {
                workload_states: vec![],
            });

        assert_eq!(
            ankaios::ToServer::try_from(proto_request),
            Ok(ankaios_command)
        );
    }

    #[test]
    fn utest_convert_proto_to_server_update_state() {
        let ankaios_request = ankaios::Request {
            request_id: "request_id".to_owned(),
            request_content: ankaios::RequestContent::UpdateStateRequest(Box::new(
                ankaios::UpdateStateRequest {
                    update_mask: vec!["test_update_mask_field".to_owned()],
                    state: ankaios::CompleteState {
                        desired_state: ankaios::State {
                            workloads: HashMap::from([(
                                "test_workload".to_owned(),
                                ankaios::StoredWorkloadSpec {
                                    agent: "test_agent".to_string(),
                                    ..Default::default()
                                },
                            )]),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                },
            )),
        };

        let proto_request = ToServer {
            to_server_enum: Some(ToServerEnum::Request(ankaios_request.clone().into())),
        };

        let ankaios_command = ankaios::ToServer::Request(ankaios_request);

        assert_eq!(
            ankaios::ToServer::try_from(proto_request),
            Ok(ankaios_command)
        );
    }

    #[test]
    fn utest_convert_proto_to_server_update_state_fails() {
        let workloads = ank_base::Workload {
            agent: Some("test_agent".to_owned()),
            dependencies: Some(Dependencies {
                dependencies: vec![("other_workload".into(), -1)].into_iter().collect(),
            }),
            ..Default::default()
        };
        let proto_request = ToServer {
            to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                request_id: "requeset_id".to_owned(),
                request_content: Some(ank_base::request::RequestContent::UpdateStateRequest(
                    ank_base::UpdateStateRequest {
                        update_mask: vec!["test_update_mask_field".to_owned()],
                        new_state: Some(test_utils::generate_test_proto_complete_state(&[(
                            "test_workload",
                            workloads,
                        )])),
                    },
                )),
            })),
        };

        assert!(ankaios::ToServer::try_from(proto_request).is_err(),);
    }

    #[test]
    fn utest_convert_proto_to_server_request_complete_state() {
        let request_id = "42".to_string();
        let field_mask = vec!["1".to_string()];

        let proto_request = ToServer {
            to_server_enum: Some(ToServerEnum::Request(ank_base::Request {
                request_id: request_id.clone(),
                request_content: Some(ank_base::request::RequestContent::CompleteStateRequest(
                    ank_base::CompleteStateRequest {
                        field_mask: field_mask.clone(),
                    },
                )),
            })),
        };

        let ankaios_command = ankaios::ToServer::Request(ankaios::Request {
            request_id,
            request_content: ankaios::RequestContent::CompleteStateRequest(
                ankaios::CompleteStateRequest { field_mask },
            ),
        });

        assert_eq!(
            ankaios::ToServer::try_from(proto_request),
            Ok(ankaios_command)
        );
    }

    ///////////////////////////////////////////////////////////////////////////
    // FromServer tests
    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn utest_convert_from_server_to_proto_update_workload() {
        let instance_name = ankaios::WorkloadInstanceName::builder()
            .workload_name("test_workload")
            .build();
        let test_ex_com = ankaios::FromServer::UpdateWorkload(ankaios::UpdateWorkload {
            added_workloads: vec![ankaios::WorkloadSpec {
                instance_name,
                runtime: "tes_runtime".to_owned(),
                ..Default::default()
            }],
            deleted_workloads: vec![generate_test_deleted_workload(
                "agent".to_string(),
                "workload X".to_string(),
            )],
        });
        let expected_ex_com = Ok(FromServer {
            from_server_enum: Some(FromServerEnum::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![AddedWorkload {
                    instance_name: Some(ank_base::WorkloadInstanceName {
                        workload_name: "test_workload".to_owned(),
                        ..Default::default()
                    }),
                    runtime: "tes_runtime".to_owned(),
                    ..Default::default()
                }],
                deleted_workloads: vec![generate_test_proto_deleted_workload()],
            })),
        });

        assert_eq!(FromServer::try_from(test_ex_com), expected_ex_com);
    }

    #[test]
    fn utest_convert_from_server_to_proto_update_workload_state() {
        let workload_state = ankaios::generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ankaios::ExecutionState::running(),
        );

        let test_ex_com = ankaios::FromServer::UpdateWorkloadState(ankaios::UpdateWorkloadState {
            workload_states: vec![workload_state.clone()],
        });
        let expected_ex_com = Ok(FromServer {
            from_server_enum: Some(FromServerEnum::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![workload_state.into()],
            })),
        });

        assert_eq!(FromServer::try_from(test_ex_com), expected_ex_com);
    }

    #[test]
    fn utest_convert_from_server_to_proto_complete_state() {
        let proto_response = ank_base::Response {
            request_id: "req_id".to_owned(),
            response_content: Some(ank_base::response::ResponseContent::CompleteState(
                ank_base::CompleteState {
                    desired_state: Some(api::ank_base::State {
                        api_version: "v0.1".into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )),
        };

        let ankaios_msg = ankaios::FromServer::Response(proto_response.clone());

        let proto_msg = Ok(FromServer {
            from_server_enum: Some(FromServerEnum::Response(proto_response)),
        });

        assert_eq!(FromServer::try_from(ankaios_msg), proto_msg);
    }
    ///////////////////////////////////////////////////////////////////////////
    // WorkloadSpec tests
    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn utest_converts_to_proto_deleted_workload() {
        let proto_workload = generate_test_proto_deleted_workload();
        let workload =
            generate_test_deleted_workload("agent".to_string(), "workload X".to_string());

        assert_eq!(DeletedWorkload::from(workload), proto_workload);
    }

    #[test]
    fn utest_converts_to_ankaios_deleted_workload() {
        let proto_workload = generate_test_proto_deleted_workload();
        let workload =
            generate_test_deleted_workload("agent".to_string(), "workload X".to_string());

        assert_eq!(
            ankaios::DeletedWorkload::try_from(proto_workload),
            Ok(workload)
        );
    }

    #[test]
    fn utest_converts_to_ankaios_deleted_workload_fails() {
        let mut proto_workload = generate_test_proto_deleted_workload();
        proto_workload.dependencies.insert("workload B".into(), -1);

        assert!(ankaios::DeletedWorkload::try_from(proto_workload).is_err());
    }

    #[test]
    fn utest_converts_to_proto_added_workload() {
        let workload_spec = generate_test_workload_spec();

        let proto_workload = AddedWorkload {
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "name".to_string(),
                agent_name: "agent".to_string(),
                id: workload_spec.runtime_config.hash_config(),
            }),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    ank_base::AddCondition::AddCondRunning.into(),
                ),
                (
                    String::from("workload C"),
                    ank_base::AddCondition::AddCondSucceeded.into(),
                ),
            ]),
            restart_policy: ank_base::RestartPolicy::Always.into(),
            runtime: String::from("runtime"),
            runtime_config: workload_spec.runtime_config.clone(),
            tags: vec![ank_base::Tag {
                key: "key".into(),
                value: "value".into(),
            }],
            control_interface_access: Default::default(),
        };

        assert_eq!(AddedWorkload::from(workload_spec), proto_workload);
    }

    #[test]
    fn utest_converts_to_ankaios_added_workload() {
        let ank_workload = ankaios::WorkloadSpec {
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    ankaios::AddCondition::AddCondRunning,
                ),
                (
                    String::from("workload C"),
                    ankaios::AddCondition::AddCondSucceeded,
                ),
            ]),
            restart_policy: ankaios::RestartPolicy::Always,
            runtime: String::from("runtime"),
            instance_name: ankaios::WorkloadInstanceName::builder()
                .agent_name("agent")
                .workload_name("name")
                .build(),
            tags: vec![],
            runtime_config: String::from("some config"),
            control_interface_access: Default::default(),
        };

        let proto_workload = AddedWorkload {
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "name".to_string(),
                agent_name: "agent".to_string(),
                ..Default::default()
            }),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    ank_base::AddCondition::AddCondRunning.into(),
                ),
                (
                    String::from("workload C"),
                    ank_base::AddCondition::AddCondSucceeded.into(),
                ),
            ]),
            restart_policy: ank_base::RestartPolicy::Always.into(),
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
            control_interface_access: Default::default(),
        };

        assert_eq!(
            ankaios::WorkloadSpec::try_from(proto_workload),
            Ok(ank_workload)
        );
    }

    #[test]
    fn utest_converts_to_ankaios_added_workload_fails() {
        let proto_workload = AddedWorkload {
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "name".to_string(),
                ..Default::default()
            }),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    ank_base::AddCondition::AddCondRunning.into(),
                ),
                (String::from("workload B"), -1),
                (
                    String::from("workload C"),
                    ank_base::AddCondition::AddCondSucceeded.into(),
                ),
            ]),
            restart_policy: ank_base::RestartPolicy::Always.into(),
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
            control_interface_access: Default::default(),
        };

        assert!(ankaios::WorkloadSpec::try_from(proto_workload).is_err());
    }

    // UpdateWorkloadState tests
    const AGENT_NAME: &str = "agent_1";
    const WORKLOAD_NAME_1: &str = "workload_name_1";
    const HASH: &str = "hash_1";

    macro_rules! update_workload_state {
        (ankaios) => {
            ankaios::UpdateWorkloadState {
                workload_states: vec![workload_state!(ankaios)],
            }
        };
        (grpc_api) => {
            crate::UpdateWorkloadState {
                workload_states: vec![workload_state!(ank_base)],
            }
        };
    }

    macro_rules! workload_state {
        (ankaios) => {{
            struct HashableString(String);

            impl ankaios::ConfigHash for HashableString {
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
                    execution_state_enum: ank_base::execution_state::ExecutionStateEnum::Running(
                        ank_base::Running::Ok.into(),
                    )
                    .into(),
                    ..Default::default()
                }
                .into(),
            }
        };
    }

    #[test]
    fn utest_converts_to_proto_update_workload_state() {
        let ankaios_update_wl_state = update_workload_state!(ankaios);
        let proto_update_wl_state = update_workload_state!(grpc_api);

        assert_eq!(
            crate::UpdateWorkloadState::from(ankaios_update_wl_state),
            proto_update_wl_state
        );
    }

    #[test]
    fn utest_converts_from_proto_update_workload_state() {
        let proto_update_wl_state = update_workload_state!(grpc_api);
        let ankaios_update_wl_state = update_workload_state!(ankaios);

        assert_eq!(
            ankaios::UpdateWorkloadState::from(proto_update_wl_state),
            ankaios_update_wl_state,
        );
    }
}

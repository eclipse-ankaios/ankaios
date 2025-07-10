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
use common::objects;
use std::collections::HashMap;

// [impl->swdd~grpc-delegate-workflow-to-external-library~1]
tonic::include_proto!("grpc_api"); // The string specified here must match the proto package name

impl AgentHello {
    pub fn new(agent_name: impl Into<String>) -> Self {
        AgentHello {
            agent_name: agent_name.into(),
            protocol_version: common::ANKAIOS_VERSION.into(),
        }
    }
}

impl CommanderHello {
    pub fn new() -> Self {
        CommanderHello {
            protocol_version: common::ANKAIOS_VERSION.into(),
        }
    }
}

impl From<AgentLoadStatus> for commands::AgentLoadStatus {
    fn from(item: AgentLoadStatus) -> Self {
        commands::AgentLoadStatus {
            agent_name: item.agent_name,
            cpu_usage: item.cpu_usage.unwrap_or_default().into(),
            free_memory: item.free_memory.unwrap_or_default().into(),
        }
    }
}

impl From<commands::AgentLoadStatus> for AgentLoadStatus {
    fn from(item: commands::AgentLoadStatus) -> Self {
        AgentLoadStatus {
            agent_name: item.agent_name,
            cpu_usage: Some(item.cpu_usage.into()),
            free_memory: Some(item.free_memory.into()),
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
            files: workload
                .files
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
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
            files: workload.files.into_iter().map(Into::into).collect(),
            control_interface_access: workload.control_interface_access.into(),
        }
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
        String::from("workload_A"),
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
        from_server::FromServerEnum, generate_test_proto_deleted_workload, AddedWorkload,
        DeletedWorkload, FromServer, LogsCancelRequest, LogsRequest, UpdateWorkload,
        UpdateWorkloadState,
    };

    use api::ank_base::{self};
    use common::{
        commands,
        objects::{
            self, generate_test_rendered_workload_files, generate_test_workload_spec, ConfigHash,
        },
        test_utils::generate_test_deleted_workload,
    };

    mod ankaios {
        pub use common::{commands::*, from_server_interface::FromServer, objects::*};
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
        proto_workload.dependencies.insert("workload_B".into(), -1);

        assert!(ankaios::DeletedWorkload::try_from(proto_workload).is_err());
    }

    #[test]
    fn utest_converts_to_proto_added_workload() {
        let mut workload_spec = generate_test_workload_spec();
        workload_spec.files = generate_test_rendered_workload_files();

        let proto_workload = AddedWorkload {
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "name".to_string(),
                agent_name: "agent".to_string(),
                id: workload_spec.runtime_config.hash_config(),
            }),
            dependencies: HashMap::from([
                (
                    String::from("workload_A"),
                    ank_base::AddCondition::AddCondRunning.into(),
                ),
                (
                    String::from("workload_C"),
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
            files: vec![
                ank_base::File {
                    mount_point: "/file.json".into(),
                    file_content: Some(ank_base::file::FileContent::Data("text data".into())),
                },
                ank_base::File {
                    mount_point: "/binary_file".into(),
                    file_content: Some(ank_base::file::FileContent::BinaryData(
                        "base64_data".into(),
                    )),
                },
            ],
        };

        assert_eq!(AddedWorkload::from(workload_spec), proto_workload);
    }

    #[test]
    fn utest_converts_to_ankaios_added_workload() {
        let ank_workload = ankaios::WorkloadSpec {
            dependencies: HashMap::from([
                (
                    String::from("workload_A"),
                    ankaios::AddCondition::AddCondRunning,
                ),
                (
                    String::from("workload_C"),
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
            files: generate_test_rendered_workload_files(),
        };

        let proto_workload = AddedWorkload {
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "name".to_string(),
                agent_name: "agent".to_string(),
                ..Default::default()
            }),
            dependencies: HashMap::from([
                (
                    String::from("workload_A"),
                    ank_base::AddCondition::AddCondRunning.into(),
                ),
                (
                    String::from("workload_C"),
                    ank_base::AddCondition::AddCondSucceeded.into(),
                ),
            ]),
            restart_policy: ank_base::RestartPolicy::Always.into(),
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
            control_interface_access: Default::default(),
            files: vec![
                ank_base::File {
                    mount_point: "/file.json".into(),
                    file_content: Some(ank_base::file::FileContent::Data("text data".into())),
                },
                ank_base::File {
                    mount_point: "/binary_file".into(),
                    file_content: Some(ank_base::file::FileContent::BinaryData(
                        "base64_data".into(),
                    )),
                },
            ],
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
                    String::from("workload_A"),
                    ank_base::AddCondition::AddCondRunning.into(),
                ),
                (String::from("workload_B"), -1),
                (
                    String::from("workload_C"),
                    ank_base::AddCondition::AddCondSucceeded.into(),
                ),
            ]),
            restart_policy: ank_base::RestartPolicy::Always.into(),
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
            control_interface_access: Default::default(),
            files: Default::default(),
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

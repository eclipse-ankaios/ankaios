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

use api::ank_base::{self, WorkloadNamed};
use common::commands;
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

impl From<AgentLoadStatus> for common::objects::AgentLoadStatus {
    fn from(item: AgentLoadStatus) -> Self {
        common::objects::AgentLoadStatus {
            agent_name: item.agent_name,
            cpu_usage: item.cpu_usage.unwrap_or_default().try_into().unwrap(),
            free_memory: item.free_memory.unwrap_or_default().try_into().unwrap(),
        }
    }
}

impl From<common::objects::AgentLoadStatus> for AgentLoadStatus {
    fn from(item: common::objects::AgentLoadStatus) -> Self {
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
            workload_states: item
                .workload_states
                .into_iter()
                .map(|x| x.into())
                .collect(),
        }
    }
}

impl From<UpdateWorkloadState> for commands::UpdateWorkloadState {
    fn from(item: UpdateWorkloadState) -> Self {
        commands::UpdateWorkloadState {
            workload_states: item
                .workload_states
                .into_iter()
                .map(|x| x.try_into().unwrap())
                .collect(),
        }
    }
}

impl TryFrom<DeletedWorkload> for ank_base::DeletedWorkload {
    type Error = String;

    fn try_from(deleted_workload: DeletedWorkload) -> Result<Self, Self::Error> {
        Ok(ank_base::DeletedWorkload {
            instance_name: deleted_workload
                .instance_name
                .ok_or("No instance name")?
                .try_into()
                .unwrap(),
            dependencies: deleted_workload
                .dependencies
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<HashMap<String, ank_base::DeleteCondition>, String>>()?,
        })
    }
}

impl From<ank_base::DeletedWorkload> for DeletedWorkload {
    fn from(value: ank_base::DeletedWorkload) -> Self {
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

impl TryFrom<AddedWorkload> for WorkloadNamed {
    type Error = String;

    fn try_from(workload: AddedWorkload) -> Result<Self, String> {
        Ok(WorkloadNamed {
            instance_name: workload
                .instance_name
                .ok_or("No instance name")?
                .try_into()?,
            workload: workload.workload.ok_or("No workload")?.try_into()?,
        })
    }
}

impl From<WorkloadNamed> for AddedWorkload {
    fn from(workload: WorkloadNamed) -> Self {
        AddedWorkload {
            instance_name: Some(workload.instance_name.into()),
            workload: Some(workload.workload.into()),
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
use api::test_utils::generate_test_workload_state_with_agent;

#[cfg(test)]
fn generate_test_proto_delete_dependencies() -> HashMap<String, i32> {
    HashMap::from([(
        String::from("workload_A"),
        DeleteCondition::DelCondNotPendingNorRunning.into(),
    )])
}

#[cfg(test)]
pub fn generate_test_proto_deleted_workload() -> DeletedWorkload {
    let instance_name = api::ank_base::WorkloadInstanceNameInternal::builder()
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
    common::to_server_interface::ToServer::UpdateWorkloadState(commands::UpdateWorkloadState {
        workload_states: vec![generate_test_workload_state_with_agent(
            workload_name,
            agent_name,
            api::ank_base::ExecutionStateInternal::failed("additional_info"),
        )],
    })
}

#[cfg(test)]
mod tests {
    // use crate::AddedWorkload;
    // use api::ank_base::{WorkloadInternal, WorkloadNamed};
    // use api::test_utils::{generate_test_workload, generate_test_workload_files};
    // use std::collections::HashMap;

    use crate::{DeletedWorkload, generate_test_proto_deleted_workload};

    use api::ank_base::{self, ConfigHash, ExecutionStateInternal, WorkloadStateInternal};
    use api::test_utils::generate_test_deleted_workload;

    mod ankaios {
        pub use common::commands::*;
    }

    ///////////////////////////////////////////////////////////////////////////
    // Workload tests
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
            ank_base::DeletedWorkload::try_from(proto_workload),
            Ok(workload)
        );
    }

    #[test]
    fn utest_converts_to_ankaios_deleted_workload_fails() {
        let mut proto_workload = generate_test_proto_deleted_workload();
        proto_workload.dependencies.insert("workload_B".into(), -1);

        assert!(ank_base::DeletedWorkload::try_from(proto_workload).is_err());
    }

    // TODO #313 Check if the following test is still needed
    // #[test]
    // fn utest_converts_to_proto_added_workload() {
    //     let mut workload_spec = generate_test_workload();
    //     workload_spec.files = generate_test_workload_files();

    //     let proto_workload = AddedWorkload {
    //         instance_name: Some(ank_base::WorkloadInstanceName {
    //             workload_name: "name".to_string(),
    //             agent_name: "agent".to_string(),
    //             id: workload_spec.runtime_config.hash_config(),
    //         }),
    //         dependencies: HashMap::from([
    //             (
    //                 String::from("workload_A"),
    //                 ank_base::AddCondition::AddCondRunning.into(),
    //             ),
    //             (
    //                 String::from("workload_C"),
    //                 ank_base::AddCondition::AddCondSucceeded.into(),
    //             ),
    //         ]),
    //         restart_policy: ank_base::RestartPolicy::Always.into(),
    //         runtime: String::from("runtime"),
    //         runtime_config: workload_spec.runtime_config.clone(),
    //         tags: HashMap::from([("key".into(), "value".into())]),
    //         control_interface_access: Some(Default::default()),
    //         files: vec![
    //             ank_base::File {
    //                 mount_point: "/file.json".into(),
    //                 file_content: Some(ank_base::file::FileContent::Data("text data".into())),
    //             },
    //             ank_base::File {
    //                 mount_point: "/binary_file".into(),
    //                 file_content: Some(ank_base::file::FileContent::BinaryData(
    //                     "base64_data".into(),
    //                 )),
    //             },
    //         ],
    //     };

    //     assert_eq!(AddedWorkload::from(workload_spec), proto_workload);
    // }

    // TODO #313 Check if the following test is still needed
    // #[test]
    // fn utest_converts_to_ankaios_added_workload() {
    //     let agent_name = "agent";
    //     let ank_workload = WorkloadInternal {
    //         agent: agent_name.to_string(),
    //         dependencies: HashMap::from([
    //             (
    //                 String::from("workload_A"),
    //                 ank_base::AddCondition::AddCondRunning,
    //             ),
    //             (
    //                 String::from("workload_C"),
    //                 ank_base::AddCondition::AddCondSucceeded,
    //             ),
    //         ])
    //         .into(),
    //         restart_policy: ank_base::RestartPolicy::Always,
    //         runtime: String::from("runtime"),
    //         instance_name: api::ank_base::WorkloadInstanceNameInternal::builder()
    //             .agent_name(agent_name)
    //             .workload_name("name")
    //             .build(),
    //         tags: Default::default(),
    //         runtime_config: String::from("some config"),
    //         control_interface_access: Default::default(),
    //         files: generate_test_workload_files(),
    //         configs: Default::default(),
    //     };

    //     let proto_workload = AddedWorkload {
    //         instance_name: Some(ank_base::WorkloadInstanceName {
    //             workload_name: "name".to_string(),
    //             agent_name: "agent".to_string(),
    //             ..Default::default()
    //         }),
    //         dependencies: HashMap::from([
    //             (
    //                 String::from("workload_A"),
    //                 ank_base::AddCondition::AddCondRunning.into(),
    //             ),
    //             (
    //                 String::from("workload_C"),
    //                 ank_base::AddCondition::AddCondSucceeded.into(),
    //             ),
    //         ]),
    //         restart_policy: ank_base::RestartPolicy::Always.into(),
    //         runtime: String::from("runtime"),
    //         runtime_config: String::from("some config"),
    //         tags: HashMap::new(),
    //         control_interface_access: Default::default(),
    //         files: vec![
    //             ank_base::File {
    //                 mount_point: "/file.json".into(),
    //                 file_content: Some(ank_base::file::FileContent::Data("text data".into())),
    //             },
    //             ank_base::File {
    //                 mount_point: "/binary_file".into(),
    //                 file_content: Some(ank_base::file::FileContent::BinaryData(
    //                     "base64_data".into(),
    //                 )),
    //             },
    //         ],
    //     };

    //     assert_eq!(WorkloadInternal::try_from(proto_workload), Ok(ank_workload));
    // }

    // TODO #313 Check if the following test is still needed
    // #[test]
    // fn utest_converts_to_ankaios_added_workload_fails() {
    //     let proto_workload = AddedWorkload {
    //         instance_name: Some(ank_base::WorkloadInstanceName {
    //             workload_name: "name".to_string(),
    //             ..Default::default()
    //         }),
    //         dependencies: HashMap::from([
    //             (
    //                 String::from("workload_A"),
    //                 ank_base::AddCondition::AddCondRunning.into(),
    //             ),
    //             (String::from("workload_B"), -1),
    //             (
    //                 String::from("workload_C"),
    //                 ank_base::AddCondition::AddCondSucceeded.into(),
    //             ),
    //         ]),
    //         restart_policy: ank_base::RestartPolicy::Always.into(),
    //         runtime: String::from("runtime"),
    //         runtime_config: String::from("some config"),
    //         tags: HashMap::new(),
    //         control_interface_access: Default::default(),
    //         files: Default::default(),
    //     };

    //     assert!(WorkloadInternal::try_from(proto_workload).is_err());
    // }

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

            impl ConfigHash for HashableString {
                fn hash_config(&self) -> String {
                    self.0.clone()
                }
            }
            WorkloadStateInternal {
                instance_name: api::ank_base::WorkloadInstanceNameInternal::builder()
                    .workload_name(WORKLOAD_NAME_1)
                    .config(&HashableString(HASH.into()))
                    .agent_name(AGENT_NAME)
                    .build(),
                execution_state: ExecutionStateInternal::running(),
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

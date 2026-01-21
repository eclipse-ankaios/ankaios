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

use ankaios_api::ank_base::{self, Tags, WorkloadNamed};
use common::commands;
use std::collections::HashMap;

// [impl->swdd~grpc-delegate-workflow-to-external-library~1]
tonic::include_proto!("grpc_api"); // The string specified here must match the proto package name

impl AgentHello {
    pub fn new(agent_name: impl Into<String>, tags: HashMap<String, String>) -> Self {
        AgentHello {
            agent_name: agent_name.into(),
            protocol_version: common::ANKAIOS_VERSION.into(),
            tags: Some(Tags { tags }),
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

impl From<AgentLoadStatus> for common::commands::AgentLoadStatus {
    fn from(item: AgentLoadStatus) -> Self {
        common::commands::AgentLoadStatus {
            agent_name: item.agent_name,
            cpu_usage: item
                .cpu_usage
                .unwrap_or_default()
                .try_into()
                .unwrap_or_default(),
            free_memory: item
                .free_memory
                .unwrap_or_default()
                .try_into()
                .unwrap_or_default(),
        }
    }
}

impl From<common::commands::AgentLoadStatus> for AgentLoadStatus {
    fn from(item: common::commands::AgentLoadStatus) -> Self {
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
use ankaios_api::ank_base::{ExecutionStateSpec, WorkloadInstanceNameSpec};
#[cfg(test)]
use ankaios_api::test_utils::{fixtures, generate_test_workload_state_with_agent};
#[cfg(test)]
use common::to_server_interface;

#[cfg(test)]
fn generate_test_proto_delete_dependencies() -> HashMap<String, i32> {
    HashMap::from([(
        String::from(fixtures::WORKLOAD_NAMES[0]),
        DeleteCondition::DelCondNotPendingNorRunning.into(),
    )])
}

#[cfg(test)]
pub fn generate_test_proto_deleted_workload() -> DeletedWorkload {
    let instance_name = WorkloadInstanceNameSpec::builder()
        .agent_name(fixtures::AGENT_NAMES[0])
        .workload_name(fixtures::WORKLOAD_NAMES[1])
        .config(&String::from(fixtures::RUNTIME_CONFIGS[0]))
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
) -> to_server_interface::ToServer {
    to_server_interface::ToServer::UpdateWorkloadState(commands::UpdateWorkloadState {
        workload_states: vec![generate_test_workload_state_with_agent(
            workload_name,
            agent_name,
            ExecutionStateSpec::failed("additional_info"),
        )],
    })
}

#[cfg(test)]
mod tests {
    use crate::{AddedWorkload, DeletedWorkload, generate_test_proto_deleted_workload};

    use ankaios_api::ank_base::{
        self, AddCondition, Dependencies, ExecutionStateSpec, WorkloadNamed, WorkloadState,
    };
    use ankaios_api::test_utils::{
        fixtures, generate_test_deleted_workload_with_params, generate_test_workload,
        generate_test_workload_named, generate_test_workload_state,
    };
    use common::commands;
    use std::collections::HashMap;

    ///////////////////////////////////////////////////////////////////////////
    // Workload tests
    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn utest_converts_to_proto_deleted_workload() {
        let proto_workload = generate_test_proto_deleted_workload();
        let workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0].to_string(),
            fixtures::WORKLOAD_NAMES[1].to_string(),
        );

        assert_eq!(DeletedWorkload::from(workload), proto_workload);
    }

    #[test]
    fn utest_converts_to_ankaios_deleted_workload() {
        let proto_workload = generate_test_proto_deleted_workload();
        let workload = generate_test_deleted_workload_with_params(
            fixtures::AGENT_NAMES[0].to_string(),
            fixtures::WORKLOAD_NAMES[1].to_string(),
        );

        assert_eq!(
            ank_base::DeletedWorkload::try_from(proto_workload),
            Ok(workload)
        );
    }

    #[test]
    fn utest_converts_to_ankaios_deleted_workload_fails() {
        let mut proto_workload = generate_test_proto_deleted_workload();
        proto_workload
            .dependencies
            .insert(fixtures::WORKLOAD_NAMES[0].into(), -1);

        assert!(ank_base::DeletedWorkload::try_from(proto_workload).is_err());
    }

    #[test]
    fn utest_converts_to_proto_added_workload() {
        let workload = generate_test_workload_named();

        let proto_workload = AddedWorkload {
            instance_name: Some(workload.instance_name.clone().into()),
            workload: Some(generate_test_workload().into()),
        };

        assert_eq!(AddedWorkload::from(workload), proto_workload);
    }

    #[test]
    fn utest_converts_to_ankaios_added_workload() {
        let workload = generate_test_workload_named();

        let proto_workload = AddedWorkload {
            instance_name: Some(workload.instance_name.clone().into()),
            workload: Some(generate_test_workload().into()),
        };

        assert_eq!(WorkloadNamed::try_from(proto_workload), Ok(workload));
    }

    #[test]
    fn utest_converts_to_ankaios_added_workload_fails() {
        let mut proto_workload = AddedWorkload {
            instance_name: None,
            workload: Some(generate_test_workload().into()),
        };
        if let Some(workload) = proto_workload.workload.as_mut() {
            workload.dependencies = Some(Dependencies {
                dependencies: HashMap::from([
                    (
                        String::from(fixtures::WORKLOAD_NAMES[0]),
                        AddCondition::AddCondRunning.into(),
                    ),
                    (String::from(fixtures::WORKLOAD_NAMES[1]), -1), // Invalid value for dependency
                    (
                        String::from(fixtures::WORKLOAD_NAMES[2]),
                        AddCondition::AddCondSucceeded.into(),
                    ),
                ]),
            })
        }

        assert!(WorkloadNamed::try_from(proto_workload).is_err());
    }

    macro_rules! update_workload_state {
        (ankaios) => {
            commands::UpdateWorkloadState {
                workload_states: vec![generate_test_workload_state(
                    fixtures::WORKLOAD_NAMES[0],
                    ExecutionStateSpec::running(),
                )],
            }
        };
        (grpc_api) => {
            crate::UpdateWorkloadState {
                workload_states: vec![WorkloadState::from(generate_test_workload_state(
                    fixtures::WORKLOAD_NAMES[0],
                    ExecutionStateSpec::running(),
                ))],
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
            commands::UpdateWorkloadState::from(proto_update_wl_state),
            ankaios_update_wl_state,
        );
    }
}

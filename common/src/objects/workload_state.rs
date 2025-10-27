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

use serde::{Deserialize, Serialize};

use api::ank_base::{self, ExecutionStateInternal, WorkloadInstanceNameInternal};

use crate::std_extensions::UnreachableOption;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkloadState {
    // [impl->swdd~common-workload-state-identification~1]
    pub instance_name: WorkloadInstanceNameInternal,
    pub execution_state: ExecutionStateInternal,
}

impl From<WorkloadState> for ank_base::WorkloadState {
    fn from(item: WorkloadState) -> Self {
        ank_base::WorkloadState {
            instance_name: Some(item.instance_name.into()),
            execution_state: Some(item.execution_state.into()),
        }
    }
}

impl From<ank_base::WorkloadState> for WorkloadState {
    fn from(item: ank_base::WorkloadState) -> Self {
        WorkloadState {
            instance_name: item
                .instance_name
                .unwrap_or_unreachable()
                .try_into()
                .unwrap(),
            execution_state: item
                .execution_state
                .unwrap_or(ank_base::ExecutionState {
                    additional_info: "Cannot covert, proceeding with unknown".to_owned(),
                    execution_state_enum: Some(
                        ank_base::execution_state::ExecutionStateEnum::Failed(
                            ank_base::Failed::Unknown as i32,
                        ),
                    ),
                })
                .try_into()
                .unwrap(),
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

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_agent(
    workload_name: &str,
    agent_name: &str,
    execution_state: ExecutionStateInternal,
) -> WorkloadState {
    WorkloadState {
        instance_name: WorkloadInstanceNameInternal::builder()
            .workload_name(workload_name)
            .agent_name(agent_name)
            .config(&"config".to_string())
            .build(),
        execution_state,
    }
}
#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_workload_spec(
    workload_spec: &api::ank_base::WorkloadInternal,
    execution_state: ExecutionStateInternal,
) -> WorkloadState {
    WorkloadState {
        instance_name: workload_spec.instance_name.clone(),
        execution_state,
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state(
    workload_name: &str,
    execution_state: ExecutionStateInternal,
) -> WorkloadState {
    generate_test_workload_state_with_agent(workload_name, "agent_name", execution_state)
}

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
#[cfg(test)]
mod tests {
    use crate::objects::WorkloadState;
    use api::ank_base::{self, ExecutionStateInternal, WorkloadInstanceNameInternal};

    // [utest->swdd~common-workload-state-identification~1]
    #[test]
    fn utest_converts_to_proto_workload_state() {
        let additional_info = "some additional info";
        let ankaios_wl_state = WorkloadState {
            execution_state: ExecutionStateInternal::starting(additional_info),
            instance_name: WorkloadInstanceNameInternal::builder()
                .workload_name("john")
                .agent_name("strange")
                .build(),
        };

        let proto_wl_state = ank_base::WorkloadState {
            execution_state: Some(ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Pending(
                    ank_base::Pending::Starting.into(),
                )),
            }),
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "john".to_string(),
                agent_name: "strange".to_string(),
                ..Default::default()
            }),
        };

        assert_eq!(
            ank_base::WorkloadState::from(ankaios_wl_state),
            proto_wl_state
        );
    }

    // [utest->swdd~common-workload-state-identification~1]
    #[test]
    fn utest_converts_to_ankaios_workload_state() {
        let ankaios_wl_state = WorkloadState {
            execution_state: ExecutionStateInternal::running(),
            instance_name: WorkloadInstanceNameInternal::builder()
                .workload_name("john")
                .agent_name("strange")
                .build(),
        };

        let proto_wl_state = ank_base::WorkloadState {
            execution_state: Some(ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Running(
                    ank_base::Running::Ok.into(),
                )),
            }),
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "john".to_string(),
                agent_name: "strange".to_string(),
                ..Default::default()
            }),
        };

        assert_eq!(WorkloadState::from(proto_wl_state), ankaios_wl_state);
    }
}

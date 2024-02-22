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

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::helpers::serialize_to_ordered_map;

use super::{
    AccessRights, AddCondition, CompleteState, Cronjob, State, Tag, UpdateStrategy,
    WorkloadInstanceName, WorkloadSpec, WorkloadState,
};

#[derive(Debug, Serialize, Default, Deserialize, Clone, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
struct StoredWorkloadSpec {
    pub agent: String,
    pub tags: Vec<Tag>,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub dependencies: HashMap<String, AddCondition>,
    pub update_strategy: UpdateStrategy,
    pub restart: bool,
    pub access_rights: AccessRights,
    pub runtime: String,
    pub runtime_config: String,
}

impl From<(String, StoredWorkloadSpec)> for WorkloadSpec {
    fn from((name, spec): (String, StoredWorkloadSpec)) -> Self {
        WorkloadSpec {
            instance_name: WorkloadInstanceName::builder()
                .workload_name(name)
                .agent_name(spec.agent)
                .config(&spec.runtime_config)
                .build(),
            tags: spec.tags,
            dependencies: spec.dependencies,
            update_strategy: spec.update_strategy,
            restart: spec.restart,
            access_rights: spec.access_rights,
            runtime: spec.runtime,
            runtime_config: spec.runtime_config,
        }
    }
}

impl From<WorkloadSpec> for StoredWorkloadSpec {
    fn from(value: WorkloadSpec) -> Self {
        StoredWorkloadSpec {
            runtime: value.runtime,
            agent: value.instance_name.agent_name().to_owned(),
            restart: value.restart,
            dependencies: value.dependencies,
            update_strategy: value.update_strategy,
            access_rights: value.access_rights,
            tags: value.tags,
            runtime_config: value.runtime_config,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ExternalState {
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub workloads: HashMap<String, StoredWorkloadSpec>,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub configs: HashMap<String, String>,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub cron_jobs: HashMap<String, Cronjob>,
}

impl From<ExternalState> for State {
    fn from(value: ExternalState) -> Self {
        State {
            workloads: value
                .workloads
                .into_iter()
                .map(|(name, spec)| (name.clone(), (name, spec).into()))
                .collect(),
            configs: value.configs,
            cron_jobs: value.cron_jobs,
        }
    }
}

impl From<State> for ExternalState {
    fn from(value: State) -> Self {
        ExternalState {
            workloads: value
                .workloads
                .into_iter()
                .map(|(name, spec)| (name, spec.into()))
                .collect(),
            configs: value.configs,
            cron_jobs: value.cron_jobs,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ExternalCompleteState {
    pub startup_state: ExternalState,
    pub current_state: ExternalState,
    pub workload_states: Vec<WorkloadState>,
}

impl From<CompleteState> for ExternalCompleteState {
    fn from(value: CompleteState) -> Self {
        ExternalCompleteState {
            startup_state: value.startup_state.into(),
            current_state: value.current_state.into(),
            workload_states: value.workload_states,
        }
    }
}

impl From<ExternalCompleteState> for CompleteState {
    fn from(value: ExternalCompleteState) -> Self {
        CompleteState {
            startup_state: value.startup_state.into(),
            current_state: value.current_state.into(),
            workload_states: value.workload_states,
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

// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {

    // #[test]
    // fn utest_serialize_state_into_ordered_output() {
    //     // input: random sorted state
    //     let ankaios_state = generate_test_state();

    //     // serialize to sorted output
    //     let sorted_state_string =
    //         serde_yaml::to_string(&ExternalState::from(ankaios_state)).unwrap();

    //     let index_workload1 = sorted_state_string.find("workload_name_1").unwrap();
    //     let index_workload2 = sorted_state_string.find("workload_name_2").unwrap();
    //     assert!(
    //         index_workload1 < index_workload2,
    //         "expected sorted workloads."
    //     );

    //     let index_config1 = sorted_state_string.find("key1").unwrap();
    //     let index_config2 = sorted_state_string.find("key2").unwrap();
    //     assert!(index_config1 < index_config2, "expected sorted configs.");

    //     let index_cron1 = sorted_state_string.find("cronjob1").unwrap();
    //     let index_cron2 = sorted_state_string.find("cronjob2").unwrap();
    //     assert!(index_cron1 < index_cron2, "expected sorted cronjobs.");
    // }
}

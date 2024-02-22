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

use api::proto;
use serde::{Deserialize, Serialize};

use crate::helpers::serialize_to_ordered_map;

use super::{AccessRights, AddCondition, Tag, UpdateStrategy, WorkloadInstanceName, WorkloadSpec};

#[derive(Debug, Serialize, Default, Deserialize, Clone, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredWorkloadSpec {
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

impl TryFrom<proto::Workload> for StoredWorkloadSpec {
    type Error = String;

    fn try_from(value: proto::Workload) -> Result<Self, String> {
        Ok(StoredWorkloadSpec {
            agent: value.agent,
            tags: value.tags.into_iter().map(|x| x.into()).collect(),
            dependencies: value
                .dependencies
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<HashMap<String, AddCondition>, String>>()?,
            update_strategy: value.update_strategy.try_into()?,
            restart: value.restart,
            access_rights: value.access_rights.unwrap_or_default().try_into()?,
            runtime: value.runtime,
            runtime_config: value.runtime_config,
        })
    }
}

impl From<StoredWorkloadSpec> for proto::Workload {
    fn from(workload: StoredWorkloadSpec) -> Self {
        proto::Workload {
            agent: workload.agent,
            dependencies: workload
                .dependencies
                .into_iter()
                .map(|(k, v)| (k, v as i32))
                .collect(),
            restart: workload.restart,
            update_strategy: workload.update_strategy as i32,
            access_rights: if workload.access_rights.is_empty() {
                None
            } else {
                Some(workload.access_rights.into())
            },
            runtime: workload.runtime,
            runtime_config: workload.runtime_config,
            tags: workload.tags.into_iter().map(|x| x.into()).collect(),
        }
    }
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_stored_workload_spec_with_config(
    agent: impl Into<String>,
    runtime_name: impl Into<String>,
    runtime_config: impl Into<String>,
) -> crate::objects::StoredWorkloadSpec {
    StoredWorkloadSpec {
        agent: agent.into(),
        dependencies: HashMap::from([
            (String::from("workload A"), AddCondition::AddCondRunning),
            (String::from("workload C"), AddCondition::AddCondSucceeded),
        ]),
        update_strategy: UpdateStrategy::Unspecified,
        restart: true,
        access_rights: AccessRights::default(),
        runtime: runtime_name.into(),
        tags: vec![Tag {
            key: "key".into(),
            value: "value".into(),
        }],
        runtime_config: runtime_config.into(),
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_stored_workload_spec(
    agent: impl Into<String>,
    runtime_name: impl Into<String>,
) -> crate::objects::StoredWorkloadSpec {
    generate_test_stored_workload_spec_with_config(
        agent,
        runtime_name,
        "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
        .to_owned()
    )
}

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

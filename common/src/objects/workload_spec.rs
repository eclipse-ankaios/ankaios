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
use std::collections::HashMap;

use api::proto;

use crate::helpers::serialize_to_ordered_map;
use crate::objects::AccessRights;
use crate::objects::Tag;

pub type WorkloadCollection = Vec<WorkloadSpec>;
pub type DeletedWorkloadCollection = Vec<DeletedWorkload>;
// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DeletedWorkload {
    pub agent: String,
    pub name: String,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub dependencies: HashMap<String, ExpectedState>,
}

impl TryFrom<(String, proto::DeletedWorkload)> for DeletedWorkload {
    type Error = String;

    fn try_from(
        (agent, deleted_workload): (String, proto::DeletedWorkload),
    ) -> Result<Self, Self::Error> {
        Ok(DeletedWorkload {
            agent,
            name: deleted_workload.name,
            dependencies: deleted_workload
                .dependencies
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<HashMap<String, ExpectedState>, String>>()?,
        })
    }
}

impl From<DeletedWorkload> for proto::DeletedWorkload {
    fn from(value: DeletedWorkload) -> Self {
        proto::DeletedWorkload {
            name: value.name,
            dependencies: value
                .dependencies
                .into_iter()
                .map(|(k, v)| (k, v as i32))
                .collect(),
        }
    }
}

// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkloadSpec {
    pub agent: String,
    pub name: String,
    pub tags: Vec<Tag>,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub dependencies: HashMap<String, ExpectedState>,
    pub update_strategy: UpdateStrategy,
    pub restart: bool,
    pub access_rights: AccessRights,
    pub runtime: String,
    pub runtime_config: String,
}

impl TryFrom<(String, proto::AddedWorkload)> for WorkloadSpec {
    type Error = String;

    fn try_from((agent, workload): (String, proto::AddedWorkload)) -> Result<Self, String> {
        Ok(WorkloadSpec {
            dependencies: workload
                .dependencies
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<HashMap<String, ExpectedState>, String>>()?,
            update_strategy: workload.update_strategy.try_into()?,
            restart: workload.restart,
            access_rights: workload.access_rights.unwrap_or_default().try_into()?,
            runtime: workload.runtime,
            name: workload.name,
            agent,
            tags: workload.tags.into_iter().map(|x| x.into()).collect(),
            runtime_config: workload.runtime_config,
        })
    }
}

impl TryFrom<(String, proto::Workload)> for WorkloadSpec {
    type Error = String;

    fn try_from((name, workload): (String, proto::Workload)) -> Result<Self, Self::Error> {
        Ok(WorkloadSpec {
            dependencies: workload
                .dependencies
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<HashMap<String, ExpectedState>, String>>()?,
            update_strategy: workload.update_strategy.try_into()?,
            restart: workload.restart,
            access_rights: workload.access_rights.unwrap_or_default().try_into()?,
            runtime: workload.runtime,
            name,
            agent: workload.agent,
            tags: workload.tags.into_iter().map(|x| x.into()).collect(),
            runtime_config: workload.runtime_config,
        })
    }
}

impl From<WorkloadSpec> for proto::Workload {
    fn from(workload: WorkloadSpec) -> Self {
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

impl From<WorkloadSpec> for proto::AddedWorkload {
    fn from(workload: WorkloadSpec) -> Self {
        proto::AddedWorkload {
            name: workload.name,
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

pub type AgentWorkloadMap = HashMap<String, (WorkloadCollection, DeletedWorkloadCollection)>;

pub fn get_workloads_per_agent(
    added_workloads: WorkloadCollection,
    deleted_workloads: DeletedWorkloadCollection,
) -> AgentWorkloadMap {
    let mut agent_workloads: AgentWorkloadMap = HashMap::new();

    for added_workload in added_workloads {
        if let Some((added_workload_vector, _)) = agent_workloads.get_mut(&added_workload.agent) {
            added_workload_vector.push(added_workload);
        } else {
            agent_workloads.insert(added_workload.agent.clone(), (vec![added_workload], vec![]));
        }
    }

    for deleted_workload in deleted_workloads {
        if let Some((_, deleted_workload_vector)) = agent_workloads.get_mut(&deleted_workload.agent)
        {
            deleted_workload_vector.push(deleted_workload);
        } else {
            agent_workloads.insert(
                deleted_workload.agent.clone(),
                (vec![], vec![deleted_workload]),
            );
        }
    }

    agent_workloads
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpectedState {
    Stopped = 0,
    Running,
}

impl TryFrom<i32> for ExpectedState {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            x if x == ExpectedState::Stopped as i32 => Ok(ExpectedState::Stopped),
            x if x == ExpectedState::Running as i32 => Ok(ExpectedState::Running),
            _ => Err(format!(
                "Received an unknown value '{value}' as ExpectedState."
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdateStrategy {
    #[default]
    Unspecified = 0,
    AtLeastOnce,
    AtMostOnce,
}

impl TryFrom<i32> for UpdateStrategy {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            x if x == UpdateStrategy::Unspecified as i32 => Ok(UpdateStrategy::Unspecified),
            x if x == UpdateStrategy::AtLeastOnce as i32 => Ok(UpdateStrategy::AtLeastOnce),
            x if x == UpdateStrategy::AtMostOnce as i32 => Ok(UpdateStrategy::AtMostOnce),
            _ => Err(format!(
                "Received an unknown value '{value}' as UpdateStrategy."
            )),
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

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {
    use api::proto;
    use std::collections::HashMap;

    use crate::objects::*;
    use crate::test_utils::*;

    #[test]
    fn utest_converts_to_proto_deleted_workload() {
        let proto_workload = generate_test_proto_deleted_workload();
        let workload =
            generate_test_deleted_workload("agent X".to_string(), "workload X".to_string());

        assert_eq!(proto::DeletedWorkload::from(workload), proto_workload);
    }

    #[test]
    fn utest_converts_to_ankaios_deleted_workload() {
        let agent = "agent X";

        let proto_workload = generate_test_proto_deleted_workload();
        let workload = generate_test_deleted_workload(agent.to_string(), "workload X".to_string());

        assert_eq!(
            DeletedWorkload::try_from((agent.to_string(), proto_workload)),
            Ok(workload)
        );
    }

    #[test]
    fn utest_converts_to_ankaios_deleted_workload_fails() {
        let agent = "agent X";

        let mut proto_workload = generate_test_proto_deleted_workload();
        proto_workload.dependencies.insert("workload B".into(), -1);

        assert!(DeletedWorkload::try_from((agent.to_string(), proto_workload)).is_err());
    }

    #[test]
    fn utest_converts_to_proto_added_workload() {
        let workload = generate_test_workload_spec();

        let proto_workload = proto::AddedWorkload {
            name: String::from("name"),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    proto::ExpectedState::Running.into(),
                ),
                (
                    String::from("workload C"),
                    proto::ExpectedState::Stopped.into(),
                ),
            ]),
            restart: true,
            update_strategy: proto::UpdateStrategy::Unspecified.into(),
            access_rights: None,
            runtime: String::from("runtime"),
            runtime_config: workload.runtime_config.clone(),
            tags: vec![proto::Tag {
                key: "key".into(),
                value: "value".into(),
            }],
        };

        assert_eq!(proto::AddedWorkload::from(workload), proto_workload);
    }

    #[test]
    fn utest_converts_to_proto_workload() {
        let workload = generate_test_workload_spec();

        let proto_workload = generate_test_proto_workload();

        assert_eq!(proto::Workload::from(workload), proto_workload);
    }

    #[test]
    fn utest_converts_to_ankaios_workload() {
        let workload = WorkloadSpec {
            dependencies: HashMap::from([
                (String::from("workload A"), ExpectedState::Running),
                (String::from("workload C"), ExpectedState::Stopped),
            ]),
            update_strategy: UpdateStrategy::Unspecified,
            restart: true,
            access_rights: AccessRights {
                allow: vec![],
                deny: vec![],
            },
            runtime: String::from("runtime"),
            name: String::from("name"),
            agent: String::from("agent"),
            tags: vec![],
            runtime_config: String::from("some config"),
        };

        let proto_workload = proto::Workload {
            agent: String::from("agent"),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    proto::ExpectedState::Running.into(),
                ),
                (
                    String::from("workload C"),
                    proto::ExpectedState::Stopped.into(),
                ),
            ]),
            restart: true,
            update_strategy: proto::UpdateStrategy::Unspecified.into(),
            access_rights: None,
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
        };

        assert_eq!(
            WorkloadSpec::try_from(("name".to_string(), proto_workload)),
            Ok(workload)
        );
    }

    #[test]
    fn utest_converts_to_ankaios_workload_fails() {
        let proto_workload = proto::Workload {
            agent: String::from("agent"),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    proto::ExpectedState::Running.into(),
                ),
                (String::from("workload B"), -1),
                (
                    String::from("workload C"),
                    proto::ExpectedState::Stopped.into(),
                ),
            ]),
            restart: true,
            update_strategy: proto::UpdateStrategy::Unspecified.into(),
            access_rights: None,
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
        };

        assert!(WorkloadSpec::try_from(("name".to_string(), proto_workload)).is_err());
    }

    #[test]
    fn utest_converts_to_ankaios_added_workload() {
        let workload = WorkloadSpec {
            dependencies: HashMap::from([
                (String::from("workload A"), ExpectedState::Running),
                (String::from("workload C"), ExpectedState::Stopped),
            ]),
            update_strategy: UpdateStrategy::Unspecified,
            restart: true,
            access_rights: AccessRights {
                allow: vec![],
                deny: vec![],
            },
            runtime: String::from("runtime"),
            name: String::from("name"),
            agent: String::from("agent"),
            tags: vec![],
            runtime_config: String::from("some config"),
        };

        let proto_workload = proto::AddedWorkload {
            name: String::from("name"),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    proto::ExpectedState::Running.into(),
                ),
                (
                    String::from("workload C"),
                    proto::ExpectedState::Stopped.into(),
                ),
            ]),
            restart: true,
            update_strategy: proto::UpdateStrategy::Unspecified.into(),
            access_rights: None,
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
        };

        assert_eq!(
            WorkloadSpec::try_from(("agent".to_string(), proto_workload)),
            Ok(workload)
        );
    }

    #[test]
    fn utest_converts_to_ankaios_added_workload_fails() {
        let proto_workload = proto::AddedWorkload {
            name: String::from("name"),
            dependencies: HashMap::from([
                (
                    String::from("workload A"),
                    proto::ExpectedState::Running.into(),
                ),
                (String::from("workload B"), -1),
                (
                    String::from("workload C"),
                    proto::ExpectedState::Stopped.into(),
                ),
            ]),
            restart: true,
            update_strategy: proto::UpdateStrategy::Unspecified.into(),
            access_rights: None,
            runtime: String::from("runtime"),
            runtime_config: String::from("some config"),
            tags: vec![],
        };

        assert!(WorkloadSpec::try_from(("agent".to_string(), proto_workload)).is_err());
    }

    #[test]
    fn utest_get_workloads_per_agent_one_agent_one_workload() {
        let added_workloads = vec![
            generate_test_workload_spec_with_param(
                "agent1".to_string(),
                "name 1".to_string(),
                "runtime1".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent1".to_string(),
                "name 2".to_string(),
                "runtime2".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent2".to_string(),
                "name 3".to_string(),
                "runtime3".to_string(),
            ),
        ];

        let deleted_workloads = vec![
            generate_test_deleted_workload("agent1".to_string(), "workload 8".to_string()),
            generate_test_deleted_workload("agent4".to_string(), "workload 9".to_string()),
        ];

        let workload_map = get_workloads_per_agent(added_workloads, deleted_workloads);
        assert_eq!(workload_map.len(), 3);

        let (agent1_added_workloads, agent1_deleted_workloads) =
            workload_map.get("agent1").unwrap();
        assert_eq!(agent1_added_workloads.len(), 2);
        assert_eq!(agent1_deleted_workloads.len(), 1);

        let workload1 = &agent1_added_workloads[0];
        let workload2 = &agent1_added_workloads[1];
        assert_eq!(workload1.agent, "agent1");
        assert_eq!(workload1.runtime, "runtime1");
        assert_eq!(workload2.agent, "agent1");
        assert_eq!(workload2.runtime, "runtime2");

        let deleted_workload1 = &agent1_deleted_workloads[0];
        assert_eq!(deleted_workload1.agent, "agent1");
        assert_eq!(deleted_workload1.name, "workload 8");

        let (agent2_added_workloads, agent2_deleted_workloads) =
            workload_map.get("agent2").unwrap();
        assert_eq!(agent2_added_workloads.len(), 1);
        assert_eq!(agent2_deleted_workloads.len(), 0);

        let workload3 = &agent2_added_workloads[0];
        assert_eq!(workload3.agent, "agent2");
        assert_eq!(workload3.runtime, "runtime3");

        assert!(workload_map.get("agent3").is_none());

        let (agent4_added_workloads, agent4_deleted_workloads) =
            workload_map.get("agent4").unwrap();
        assert_eq!(agent4_added_workloads.len(), 0);
        assert_eq!(agent4_deleted_workloads.len(), 1);

        let workload3 = &agent4_deleted_workloads[0];
        assert_eq!(workload3.agent, "agent4");
        assert_eq!(workload3.name, "workload 9");
    }

    #[test]
    fn utest_expected_state_from_int() {
        assert_eq!(ExpectedState::try_from(0).unwrap(), ExpectedState::Stopped);
        assert_eq!(ExpectedState::try_from(1).unwrap(), ExpectedState::Running);
        assert_eq!(
            ExpectedState::try_from(100),
            Err::<ExpectedState, String>(
                "Received an unknown value '100' as ExpectedState.".to_string()
            )
        );
    }

    #[test]
    fn utest_update_strategy_from_int() {
        assert_eq!(
            UpdateStrategy::try_from(0).unwrap(),
            UpdateStrategy::Unspecified
        );
        assert_eq!(
            UpdateStrategy::try_from(1).unwrap(),
            UpdateStrategy::AtLeastOnce
        );
        assert_eq!(
            UpdateStrategy::try_from(2).unwrap(),
            UpdateStrategy::AtMostOnce
        );
        assert_eq!(
            UpdateStrategy::try_from(100),
            Err::<UpdateStrategy, String>(
                "Received an unknown value '100' as UpdateStrategy.".to_string()
            )
        );
    }

    #[test]
    fn utest_serialize_deleted_workload_into_ordered_output() {
        let deleted_workload =
            generate_test_deleted_workload("agent X".to_string(), "workload X".to_string());

        let serialized_deleted_workload = serde_yaml::to_string(&deleted_workload).unwrap();
        let indices = [
            serialized_deleted_workload.find("workload A").unwrap(),
            serialized_deleted_workload.find("workload C").unwrap(),
        ];
        assert!(
            indices.windows(2).all(|window| window[0] < window[1]),
            "expected ordered dependencies."
        );
    }
}

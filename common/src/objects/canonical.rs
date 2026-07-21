// Copyright (c) 2026 Elektrobit Automotive GmbH
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

// [impl->swdd~common-state-canonicalization~1]

use ankaios_api::ank_base::{
    ConfigItem, ControlInterfaceAccess, File, State, Workload,
    access_rights_rule::AccessRightsRuleEnum,
};
use std::collections::BTreeMap;

/// Trait for types that can be canonicalized to deterministic byte sequences
pub trait Canonical {
    /// Convert to canonical bytes suitable for cryptographic signing
    fn to_canonical_bytes(&self) -> Result<Vec<u8>, String>;
}

impl Canonical for State {
    fn to_canonical_bytes(&self) -> Result<Vec<u8>, String> {
        let canonical = CanonicalState::from_state(self);
        bincode::serialize(&canonical)
            .map_err(|e| format!("Failed to serialize State: {}", e))
    }
}

impl Canonical for Workload {
    fn to_canonical_bytes(&self) -> Result<Vec<u8>, String> {
        let canonical = CanonicalWorkload::from_workload(self);
        bincode::serialize(&canonical)
            .map_err(|e| format!("Failed to serialize Workload: {}", e))
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CanonicalState {
    api_version: String,
    workloads: Option<BTreeMap<String, CanonicalWorkload>>,
    configs: Option<BTreeMap<String, CanonicalConfigItem>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CanonicalWorkload {
    agent: Option<String>,
    restart_policy: Option<i32>,
    dependencies: Option<BTreeMap<String, i32>>,
    tags: Option<BTreeMap<String, String>>,
    runtime: Option<String>,
    runtime_config: Option<String>,
    control_interface_access: Option<CanonicalControlInterfaceAccess>,
    configs: Option<BTreeMap<String, String>>,
    files: Option<Vec<CanonicalFile>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum CanonicalConfigItem {
    String(String),
    Array(Vec<CanonicalConfigItem>),
    Object(BTreeMap<String, CanonicalConfigItem>),
    None,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CanonicalControlInterfaceAccess {
    allow_rules: Vec<CanonicalAccessRightsRule>,
    deny_rules: Vec<CanonicalAccessRightsRule>,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum CanonicalAccessRightsRule {
    StateRule { operation: i32, filter_masks: Vec<String> },
    LogRule { workload_names: Vec<String> },
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CanonicalFile {
    mount_point: String,
    content: Option<CanonicalFileContent>,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum CanonicalFileContent {
    Data(String),
    BinaryData(String),
}

impl CanonicalState {
    fn from_state(state: &State) -> Self {
        let configs = state.configs.as_ref().map(|cm| {
            cm.configs
                .iter()
                .map(|(k, v)| (k.clone(), CanonicalConfigItem::from_config_item(v)))
                .collect()
        });

        Self {
            api_version: state.api_version.clone(),
            workloads: state.workloads.as_ref().map(|wm| {
                wm.workloads
                    .iter()
                    .map(|(k, v)| (k.clone(), CanonicalWorkload::from_workload(v)))
                    .collect()
            }),
            configs,
        }
    }
}

impl CanonicalWorkload {
    fn from_workload(w: &Workload) -> Self {
        Self {
            agent: w.agent.clone(),
            restart_policy: w.restart_policy,
            dependencies: w.dependencies.as_ref().map(|deps| {
                deps.dependencies.iter().map(|(k, v)| (k.clone(), *v)).collect()
            }),
            tags: w.tags.as_ref().map(|tags| {
                tags.tags.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }),
            runtime: w.runtime.clone(),
            runtime_config: w.runtime_config.clone(),
            control_interface_access: w.control_interface_access.as_ref().map(
                CanonicalControlInterfaceAccess::from_cia,
            ),
            configs: w.configs.as_ref().map(|configs| {
                configs.configs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }),
            files: w
                .files
                .as_ref()
                .map(|files| files.files.iter().map(CanonicalFile::from_file).collect()),
        }
    }
}

impl CanonicalConfigItem {
    fn from_config_item(item: &ConfigItem) -> Self {
        use ankaios_api::ank_base::config_item::ConfigItemEnum;
        match &item.config_item_enum {
            Some(ConfigItemEnum::String(s)) => CanonicalConfigItem::String(s.clone()),
            Some(ConfigItemEnum::Array(arr)) => CanonicalConfigItem::Array(
                arr.values
                    .iter()
                    .map(CanonicalConfigItem::from_config_item)
                    .collect(),
            ),
            Some(ConfigItemEnum::Object(obj)) => CanonicalConfigItem::Object(
                obj.fields
                    .iter()
                    .map(|(k, v)| (k.clone(), CanonicalConfigItem::from_config_item(v)))
                    .collect(),
            ),
            None => CanonicalConfigItem::None,
        }
    }
}

impl CanonicalControlInterfaceAccess {
    fn from_cia(cia: &ControlInterfaceAccess) -> Self {
        Self {
            allow_rules: cia
                .allow_rules
                .iter()
                .map(|r| CanonicalAccessRightsRule::from_rule(r))
                .collect(),
            deny_rules: cia
                .deny_rules
                .iter()
                .map(|r| CanonicalAccessRightsRule::from_rule(r))
                .collect(),
        }
    }
}

impl CanonicalAccessRightsRule {
    fn from_rule(rule: &ankaios_api::ank_base::AccessRightsRule) -> Self {
        match &rule.access_rights_rule_enum {
            Some(AccessRightsRuleEnum::StateRule(sr)) => CanonicalAccessRightsRule::StateRule {
                operation: sr.operation,
                filter_masks: sr.filter_masks.clone(),
            },
            Some(AccessRightsRuleEnum::LogRule(lr)) => CanonicalAccessRightsRule::LogRule {
                workload_names: lr.workload_names.clone(),
            },
            None => CanonicalAccessRightsRule::StateRule {
                operation: 0,
                filter_masks: vec![],
            },
        }
    }
}

impl CanonicalFile {
    fn from_file(file: &File) -> Self {
        use ankaios_api::ank_base::file::FileContent;
        Self {
            mount_point: file.mount_point.clone(),
            content: file.file_content.as_ref().map(|fc| match fc {
                FileContent::Data(d) => CanonicalFileContent::Data(d.clone()),
                FileContent::BinaryData(d) => CanonicalFileContent::BinaryData(d.clone()),
            }),
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
mod tests {
    use super::*;
    use ankaios_api::ank_base::*;

    // [utest->swdd~common-state-canonicalization~1]
    #[test]
    fn utest_state_canonical_bytes_deterministic() {
        // Create a state with workloads in different orders
        let state1 = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap {
                workloads: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("workload_b".to_string(), Workload {
                        agent: Some("agent1".to_string()),
                        ..Default::default()
                    });
                    map.insert("workload_a".to_string(), Workload {
                        agent: Some("agent2".to_string()),
                        ..Default::default()
                    });
                    map
                },
            }),
            configs: None,
        };

        let state2 = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap {
                workloads: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("workload_a".to_string(), Workload {
                        agent: Some("agent2".to_string()),
                        ..Default::default()
                    });
                    map.insert("workload_b".to_string(), Workload {
                        agent: Some("agent1".to_string()),
                        ..Default::default()
                    });
                    map
                },
            }),
            configs: None,
        };

        // Both should produce identical canonical bytes
        let bytes1 = state1.to_canonical_bytes().unwrap();
        let bytes2 = state2.to_canonical_bytes().unwrap();

        assert_eq!(bytes1, bytes2, "Canonical bytes should be identical regardless of insertion order");
    }

    // [utest->swdd~common-state-canonicalization~1]
    #[test]
    fn utest_workload_tags_sorted() {
        let workload1 = Workload {
            tags: Some(Tags {
                tags: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("z_tag".to_string(), "value1".to_string());
                    map.insert("a_tag".to_string(), "value2".to_string());
                    map.insert("m_tag".to_string(), "value3".to_string());
                    map
                },
            }),
            ..Default::default()
        };

        let workload2 = Workload {
            tags: Some(Tags {
                tags: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("a_tag".to_string(), "value2".to_string());
                    map.insert("m_tag".to_string(), "value3".to_string());
                    map.insert("z_tag".to_string(), "value1".to_string());
                    map
                },
            }),
            ..Default::default()
        };

        let bytes1 = workload1.to_canonical_bytes().unwrap();
        let bytes2 = workload2.to_canonical_bytes().unwrap();

        assert_eq!(bytes1, bytes2, "Workload canonical bytes should be identical with different tag ordering");
    }

    // [utest->swdd~common-state-canonicalization~1]
    #[test]
    fn utest_dependencies_sorted() {
        let workload1 = Workload {
            dependencies: Some(Dependencies {
                dependencies: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("dep_z".to_string(), AddCondition::AddCondRunning as i32);
                    map.insert("dep_a".to_string(), AddCondition::AddCondSucceeded as i32);
                    map
                },
            }),
            ..Default::default()
        };

        let workload2 = Workload {
            dependencies: Some(Dependencies {
                dependencies: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("dep_a".to_string(), AddCondition::AddCondSucceeded as i32);
                    map.insert("dep_z".to_string(), AddCondition::AddCondRunning as i32);
                    map
                },
            }),
            ..Default::default()
        };

        let bytes1 = workload1.to_canonical_bytes().unwrap();
        let bytes2 = workload2.to_canonical_bytes().unwrap();

        assert_eq!(bytes1, bytes2, "Dependencies should be sorted deterministically");
    }

    // [utest->swdd~common-state-canonicalization~1]
    #[test]
    fn utest_configs_sorted() {
        let state1 = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: Some(ConfigMap {
                configs: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("config_z".to_string(), ConfigItem {
                        config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String("value1".to_string())),
                    });
                    map.insert("config_a".to_string(), ConfigItem {
                        config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String("value2".to_string())),
                    });
                    map
                },
            }),
        };

        let state2 = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: Some(ConfigMap {
                configs: {
                    let mut map = std::collections::HashMap::new();
                    map.insert("config_a".to_string(), ConfigItem {
                        config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String("value2".to_string())),
                    });
                    map.insert("config_z".to_string(), ConfigItem {
                        config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String("value1".to_string())),
                    });
                    map
                },
            }),
        };

        let bytes1 = state1.to_canonical_bytes().unwrap();
        let bytes2 = state2.to_canonical_bytes().unwrap();

        assert_eq!(bytes1, bytes2, "Configs should be sorted deterministically");
    }

    // [utest->swdd~common-state-canonicalization~1]
    #[test]
    fn utest_empty_state_canonical() {
        let state = State {
            api_version: "v1".to_string(),
            workloads: None,
            configs: None,
        };

        let bytes = state.to_canonical_bytes();
        assert!(bytes.is_ok(), "Empty state should be canonicalizable");
        assert!(!bytes.unwrap().is_empty(), "Canonical bytes should not be empty");
    }
}

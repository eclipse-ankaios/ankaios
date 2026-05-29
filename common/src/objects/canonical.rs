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

use ankaios_api::ank_base::{State, Workload};
use prost::Message;
use std::collections::BTreeMap;

/// Trait for types that can be canonicalized to deterministic byte sequences
pub trait Canonical {
    /// Convert to canonical bytes suitable for cryptographic signing
    fn to_canonical_bytes(&self) -> Result<Vec<u8>, String>;
}

impl Canonical for State {
    /// Converts a State to canonical bytes
    ///
    /// This ensures that identical logical states produce identical byte sequences
    /// regardless of map ordering or internal representation. Required for
    /// signature verification.
    ///
    /// Note: We use bincode with sorted BTreeMaps instead of protobuf because
    /// protobuf's map encoding is non-deterministic (depends on HashMap iteration order).
    /// Bincode provides deterministic serialization when using BTreeMap.
    fn to_canonical_bytes(&self) -> Result<Vec<u8>, String> {
        // Convert to a canonicalized structure with BTreeMaps
        let canonical = CanonicalState::from_state(self)?;

        // Serialize with bincode (deterministic)
        bincode::serialize(&canonical)
            .map_err(|e| format!("Failed to serialize State: {}", e))
    }
}

/// Internal canonical representation with sorted maps for deterministic serialization
#[derive(serde::Serialize, serde::Deserialize)]
struct CanonicalState {
    api_version: String,
    workloads: Option<BTreeMap<String, CanonicalWorkload>>,
    configs: Option<BTreeMap<String, Vec<u8>>>, // Serialize ConfigItem as bytes
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CanonicalWorkload {
    agent: Option<String>,
    restart_policy: Option<i32>,
    dependencies: Option<BTreeMap<String, i32>>,
    tags: Option<BTreeMap<String, String>>,
    runtime: Option<String>,
    runtime_config: Option<String>,
    control_interface_access: Option<Vec<u8>>, // Serialize as bytes
    configs: Option<BTreeMap<String, String>>,
    files: Option<Vec<u8>>, // Serialize Files as bytes
}

impl CanonicalState {
    fn from_state(state: &State) -> Result<Self, String> {
        let configs = match &state.configs {
            None => None,
            Some(cm) => {
                let mut sorted_configs = BTreeMap::new();
                for (k, v) in &cm.configs {
                    let mut buf = Vec::new();
                    v.encode(&mut buf).map_err(|e| format!("Failed to encode config: {}", e))?;
                    sorted_configs.insert(k.clone(), buf);
                }
                Some(sorted_configs)
            }
        };

        Ok(Self {
            api_version: state.api_version.clone(),
            workloads: state.workloads.as_ref().map(|wm| {
                wm.workloads
                    .iter()
                    .map(|(k, v)| (k.clone(), CanonicalWorkload::from_workload(v)))
                    .collect()
            }),
            configs,
        })
    }
}

impl CanonicalWorkload {
    fn from_workload(w: &Workload) -> Self {
        use prost::Message;

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
            control_interface_access: w.control_interface_access.as_ref().and_then(|cia| {
                let mut buf = Vec::new();
                cia.encode(&mut buf).ok()?;
                Some(buf)
            }),
            configs: w.configs.as_ref().map(|configs| {
                configs.configs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }),
            files: w.files.as_ref().and_then(|files| {
                let mut buf = Vec::new();
                files.encode(&mut buf).ok()?;
                Some(buf)
            }),
        }
    }
}

impl Canonical for Workload {
    /// Converts a Workload to canonical bytes
    fn to_canonical_bytes(&self) -> Result<Vec<u8>, String> {
        let canonical = CanonicalWorkload::from_workload(self);

        bincode::serialize(&canonical)
            .map_err(|e| format!("Failed to serialize Workload: {}", e))
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

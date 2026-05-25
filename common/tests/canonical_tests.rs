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

use ankaios_api::ank_base::{State, Workload, WorkloadMap};
use common::objects::canonical::Canonical;
use std::collections::HashMap;

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_identical_states_produce_identical_bytes() {
    let mut workloads1 = HashMap::new();
    workloads1.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            restart_policy: Some(3), // ALWAYS
            ..Default::default()
        },
    );
    workloads1.insert(
        "workload2".to_string(),
        Workload {
            agent: Some("agent_B".to_string()),
            runtime: Some("podman".to_string()),
            restart_policy: Some(3),
            ..Default::default()
        },
    );

    let state1 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads1.clone(),
        }),
        ..Default::default()
    };

    // Create second state with same data but different insertion order
    let mut workloads2 = HashMap::new();
    workloads2.insert(
        "workload2".to_string(),
        Workload {
            agent: Some("agent_B".to_string()),
            runtime: Some("podman".to_string()),
            restart_policy: Some(3),
            ..Default::default()
        },
    );
    workloads2.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            restart_policy: Some(3),
            ..Default::default()
        },
    );

    let state2 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads2,
        }),
        ..Default::default()
    };

    let canonical1 = state1.to_canonical_bytes().unwrap();
    let canonical2 = state2.to_canonical_bytes().unwrap();

    assert_eq!(
        canonical1, canonical2,
        "Identical states with different map ordering should produce identical canonical bytes"
    );
}

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_different_states_produce_different_bytes() {
    let mut workloads1 = HashMap::new();
    workloads1.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            ..Default::default()
        },
    );

    let state1 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads1,
        }),
        ..Default::default()
    };

    let mut workloads2 = HashMap::new();
    workloads2.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_B".to_string()), // Different agent
            runtime: Some("podman".to_string()),
            ..Default::default()
        },
    );

    let state2 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads2,
        }),
        ..Default::default()
    };

    let canonical1 = state1.to_canonical_bytes().unwrap();
    let canonical2 = state2.to_canonical_bytes().unwrap();

    assert_ne!(
        canonical1, canonical2,
        "Different states should produce different canonical bytes"
    );
}

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_empty_state() {
    let state = State::default();
    let result = state.to_canonical_bytes();
    assert!(result.is_ok(), "Empty state should canonicalize successfully");
}

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_state_with_configs() {
    use ankaios_api::ank_base::{ConfigItem, ConfigMap};

    let mut configs1 = HashMap::new();
    configs1.insert(
        "config1".to_string(),
        ConfigItem {
            config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String(
                "value1".to_string(),
            )),
        },
    );
    configs1.insert(
        "config2".to_string(),
        ConfigItem {
            config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String(
                "value2".to_string(),
            )),
        },
    );

    let state1 = State {
        configs: Some(ConfigMap { configs: configs1 }),
        ..Default::default()
    };

    // Same configs, different insertion order
    let mut configs2 = HashMap::new();
    configs2.insert(
        "config2".to_string(),
        ConfigItem {
            config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String(
                "value2".to_string(),
            )),
        },
    );
    configs2.insert(
        "config1".to_string(),
        ConfigItem {
            config_item_enum: Some(ankaios_api::ank_base::config_item::ConfigItemEnum::String(
                "value1".to_string(),
            )),
        },
    );

    let state2 = State {
        configs: Some(ConfigMap { configs: configs2 }),
        ..Default::default()
    };

    let canonical1 = state1.to_canonical_bytes().unwrap();
    let canonical2 = state2.to_canonical_bytes().unwrap();

    assert_eq!(
        canonical1, canonical2,
        "States with same configs in different order should produce identical bytes"
    );
}

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_workload_with_dependencies() {
    use ankaios_api::ank_base::Dependencies;

    let mut deps1 = HashMap::new();
    deps1.insert("dep1".to_string(), 1); // AddCondition::ADD_COND_RUNNING
    deps1.insert("dep2".to_string(), 2); // AddCondition::ADD_COND_SUCCEEDED

    let mut workloads1 = HashMap::new();
    workloads1.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            dependencies: Some(Dependencies {
                dependencies: deps1.clone(),
            }),
            ..Default::default()
        },
    );

    let state1 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads1,
        }),
        ..Default::default()
    };

    // Same dependencies, different insertion order
    let mut deps2 = HashMap::new();
    deps2.insert("dep2".to_string(), 2);
    deps2.insert("dep1".to_string(), 1);

    let mut workloads2 = HashMap::new();
    workloads2.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            dependencies: Some(Dependencies {
                dependencies: deps2,
            }),
            ..Default::default()
        },
    );

    let state2 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads2,
        }),
        ..Default::default()
    };

    let canonical1 = state1.to_canonical_bytes().unwrap();
    let canonical2 = state2.to_canonical_bytes().unwrap();

    assert_eq!(
        canonical1, canonical2,
        "Workloads with same dependencies in different order should produce identical bytes"
    );
}

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_workload_with_tags() {
    use ankaios_api::ank_base::Tags;

    let mut tags1 = HashMap::new();
    tags1.insert("key1".to_string(), "value1".to_string());
    tags1.insert("key2".to_string(), "value2".to_string());

    let mut workloads1 = HashMap::new();
    workloads1.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            tags: Some(Tags { tags: tags1.clone() }),
            ..Default::default()
        },
    );

    let state1 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads1,
        }),
        ..Default::default()
    };

    // Same tags, different insertion order
    let mut tags2 = HashMap::new();
    tags2.insert("key2".to_string(), "value2".to_string());
    tags2.insert("key1".to_string(), "value1".to_string());

    let mut workloads2 = HashMap::new();
    workloads2.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            tags: Some(Tags { tags: tags2 }),
            ..Default::default()
        },
    );

    let state2 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads2,
        }),
        ..Default::default()
    };

    let canonical1 = state1.to_canonical_bytes().unwrap();
    let canonical2 = state2.to_canonical_bytes().unwrap();

    assert_eq!(
        canonical1, canonical2,
        "Workloads with same tags in different order should produce identical bytes"
    );
}

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_complex_nested_state() {
    use ankaios_api::ank_base::{Dependencies, Tags};

    let mut deps = HashMap::new();
    deps.insert("dep1".to_string(), 1);

    let mut tags = HashMap::new();
    tags.insert("env".to_string(), "prod".to_string());

    let mut workloads = HashMap::new();
    workloads.insert(
        "complex-workload".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            restart_policy: Some(3),
            dependencies: Some(Dependencies {
                dependencies: deps,
            }),
            tags: Some(Tags { tags }),
            runtime_config: Some("image: nginx:latest".to_string()),
            control_interface_access: Some(ankaios_api::ank_base::ControlInterfaceAccess {
                allow_rules: vec![],
                deny_rules: vec![],
            }),
            ..Default::default()
        },
    );

    let mut configs = HashMap::new();
    configs.insert(
        "global_config".to_string(),
        ankaios_api::ank_base::ConfigItem {
            config_item_enum: Some(
                ankaios_api::ank_base::config_item::ConfigItemEnum::String(
                    "some_value".to_string(),
                ),
            ),
        },
    );

    let state = State {
        workloads: Some(WorkloadMap { workloads }),
        configs: Some(ankaios_api::ank_base::ConfigMap { configs }),
        ..Default::default()
    };

    let canonical = state.to_canonical_bytes();
    assert!(
        canonical.is_ok(),
        "Complex nested state should canonicalize successfully"
    );

    // Verify determinism by canonicalizing twice
    let canonical1 = state.to_canonical_bytes().unwrap();
    let canonical2 = state.to_canonical_bytes().unwrap();
    assert_eq!(
        canonical1, canonical2,
        "Canonicalization should be deterministic"
    );
}

// [utest->swdd~common-state-canonicalization~1]
#[test]
fn utest_canonical_workload_with_runtime_config() {
    let mut workloads1 = HashMap::new();
    workloads1.insert(
        "workload1".to_string(),
        Workload {
            agent: Some("agent_A".to_string()),
            runtime: Some("podman".to_string()),
            runtime_config: Some("image: alpine:latest\ncommand: /bin/sh".to_string()),
            ..Default::default()
        },
    );

    let state1 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads1.clone(),
        }),
        ..Default::default()
    };

    let state2 = State {
        workloads: Some(WorkloadMap {
            workloads: workloads1,
        }),
        ..Default::default()
    };

    let canonical1 = state1.to_canonical_bytes().unwrap();
    let canonical2 = state2.to_canonical_bytes().unwrap();

    assert_eq!(
        canonical1, canonical2,
        "States with runtime_config should produce identical bytes when identical"
    );
}

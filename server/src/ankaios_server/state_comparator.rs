// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use ankaios_api::ank_base::WorkloadInstanceNameSpec;
use common::{
    state_manipulation::{Object, Path},
    std_extensions::{IllegalStateResult, UnreachableOption},
};
use serde_yaml::{Mapping, Value};
use std::collections::HashSet;

#[cfg(test)]
use mockall::automock;

// [impl->swdd~server-calculates-state-differences~1]

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateComparator {
    old_state: Mapping,
    new_state: Mapping,
}

pub enum StackTask<'a> {
    VisitPair(&'a Mapping, &'a Mapping),
    PushField(String),
    PopField,
}

#[cfg_attr(test, automock)]
impl StateComparator {
    pub fn new(old_state: Mapping, new_state: Mapping) -> Self {
        Self {
            old_state,
            new_state,
        }
    }
    /// Construct separated trees for added, updated and removed fields between self and other using a depth-first search (DFS) algorithm.
    ///
    /// ## Returns
    ///
    /// - a [`StateDifferenceTree`] containing added, updated and removed field paths as separate tree structures. Leaf nodes have a null value.
    ///
    pub fn state_differences(&self) -> StateDifferenceTree {
        let convert_key_to_string = |key: &Value| -> Option<String> {
            match key {
                Value::String(key_str) => Some(key_str.clone()),
                Value::Number(key_number) if key_number.is_i64() => Some(key_number.to_string()),
                _ => {
                    log::warn!(
                        "Unsupported key type in state for state difference calculation: {key:?}"
                    );
                    None
                }
            }
        };

        let mut state_difference_tree = StateDifferenceTree::new();
        let mut stack_tasks = Vec::new();

        stack_tasks.push(StackTask::VisitPair(&self.old_state, &self.new_state));
        let mut current_field_mask = Vec::new();
        while let Some(task) = stack_tasks.pop() {
            match task {
                StackTask::VisitPair(current_node, other_node) => {
                    let current_keys: HashSet<_> = current_node.keys().collect();
                    let other_keys: HashSet<_> = other_node.keys().collect();

                    for key in &other_keys {
                        if !current_keys.contains(key) {
                            let Some(added_key) = convert_key_to_string(key) else {
                                continue;
                            };
                            let mut added_field_mask = current_field_mask.clone();
                            added_field_mask.push(added_key);
                            state_difference_tree
                                .insert_added_first_difference_tree(added_field_mask.clone());

                            StateDifferenceTree::insert_path(
                                &mut state_difference_tree.added_tree.full_difference_tree,
                                Path::from(added_field_mask),
                                Self::copy_nested_keys_to_tree(
                                    other_node.get(key).unwrap_or_unreachable(),
                                ),
                            );
                        }
                    }

                    for key in &current_keys {
                        if !other_keys.contains(key) {
                            let Some(removed_key) = convert_key_to_string(key) else {
                                continue;
                            };
                            let mut removed_field_mask = current_field_mask.clone();
                            removed_field_mask.push(removed_key);
                            state_difference_tree
                                .insert_removed_first_difference_tree(removed_field_mask.clone());
                            StateDifferenceTree::insert_path(
                                &mut state_difference_tree.removed_tree.full_difference_tree,
                                Path::from(removed_field_mask),
                                Self::copy_nested_keys_to_tree(
                                    current_node.get(key).unwrap_or_unreachable(),
                                ),
                            );
                        } else {
                            let Some(key_str) = convert_key_to_string(key) else {
                                continue;
                            };

                            let current_value = current_node.get(key).unwrap_or_unreachable();
                            let other_value = other_node.get(key).unwrap_or_unreachable();

                            match (current_value, other_value) {
                                (Value::Mapping(current_map), Value::Mapping(other_map)) => {
                                    stack_tasks.push(StackTask::PopField);
                                    stack_tasks.push(StackTask::VisitPair(current_map, other_map));
                                    stack_tasks.push(StackTask::PushField(key_str));
                                }
                                (Value::Sequence(current_seq), Value::Sequence(other_seq)) => {
                                    let mut sequence_field_mask = current_field_mask.clone();
                                    sequence_field_mask.push(key_str);

                                    if current_seq.is_empty() && !other_seq.is_empty() {
                                        state_difference_tree.insert_added_first_difference_tree(
                                            sequence_field_mask,
                                        );
                                    } else if !current_seq.is_empty() && other_seq.is_empty() {
                                        state_difference_tree.insert_removed_first_difference_tree(
                                            sequence_field_mask,
                                        );
                                    } else if current_seq != other_seq {
                                        state_difference_tree
                                            .insert_updated_path(sequence_field_mask);
                                    }
                                }
                                (current_value, other_value) => {
                                    if current_value != other_value {
                                        let mut updated_field_mask = current_field_mask.clone();
                                        updated_field_mask.push(key_str);
                                        state_difference_tree
                                            .insert_updated_path(updated_field_mask);
                                    }
                                }
                            }
                        }
                    }
                }
                StackTask::PushField(key) => {
                    current_field_mask.push(key);
                    continue;
                }
                StackTask::PopField => {
                    current_field_mask.pop();
                    continue;
                }
            }
        }

        state_difference_tree
    }

    fn copy_nested_keys_to_tree(start_node: &Value) -> Value {
        match start_node {
            Value::Mapping(map) if !map.is_empty() => {
                let mut new_map = Mapping::new();
                for (key, next_value) in map {
                    let new_value = Self::copy_nested_keys_to_tree(next_value);
                    new_map.insert(key.clone(), new_value);
                }
                Value::Mapping(new_map)
            }
            Value::Sequence(_) => Value::Null,
            _ => Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDifferenceTree {
    pub added_tree: AddedTree,
    pub removed_tree: RemovedTree,
    pub updated_tree: UpdatedTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddedTree {
    pub first_difference_tree: Object,
    pub full_difference_tree: Object,
}

impl AddedTree {
    pub fn is_empty(&self) -> bool {
        self.first_difference_tree.is_empty() && self.full_difference_tree.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]

pub struct RemovedTree {
    pub first_difference_tree: Object,
    pub full_difference_tree: Object,
}

impl RemovedTree {
    pub fn is_empty(&self) -> bool {
        self.first_difference_tree.is_empty() && self.full_difference_tree.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdatedTree {
    pub full_difference_tree: Object,
}

impl UpdatedTree {
    pub fn is_empty(&self) -> bool {
        self.full_difference_tree.is_empty()
    }
}

impl StateDifferenceTree {
    pub fn new() -> Self {
        Self {
            added_tree: AddedTree::default(),
            removed_tree: RemovedTree::default(),
            updated_tree: UpdatedTree::default(),
        }
    }

    pub fn insert_added_first_difference_tree(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.added_tree.first_difference_tree,
            Path::from(path),
            serde_yaml::Value::Null,
        );
    }

    pub fn insert_added_full_difference_tree(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.added_tree.full_difference_tree,
            Path::from(path),
            serde_yaml::Value::Null,
        );
    }

    pub fn insert_removed_first_difference_tree(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.removed_tree.first_difference_tree,
            Path::from(path),
            serde_yaml::Value::Null,
        );
    }

    pub fn insert_removed_full_difference_tree(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.removed_tree.full_difference_tree,
            Path::from(path),
            serde_yaml::Value::Null,
        );
    }

    pub fn insert_updated_path(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.updated_tree.full_difference_tree,
            Path::from(path),
            serde_yaml::Value::Null,
        );
    }

    pub fn is_empty(&self) -> bool {
        self.added_tree.is_empty() && self.removed_tree.is_empty() && self.updated_tree.is_empty()
    }

    fn insert_path(tree: &mut Object, at_path: Path, new_value: Value) {
        tree.set(&at_path, new_value).unwrap_or_illegal_state();
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct FieldDifferencePath;

impl FieldDifferencePath {
    const AGENT_KEY: &'static str = "agents";
    pub const CPU_RESOURCE_KEY: &'static str = "cpuUsage";
    pub const MEMORY_RESOURCE_KEY: &'static str = "freeMemory";
    const WORKLOAD_STATES_KEY: &'static str = "workloadStates";

    pub fn agent(agent_name: &str) -> Vec<String> {
        vec![Self::AGENT_KEY.to_string(), agent_name.to_string()]
    }

    pub fn agent_cpu(agent_name: &str) -> Vec<String> {
        vec![
            Self::AGENT_KEY.to_string(),
            agent_name.to_string(),
            Self::CPU_RESOURCE_KEY.to_string(),
        ]
    }

    pub fn agent_memory(agent_name: &str) -> Vec<String> {
        vec![
            Self::AGENT_KEY.to_string(),
            agent_name.to_string(),
            Self::MEMORY_RESOURCE_KEY.to_string(),
        ]
    }

    pub fn workload_state_agent(instance_name: &WorkloadInstanceNameSpec) -> Vec<String> {
        vec![
            Self::WORKLOAD_STATES_KEY.to_owned(),
            instance_name.agent_name().to_owned(),
        ]
    }

    pub fn workload_state(instance_name: &WorkloadInstanceNameSpec) -> Vec<String> {
        vec![
            Self::WORKLOAD_STATES_KEY.to_owned(),
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
            instance_name.id().to_owned(),
        ]
    }
}

#[cfg(test)]
pub fn validate_path_in_tree(tree: Object, path: &[String]) -> bool {
    let mapping = &Value::Mapping(tree.into());
    let mut current_node = mapping;
    for key in path {
        let next_node = current_node.get(key);
        assert!(next_node.is_some(), "Key '{key}' not found in the tree.");
        current_node = next_node.unwrap_or_unreachable();
    }
    true
}

#[cfg(test)]
pub fn generate_difference_tree_from_paths(new_tree_paths: &[Vec<String>]) -> Object {
    let mut mapping = serde_yaml::Mapping::new();
    for tree_path in new_tree_paths {
        let mut current_map = &mut mapping;
        let last = tree_path.last();
        for part in tree_path {
            if Some(part) == last {
                break;
            }
            current_map.insert(
                Value::String(part.clone()),
                serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            );
            current_map = current_map
                .get_mut(Value::String(part.to_owned()))
                .unwrap_or_unreachable()
                .as_mapping_mut()
                .unwrap_or_unreachable();
        }
        if let Some(last) = last {
            current_map.insert(Value::String(last.to_owned()), serde_yaml::Value::Null);
        }
    }
    Object::from(serde_yaml::Value::Mapping(mapping))
}

// [utest->swdd~server-calculates-state-differences~1]
#[cfg(test)]
mod tests {
    use super::WorkloadInstanceNameSpec;
    use common::state_manipulation::{Object, Path};

    use super::{FieldDifferencePath, Mapping, StateComparator, StateDifferenceTree};

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_ID_1: &str = "id_1";

    #[test]
    fn utest_state_comparator_new() {
        let mut old_state = Mapping::default();
        old_state.insert(
            serde_yaml::Value::String("key_1".to_owned()),
            serde_yaml::Value::String("value_1".to_owned()),
        );
        let new_state = Mapping::default();

        let state_comparator = StateComparator::new(old_state.clone(), new_state.clone());

        assert_eq!(state_comparator.old_state, old_state);
        assert_eq!(state_comparator.new_state, new_state);
    }

    #[test]
    fn utest_calculate_state_differences_no_differences_on_empty_states() {
        let state_comparator = StateComparator {
            old_state: Mapping::default(),
            new_state: Mapping::default(),
        };

        let state_difference_tree = state_comparator.state_differences();

        assert!(state_difference_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_no_differences_on_equal_states() {
        let old_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1
              key_2_2: value_2_2
            key_1_2: value_1_2
        "#;

        let old_state: Mapping = serde_yaml::from_str(old_state_yaml).unwrap();

        let state_comparator = StateComparator {
            old_state: old_state.clone(),
            new_state: old_state,
        };

        let state_difference_tree = state_comparator.state_differences();

        assert!(state_difference_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_added_mapping() {
        let old_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1
        "#;

        let new_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1
              key_2_2:
                key_3_1: value_3_1
            key_1_2: {}
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_added_tree_yaml = r#"
            key_1_1:
                key_2_2:
                    key_3_1: null
            key_1_2: null
        "#;
        let expected_added_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_added_tree_yaml).unwrap(),
        );

        assert_eq!(state_difference_tree.added_tree, expected_added_tree);
        assert!(state_difference_tree.updated_tree.is_empty());
        assert!(state_difference_tree.removed_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_updated_mapping() {
        let old_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1
            key_1_2: {}
        "#;

        let new_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1_updated
            key_1_2: {}
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_updated_tree_yaml = r#"
            key_1_1:
              key_2_1: null
        "#;
        let expected_updated_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_updated_tree_yaml).unwrap(),
        );

        assert_eq!(state_difference_tree.updated_tree, expected_updated_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.removed_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_removed_mapping() {
        let old_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1
            key_1_2: {}
        "#;

        let new_state_yaml = r#"
            key_1_1: {}
            key_1_2: {}
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_removed_tree_yaml = r#"
            key_1_1:
              key_2_1: null
        "#;
        let expected_removed_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_tree_yaml).unwrap(),
        );

        assert_eq!(state_difference_tree.removed_tree, expected_removed_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.updated_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_removed_nested_mapping() {
        let old_state_yaml = r#"
            key_1_1:
              key_2_1:
                key_3_1: value_3_1
            key_1_2: {}
        "#;

        let new_state_yaml = r#"
            key_1_2: {}
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();
        let expected_removed_tree_yaml = r#"
            key_1_1:
              key_2_1:
                key_3_1: null
        "#;
        let expected_removed_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_tree_yaml).unwrap(),
        );

        assert_eq!(state_difference_tree.removed_tree, expected_removed_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.updated_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_added_sequence() {
        let old_state_yaml = r#"
            key_1: []
        "#;

        let new_state_yaml = r#"
            key_1:
              - seq_value
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_added_tree_yaml = r#"
            key_1: null
        "#;
        let expected_added_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_added_tree_yaml).unwrap(),
        );

        assert_eq!(state_difference_tree.added_tree, expected_added_tree);
        assert!(state_difference_tree.updated_tree.is_empty());
        assert!(state_difference_tree.removed_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_updated_sequence() {
        let old_state_yaml = r#"
            key_1:
              - seq_value_1
        "#;

        let new_state_yaml = r#"
            key_1:
              - seq_value_1
              - seq_value_2
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_updated_tree_yaml = r#"
            key_1: null
        "#;
        let expected_updated_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_updated_tree_yaml).unwrap(),
        );

        assert_eq!(state_difference_tree.updated_tree, expected_updated_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.removed_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_removed_sequence() {
        let old_state_yaml = r#"
            key_1:
              - seq_value
        "#;

        let new_state_yaml = r#"
            key_1: []
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_removed_tree_yaml = r#"
            key_1: null
        "#;
        let expected_removed_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_tree_yaml).unwrap(),
        );

        assert_eq!(state_difference_tree.removed_tree, expected_removed_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.updated_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_key_is_number() {
        let old_state_yaml = r#"
            20: value
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: Mapping::default(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_removed_tree_yaml = r#"
            "20": null
        "#;
        let expected_removed_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_tree_yaml).unwrap(),
        );
        assert_eq!(state_difference_tree.removed_tree, expected_removed_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.updated_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_unsupported_key() {
        let old_state_yaml = r#"
            20.0: value
        "#;

        let state_comparator = StateComparator {
            old_state: Mapping::default(),
            new_state: serde_yaml::from_str(old_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        assert!(state_difference_tree.is_empty());
    }

    #[test]
    fn utest_field_difference_added_agent() {
        let field_difference_path = FieldDifferencePath::agent(AGENT_A);

        assert_eq!(
            field_difference_path,
            [
                FieldDifferencePath::AGENT_KEY.to_owned(),
                AGENT_A.to_owned()
            ]
        );
    }

    #[test]
    fn utest_field_difference_updated_agent_cpu() {
        let field_difference_path = FieldDifferencePath::agent_cpu(AGENT_A);
        assert_eq!(
            field_difference_path,
            [
                FieldDifferencePath::AGENT_KEY.to_owned(),
                AGENT_A.to_owned(),
                FieldDifferencePath::CPU_RESOURCE_KEY.to_owned(),
            ]
        );
    }

    #[test]
    fn utest_field_difference_updated_agent_memory() {
        let field_difference_path = FieldDifferencePath::agent_memory(AGENT_A);
        assert_eq!(
            field_difference_path,
            [
                FieldDifferencePath::AGENT_KEY.to_owned(),
                AGENT_A.to_owned(),
                FieldDifferencePath::MEMORY_RESOURCE_KEY.to_owned(),
            ]
        );
    }

    #[test]
    fn utest_field_difference_added_workload_state() {
        let instance_name = WorkloadInstanceNameSpec::new(AGENT_A, WORKLOAD_NAME_1, WORKLOAD_ID_1);
        let field_difference_path = FieldDifferencePath::workload_state(&instance_name);
        assert_eq!(
            field_difference_path,
            [
                FieldDifferencePath::WORKLOAD_STATES_KEY.to_owned(),
                instance_name.agent_name().to_owned(),
                instance_name.workload_name().to_owned(),
                instance_name.id().to_owned(),
            ]
        );
    }

    #[test]
    fn utest_state_difference_tree_insert_variants() {
        let mut state_difference_tree = StateDifferenceTree::new();
        let path = vec!["level_1".to_owned(), "level_2".to_owned()];
        state_difference_tree.insert_added_first_difference_tree(path.clone());
        state_difference_tree.insert_removed_first_difference_tree(path.clone());
        state_difference_tree.insert_updated_path(path);

        let expected_tree_yaml = r#"
            level_1:
              level_2: null
        "#;
        let expected_tree =
            Object::from(serde_yaml::from_str::<serde_yaml::Value>(expected_tree_yaml).unwrap());

        assert_eq!(state_difference_tree.added_tree, expected_tree);
        assert_eq!(state_difference_tree.removed_tree, expected_tree);
        assert_eq!(state_difference_tree.updated_tree, expected_tree);
    }

    #[test]
    fn utest_state_difference_tree_is_empty() {
        let empty_tree = StateDifferenceTree::new();
        assert!(empty_tree.is_empty());

        let empty_tree = StateDifferenceTree {
            added_tree: Object::default(),
            removed_tree: Object::from(serde_yaml::Value::Mapping(Mapping::new())),
            updated_tree: Object::default(),
        };

        assert!(empty_tree.is_empty());

        let new_tree = Mapping::from_iter([(
            serde_yaml::Value::String("key".to_owned()),
            serde_yaml::Value::String("value".to_owned()),
        )]);
        let non_empty_tree = StateDifferenceTree {
            added_tree: Object::default(),
            removed_tree: Object::from(serde_yaml::Value::Mapping(new_tree)),
            updated_tree: Object::default(),
        };
        assert!(!non_empty_tree.is_empty());
    }

    #[test]
    fn utest_state_difference_tree_insert_path() {
        let initial_tree_yaml = r#"
            level_1_0:
              level_2_0: null
        "#;

        let mut tree =
            Object::from(serde_yaml::from_str::<serde_yaml::Value>(initial_tree_yaml).unwrap());

        let path = Path::from(vec!["level_1_0".to_owned(), "level_2_1".to_owned()]);
        StateDifferenceTree::insert_first_change_level_path(
            &mut tree,
            path,
            serde_yaml::Value::String("value".to_owned()),
        );

        let expected_tree_yaml = r#"
            level_1_0:
              level_2_0: null
              level_2_1: value
        "#;
        let expected_tree =
            Object::from(serde_yaml::from_str::<serde_yaml::Value>(expected_tree_yaml).unwrap());

        assert_eq!(tree, expected_tree);

        let path = Path::from(vec!["level_1_1".to_owned(), "level_2_0".to_owned()]);
        StateDifferenceTree::insert_first_change_level_path(
            &mut tree,
            path,
            serde_yaml::Value::Null,
        );

        let expected_tree_yaml = r#"
            level_1_0:
              level_2_0: null
              level_2_1: value
            level_1_1:
              level_2_0: null
        "#;
        let expected_tree =
            Object::from(serde_yaml::from_str::<serde_yaml::Value>(expected_tree_yaml).unwrap());
        assert_eq!(tree, expected_tree);
    }
}

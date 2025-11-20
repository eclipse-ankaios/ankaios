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

use common::{
    objects::WorkloadInstanceName,
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
    /// - a [`StateDifferenceTree`] containing added, updated and removed fields as separate tree structures.
    ///
    pub fn state_differences(&self) -> StateDifferenceTree {
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
                            let Value::String(added_key) = key else {
                                continue;
                            };
                            let mut added_field_mask = current_field_mask.clone();
                            added_field_mask.push(added_key.clone());
                            StateDifferenceTree::insert_path(
                                &mut state_difference_tree.added_tree,
                                Path::from(added_field_mask),
                                other_node.get(key).cloned().unwrap_or_unreachable(),
                            );
                        }
                    }

                    for key in &current_keys {
                        if !other_keys.contains(key) {
                            let Value::String(removed_key) = key else {
                                continue;
                            };
                            let mut removed_field_mask = current_field_mask.clone();
                            removed_field_mask.push(removed_key.clone());
                            StateDifferenceTree::insert_path(
                                &mut state_difference_tree.removed_tree,
                                Path::from(removed_field_mask),
                                current_node.get(key).cloned().unwrap_or_unreachable(),
                            );
                        } else {
                            let Value::String(key_str) = key else {
                                continue;
                            };

                            let current_value = current_node.get(key).unwrap_or_unreachable();
                            let other_value = other_node.get(key).unwrap_or_unreachable();

                            match (current_value, other_value) {
                                (Value::Mapping(current_map), Value::Mapping(other_map)) => {
                                    stack_tasks.push(StackTask::PopField);
                                    stack_tasks.push(StackTask::VisitPair(current_map, other_map));
                                    stack_tasks.push(StackTask::PushField(key_str.clone()));
                                }
                                (Value::Sequence(current_seq), Value::Sequence(other_seq)) => {
                                    let mut sequence_field_mask = current_field_mask.clone();
                                    sequence_field_mask.push(key_str.clone());

                                    if current_seq.is_empty() && !other_seq.is_empty() {
                                        StateDifferenceTree::insert_path(
                                            &mut state_difference_tree.added_tree,
                                            Path::from(sequence_field_mask),
                                            Value::Sequence(other_seq.clone()),
                                        );
                                    } else if !current_seq.is_empty() && other_seq.is_empty() {
                                        StateDifferenceTree::insert_path(
                                            &mut state_difference_tree.removed_tree,
                                            Path::from(sequence_field_mask),
                                            Value::Sequence(current_seq.clone()),
                                        );
                                    } else if current_seq != other_seq {
                                        StateDifferenceTree::insert_path(
                                            &mut state_difference_tree.updated_tree,
                                            Path::from(sequence_field_mask),
                                            Value::Sequence(other_seq.clone()),
                                        );
                                    }
                                }
                                (current_value, other_value) => {
                                    if current_value != other_value {
                                        let mut updated_field_mask = current_field_mask.clone();
                                        updated_field_mask.push(key_str.clone());
                                        StateDifferenceTree::insert_path(
                                            &mut state_difference_tree.updated_tree,
                                            Path::from(updated_field_mask),
                                            other_value.clone(),
                                        );
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDifferenceTree {
    added_tree: Object,
    removed_tree: Object,
    updated_tree: Object,
}

impl StateDifferenceTree {
    pub fn new() -> Self {
        Self {
            added_tree: Object::default(),
            removed_tree: Object::default(),
            updated_tree: Object::default(),
        }
    }

    pub fn get_altered_fields(&self, paths: &[Path]) -> AlteredFields {
        let mut altered_fields = AlteredFields::default();

        let expanded_added_paths = self.added_tree.expand_wildcards(paths);
        Self::collect_altered_fields(
            &self.added_tree,
            &expanded_added_paths,
            &mut altered_fields.added_fields,
        );

        let expanded_removed_paths = self.removed_tree.expand_wildcards(paths);
        Self::collect_altered_fields(
            &self.removed_tree,
            &expanded_removed_paths,
            &mut altered_fields.removed_fields,
        );

        let expanded_updated_paths = self.updated_tree.expand_wildcards(paths);
        Self::collect_altered_fields(
            &self.updated_tree,
            &expanded_updated_paths,
            &mut altered_fields.updated_fields,
        );

        altered_fields
    }

    pub fn insert_added(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.added_tree,
            Path::from(path),
            serde_yaml::Value::Null,
        );
    }

    pub fn insert_removed(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.removed_tree,
            Path::from(path),
            serde_yaml::Value::Null,
        );
    }

    pub fn insert_updated(&mut self, path: Vec<String>) {
        Self::insert_path(
            &mut self.updated_tree,
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

    fn collect_altered_fields(tree: &Object, paths: &[Path], altered_fields: &mut Vec<String>) {
        for path in paths {
            if let Some(node) = tree.get(path).cloned() {
                let fields_matching_mask = collect_paths_iterative(&node, path.parts());
                fields_matching_mask.into_iter().for_each(|added_path| {
                    altered_fields.push(added_path);
                });
            }
        }
    }
}

/// Collect all leaf paths reachable from the provided start path.
pub fn collect_paths_iterative(root: &Value, start_path: &[String]) -> Vec<String> {
    let node = root;
    let prefix = start_path.join(".");
    let mut results = Vec::new();
    let mut stack = vec![(node, prefix)];
    while let Some((current, current_path)) = stack.pop() {
        match current {
            Value::Mapping(map) if !map.is_empty() => {
                for (k, v) in map {
                    if let Value::String(key) = k {
                        let new_path = format!("{current_path}.{key}");
                        stack.push((v, new_path));
                    }
                }
            }
            // Any non-mapping or empty mapping is treated as a leaf node
            _ => {
                results.push(current_path);
            }
        }
    }
    results
}

#[derive(Debug, Default)]
pub struct AlteredFields {
    pub added_fields: Vec<String>,
    pub removed_fields: Vec<String>,
    pub updated_fields: Vec<String>,
}

impl AlteredFields {
    pub fn all_empty(&self) -> bool {
        self.added_fields.is_empty()
            && self.removed_fields.is_empty()
            && self.updated_fields.is_empty()
    }
}

impl From<AlteredFields> for api::ank_base::AlteredFields {
    fn from(altered_fields: AlteredFields) -> Self {
        api::ank_base::AlteredFields {
            added_fields: altered_fields.added_fields,
            removed_fields: altered_fields.removed_fields,
            updated_fields: altered_fields.updated_fields,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct FieldDifferencePath;

impl FieldDifferencePath {
    const AGENT_KEY: &'static str = "agents";
    const CPU_RESOURCE_KEY: &'static str = "cpuUsage";
    const MEMORY_RESOURCE_KEY: &'static str = "freeMemory";
    const WORKLOAD_STATES_KEY: &'static str = "workloadStates";

    pub fn agent(agent_name: &str) -> Vec<String> {
        vec![Self::AGENT_KEY.to_string(), agent_name.to_string()]
    }

    pub fn updated_agent_cpu(agent_name: &str) -> Vec<String> {
        vec![
            Self::AGENT_KEY.to_string(),
            agent_name.to_string(),
            Self::CPU_RESOURCE_KEY.to_string(),
        ]
    }

    pub fn updated_agent_memory(agent_name: &str) -> Vec<String> {
        vec![
            Self::AGENT_KEY.to_string(),
            agent_name.to_string(),
            Self::MEMORY_RESOURCE_KEY.to_string(),
        ]
    }

    pub fn workload_state(instance_name: &WorkloadInstanceName) -> Vec<String> {
        vec![
            Self::WORKLOAD_STATES_KEY.to_owned(),
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
            instance_name.id().to_owned(),
        ]
    }
}

pub enum StackTask<'a> {
    VisitPair(&'a Mapping, &'a Mapping),
    PushField(String),
    PopField,
}

// [utest->swdd~server-calculates-state-differences~1]
#[cfg(test)]
mod tests {

    use common::{objects::WorkloadInstanceName, state_manipulation::Object};

    use super::{FieldDifferencePath, Mapping, StateComparator};

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
              key_2_2: value_2_2
            key_1_2: {}
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_added_tree_yaml = r#"
            key_1_1:
              key_2_2: value_2_2
            key_1_2: {}
        "#;
        let expected_added_tree =
            Object::try_from(serde_yaml::to_value(expected_added_tree_yaml).unwrap()).unwrap();

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
              key_2_1: value_2_1_updated
        "#;
        let expected_updated_tree =
            Object::try_from(serde_yaml::to_value(expected_updated_tree_yaml).unwrap()).unwrap();

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
              key_2_1: value_2_1
        "#;
        let expected_removed_tree =
            Object::try_from(serde_yaml::to_value(expected_removed_tree_yaml).unwrap()).unwrap();

        assert_eq!(state_difference_tree.removed_tree, expected_removed_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.updated_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_removed_nested_mapping() {
        let old_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1
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
              key_2_1: value_2_1
        "#;
        let expected_removed_tree =
            Object::try_from(serde_yaml::to_value(expected_removed_tree_yaml).unwrap()).unwrap();

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
            key_1:
              - seq_value
        "#;
        let expected_added_tree =
            Object::try_from(serde_yaml::to_value(expected_added_tree_yaml).unwrap()).unwrap();

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
            key_1:
              - seq_value_1
              - seq_value_2
        "#;
        let expected_updated_tree =
            Object::try_from(serde_yaml::to_value(expected_updated_tree_yaml).unwrap()).unwrap();

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
            key_1:
              - seq_value
        "#;
        let expected_removed_tree =
            Object::try_from(serde_yaml::to_value(expected_removed_tree_yaml).unwrap()).unwrap();

        assert_eq!(state_difference_tree.removed_tree, expected_removed_tree);
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.updated_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_key_is_not_string() {
        let old_state_yaml = r#"
            0: value
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: Mapping::default(),
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
        let field_difference_path = FieldDifferencePath::updated_agent_cpu(AGENT_A);
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
        let field_difference_path = FieldDifferencePath::updated_agent_memory(AGENT_A);
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
        let instance_name = WorkloadInstanceName::new(AGENT_A, WORKLOAD_NAME_1, WORKLOAD_ID_1);
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
}

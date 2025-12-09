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

use ankaios_api::ank_base::CompleteStateSpec;
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
    pub fn new(old_state_spec: CompleteStateSpec, new_state_spec: CompleteStateSpec) -> Self {
        let old_state =
            serde_yaml::to_value(ankaios_api::ank_base::CompleteState::from(old_state_spec))
                .unwrap_or_illegal_state()
                .as_mapping()
                .unwrap_or_unreachable()
                .to_owned();
        let new_state =
            serde_yaml::to_value(ankaios_api::ank_base::CompleteState::from(new_state_spec))
                .unwrap_or_illegal_state()
                .as_mapping()
                .unwrap_or_unreachable()
                .to_owned();

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
                StackTask::VisitPair(current_old_state_node, current_new_state_node) => {
                    let keys_old_node: HashSet<_> = current_old_state_node.keys().collect();
                    let keys_new_node: HashSet<_> = current_new_state_node.keys().collect();

                    for key in &keys_new_node {
                        if !keys_old_node.contains(key) {
                            let Some(added_key) = convert_key_to_string(key) else {
                                continue;
                            };
                            let mut added_field_mask = current_field_mask.clone();
                            added_field_mask.push(added_key);
                            // [impl->swdd~server-generates-trees-for-first-and-full-difference-field-paths~1]
                            state_difference_tree
                                .insert_added_path(added_field_mask.clone(), Default::default());

                            StateDifferenceTree::insert_path(
                                &mut state_difference_tree.added_tree.full_difference_tree,
                                Path::from(added_field_mask),
                                Self::copy_nested_keys_to_tree(
                                    current_new_state_node.get(key).unwrap_or_unreachable(),
                                ),
                            );
                        }
                    }

                    for key in &keys_old_node {
                        if !keys_new_node.contains(key) {
                            let Some(removed_key) = convert_key_to_string(key) else {
                                continue;
                            };
                            let mut removed_field_mask = current_field_mask.clone();
                            removed_field_mask.push(removed_key);
                            // [impl->swdd~server-generates-trees-for-first-and-full-difference-field-paths~1]
                            state_difference_tree.insert_removed_path(
                                removed_field_mask.clone(),
                                Default::default(),
                            );

                            StateDifferenceTree::insert_path(
                                &mut state_difference_tree.removed_tree.full_difference_tree,
                                Path::from(removed_field_mask),
                                Self::copy_nested_keys_to_tree(
                                    current_old_state_node.get(key).unwrap_or_unreachable(),
                                ),
                            );
                        } else {
                            let Some(converted_key) = convert_key_to_string(key) else {
                                continue;
                            };

                            let next_old_state_node =
                                current_old_state_node.get(key).unwrap_or_unreachable();
                            let next_new_state_node =
                                current_new_state_node.get(key).unwrap_or_unreachable();

                            match (next_old_state_node, next_new_state_node) {
                                (
                                    Value::Mapping(next_old_state_mapping),
                                    Value::Mapping(next_new_state_mapping),
                                ) => {
                                    stack_tasks.push(StackTask::PopField);
                                    stack_tasks.push(StackTask::VisitPair(
                                        next_old_state_mapping,
                                        next_new_state_mapping,
                                    ));
                                    stack_tasks.push(StackTask::PushField(converted_key));
                                }
                                (
                                    Value::Sequence(old_state_sequence),
                                    Value::Sequence(new_state_sequence),
                                ) => {
                                    let mut sequence_field_mask = current_field_mask.clone();
                                    sequence_field_mask.push(converted_key);

                                    if old_state_sequence.is_empty()
                                        && !new_state_sequence.is_empty()
                                    {
                                        state_difference_tree.insert_added_path(
                                            sequence_field_mask,
                                            Default::default(),
                                        );
                                    } else if !old_state_sequence.is_empty()
                                        && new_state_sequence.is_empty()
                                    {
                                        state_difference_tree.insert_removed_path(
                                            sequence_field_mask,
                                            Default::default(),
                                        );
                                    } else if old_state_sequence != new_state_sequence {
                                        state_difference_tree
                                            .insert_updated_path(sequence_field_mask);
                                    }
                                }
                                (old_state_value, new_state_value) => {
                                    if old_state_value != new_state_value {
                                        let mut updated_field_mask = current_field_mask.clone();
                                        updated_field_mask.push(converted_key);
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

    // [impl->swdd~server-generates-trees-for-first-and-full-difference-field-paths~1]
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

    pub fn insert_added_path(
        &mut self,
        first_level_change_path: Vec<String>,
        full_difference_path: Vec<String>,
    ) {
        Self::insert_path(
            &mut self.added_tree.first_difference_tree,
            Path::from(first_level_change_path),
            serde_yaml::Value::Null,
        );

        Self::insert_path(
            &mut self.added_tree.full_difference_tree,
            Path::from(full_difference_path),
            serde_yaml::Value::Null,
        );
    }

    pub fn insert_removed_path(
        &mut self,
        first_level_change_path: Vec<String>,
        full_difference_path: Vec<String>,
    ) {
        Self::insert_path(
            &mut self.removed_tree.first_difference_tree,
            Path::from(first_level_change_path),
            serde_yaml::Value::Null,
        );
        Self::insert_path(
            &mut self.removed_tree.full_difference_tree,
            Path::from(full_difference_path),
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
        if at_path.parts().is_empty() {
            return;
        }

        tree.set(&at_path, new_value).unwrap_or_illegal_state();
    }
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
    use ankaios_api::test_utils::{generate_test_complete_state, generate_test_workload};
    use common::state_manipulation::{Object, Path};

    use crate::ankaios_server::state_comparator::{AddedTree, RemovedTree, UpdatedTree};

    use super::{CompleteStateSpec, Mapping, StateComparator, StateDifferenceTree};

    #[test]
    fn utest_state_comparator_new() {
        let new_state_spec = generate_test_complete_state(vec![generate_test_workload()]);
        let old_state_spec = CompleteStateSpec::default();

        let state_comparator = StateComparator::new(old_state_spec.clone(), new_state_spec.clone());

        let expected_new_state =
            serde_yaml::to_value(ankaios_api::ank_base::CompleteState::from(new_state_spec))
                .unwrap()
                .as_mapping()
                .unwrap()
                .to_owned();

        let expected_old_state =
            serde_yaml::to_value(ankaios_api::ank_base::CompleteState::from(old_state_spec))
                .unwrap()
                .as_mapping()
                .unwrap()
                .to_owned();

        assert_eq!(state_comparator.old_state, expected_old_state);
        assert_eq!(state_comparator.new_state, expected_new_state);
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

    // [utest->swdd~server-generates-trees-for-first-and-full-difference-field-paths~1]
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

        let expected_first_level_change_tree_yaml = r#"
            key_1_1:
                key_2_2: null
            key_1_2: null
        "#;

        let expected_full_change_tree_yaml = r#"
            key_1_1:
                key_2_2:
                    key_3_1: null
            key_1_2: null
        "#;
        let expected_first_level_difference_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_first_level_change_tree_yaml)
                .unwrap(),
        );

        let expected_full_difference_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_full_change_tree_yaml).unwrap(),
        );

        assert_eq!(
            state_difference_tree.added_tree.first_difference_tree,
            expected_first_level_difference_tree
        );
        assert_eq!(
            state_difference_tree.added_tree.full_difference_tree,
            expected_full_difference_tree
        );
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

        assert_eq!(
            state_difference_tree.updated_tree.full_difference_tree,
            expected_updated_tree
        );
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.removed_tree.is_empty());
    }

    // [utest->swdd~server-generates-trees-for-first-and-full-difference-field-paths~1]
    #[test]
    fn utest_calculate_state_differences_removed_mapping() {
        let old_state_yaml = r#"
            key_1_1:
              key_2_1: value_2_1
              key_3_1:
                key_4_1: value_4_1
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

        let expected_removed_first_difference_tree_yaml = r#"
            key_1_1:
              key_2_1: null
              key_3_1: null
        "#;
        let expected_removed_first_difference_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_first_difference_tree_yaml)
                .unwrap(),
        );

        let expected_removed_full_difference_tree_yaml = r#"
            key_1_1:
              key_2_1: null
              key_3_1:
                key_4_1: null
        "#;

        let expected_removed_full_difference_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_full_difference_tree_yaml)
                .unwrap(),
        );

        assert_eq!(
            state_difference_tree.removed_tree.first_difference_tree,
            expected_removed_first_difference_tree
        );
        assert_eq!(
            state_difference_tree.removed_tree.full_difference_tree,
            expected_removed_full_difference_tree
        );
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.updated_tree.is_empty());
    }

    // [utest->swdd~server-generates-trees-for-first-and-full-difference-field-paths~1]
    #[test]
    fn utest_calculate_state_differences_removed_mapping_equal_first_and_full_difference_tree() {
        let old_state_yaml = r#"
            key_1_1: {}
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
            key_1_1: null
        "#;
        let expected_removed_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_tree_yaml).unwrap(),
        );

        assert_eq!(
            state_difference_tree.removed_tree.first_difference_tree,
            expected_removed_tree
        );
        assert_eq!(
            state_difference_tree.removed_tree.first_difference_tree,
            state_difference_tree.removed_tree.full_difference_tree
        );
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

        assert_eq!(
            state_difference_tree.added_tree.first_difference_tree,
            expected_added_tree
        );
        assert_eq!(
            state_difference_tree.added_tree.full_difference_tree,
            Default::default(),
        );
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

        assert_eq!(
            state_difference_tree.updated_tree.full_difference_tree,
            expected_updated_tree
        );
        assert!(state_difference_tree.added_tree.is_empty());
        assert!(state_difference_tree.removed_tree.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_removed_sequence() {
        let old_state_yaml = r#"
            key_1:
              key_2:
                key_3:
                  - seq_value
        "#;

        let new_state_yaml = r#"
            key_1:
              key_2:
                key_3: []
        "#;

        let state_comparator = StateComparator {
            old_state: serde_yaml::from_str(old_state_yaml).unwrap(),
            new_state: serde_yaml::from_str(new_state_yaml).unwrap(),
        };

        let state_difference_tree = state_comparator.state_differences();

        let expected_removed_tree_yaml = r#"
            key_1:
              key_2:
                key_3: null
        "#;
        let expected_removed_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_removed_tree_yaml).unwrap(),
        );

        assert_eq!(
            state_difference_tree.removed_tree.first_difference_tree,
            expected_removed_tree
        );
        assert_eq!(
            state_difference_tree.removed_tree.full_difference_tree,
            Default::default(),
        );
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
        assert_eq!(
            state_difference_tree.removed_tree.first_difference_tree,
            expected_removed_tree
        );
        assert_eq!(
            state_difference_tree.removed_tree.first_difference_tree,
            state_difference_tree.removed_tree.full_difference_tree
        );
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
    fn utest_state_difference_tree_insert_variants() {
        let mut state_difference_tree = StateDifferenceTree::new();
        let path_to_first_change = vec!["level_1".to_owned(), "level_2".to_owned()];
        let full_change_path = vec![
            "level_1".to_owned(),
            "level_2".to_owned(),
            "level_3".to_owned(),
        ];
        state_difference_tree
            .insert_added_path(path_to_first_change.clone(), full_change_path.clone());
        state_difference_tree.insert_removed_path(path_to_first_change, full_change_path.clone());
        state_difference_tree.insert_updated_path(full_change_path);

        let expected_first_difference_tree_yaml = r#"
            level_1:
              level_2: null
        "#;
        let expected_first_difference_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_first_difference_tree_yaml).unwrap(),
        );

        let expected_full_difference_tree_yaml = r#"
            level_1:
              level_2:
                level_3: null
        "#;
        let expected_full_difference_tree = Object::from(
            serde_yaml::from_str::<serde_yaml::Value>(expected_full_difference_tree_yaml).unwrap(),
        );

        assert_eq!(
            state_difference_tree.added_tree.first_difference_tree,
            expected_first_difference_tree
        );
        assert_eq!(
            state_difference_tree.added_tree.full_difference_tree,
            expected_full_difference_tree
        );
        assert_eq!(
            state_difference_tree.removed_tree.first_difference_tree,
            expected_first_difference_tree
        );
        assert_eq!(
            state_difference_tree.removed_tree.full_difference_tree,
            expected_full_difference_tree
        );
        assert_eq!(
            state_difference_tree.updated_tree.full_difference_tree,
            expected_full_difference_tree
        );
    }

    #[test]
    fn utest_state_difference_tree_is_empty() {
        let empty_tree = StateDifferenceTree::new();
        assert!(empty_tree.is_empty());

        let empty_tree = StateDifferenceTree {
            added_tree: AddedTree::default(),
            removed_tree: RemovedTree {
                first_difference_tree: Object::from(serde_yaml::Value::Mapping(Mapping::new())),
                full_difference_tree: Object::default(),
            },
            updated_tree: UpdatedTree::default(),
        };

        assert!(empty_tree.is_empty());

        let new_tree = Mapping::from_iter([(
            serde_yaml::Value::String("key".to_owned()),
            serde_yaml::Value::String("value".to_owned()),
        )]);
        let non_empty_tree = StateDifferenceTree {
            added_tree: AddedTree {
                first_difference_tree: Object::from(serde_yaml::Value::Mapping(new_tree)),
                full_difference_tree: Object::default(),
            },
            removed_tree: RemovedTree::default(),
            updated_tree: UpdatedTree::default(),
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
        StateDifferenceTree::insert_path(
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
        StateDifferenceTree::insert_path(&mut tree, path, serde_yaml::Value::Null);

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

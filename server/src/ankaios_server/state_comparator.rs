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

use common::{objects::WorkloadInstanceName, std_extensions::UnreachableOption};
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
    /// Determine the added, updated and removed fields between self and other using a depth-first search (DFS) algorithm.
    ///
    /// ## Returns
    ///
    /// - a [Vec<`FieldDifference`>] containing added, updated and removed fields and the corresponding field mask.
    ///
    pub fn state_differences(&self) -> Vec<FieldDifference> {
        let mut field_differences = Vec::new();
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
                            field_differences.push(FieldDifference::Added(added_field_mask));
                        }
                    }

                    for key in &current_keys {
                        if !other_keys.contains(key) {
                            let Value::String(removed_key) = key else {
                                continue;
                            };
                            let mut removed_field_mask = current_field_mask.clone();
                            removed_field_mask.push(removed_key.clone());
                            field_differences.push(FieldDifference::Removed(removed_field_mask));
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
                                        field_differences
                                            .push(FieldDifference::Added(sequence_field_mask));
                                    } else if !current_seq.is_empty() && other_seq.is_empty() {
                                        field_differences
                                            .push(FieldDifference::Removed(sequence_field_mask));
                                    } else if current_seq != other_seq {
                                        field_differences
                                            .push(FieldDifference::Updated(sequence_field_mask));
                                    }
                                }
                                _ => {
                                    if current_value != other_value {
                                        let mut updated_field_mask = current_field_mask.clone();
                                        updated_field_mask.push(key_str.clone());
                                        field_differences
                                            .push(FieldDifference::Updated(updated_field_mask));
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

        field_differences
    }
}

type FieldMask = Vec<String>; // e.g. ["desiredState", "workloads", "workload_1", "agent"]

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum FieldDifference {
    Added(FieldMask),
    Removed(FieldMask),
    Updated(FieldMask),
}

impl FieldDifference {
    const AGENT_KEY: &'static str = "agents";
    const CPU_RESOURCE_KEY: &'static str = "cpuUsage";
    const MEMORY_RESOURCE_KEY: &'static str = "freeMemory";
    const WORKLOAD_STATES_KEY: &'static str = "workloadStates";

    pub fn added_agent(agent_name: &str) -> Self {
        FieldDifference::Added(vec![Self::AGENT_KEY.to_string(), agent_name.to_string()])
    }

    pub fn removed_agent(agent_name: &str) -> Self {
        FieldDifference::Removed(vec![Self::AGENT_KEY.to_string(), agent_name.to_string()])
    }

    pub fn updated_agent_cpu(agent_name: &str) -> Self {
        FieldDifference::Updated(vec![
            Self::AGENT_KEY.to_string(),
            agent_name.to_string(),
            Self::CPU_RESOURCE_KEY.to_string(),
        ])
    }

    pub fn updated_agent_memory(agent_name: &str) -> Self {
        FieldDifference::Updated(vec![
            Self::AGENT_KEY.to_string(),
            agent_name.to_string(),
            Self::MEMORY_RESOURCE_KEY.to_string(),
        ])
    }

    pub fn added_workload_state(instance_name: &WorkloadInstanceName) -> Self {
        FieldDifference::Added(vec![
            Self::WORKLOAD_STATES_KEY.to_owned(),
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
            instance_name.id().to_owned(),
        ])
    }

    pub fn updated_workload_state(instance_name: &WorkloadInstanceName) -> Self {
        FieldDifference::Updated(vec![
            Self::WORKLOAD_STATES_KEY.to_owned(),
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
            instance_name.id().to_owned(),
        ])
    }

    pub fn removed_workload_state(instance_name: &WorkloadInstanceName) -> Self {
        FieldDifference::Removed(vec![
            Self::WORKLOAD_STATES_KEY.to_owned(),
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
            instance_name.id().to_owned(),
        ])
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

    use common::objects::WorkloadInstanceName;

    use super::{FieldDifference, Mapping, StateComparator};

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

        let changed_fields = state_comparator.state_differences();

        assert!(changed_fields.is_empty());
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

        let changed_fields = state_comparator.state_differences();

        assert!(changed_fields.is_empty());
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

        let mut changed_fields = state_comparator.state_differences();
        changed_fields.sort();

        assert_eq!(
            changed_fields,
            vec![
                FieldDifference::Added(vec!["key_1_1".to_owned(), "key_2_2".to_owned()]),
                FieldDifference::Added(vec!["key_1_2".to_owned()]),
            ]
        );
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

        let changed_fields = state_comparator.state_differences();

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Updated(vec![
                "key_1_1".to_owned(),
                "key_2_1".to_owned()
            ]),]
        );
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

        let changed_fields = state_comparator.state_differences();

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Removed(vec![
                "key_1_1".to_owned(),
                "key_2_1".to_owned()
            ]),]
        );
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

        let changed_fields = state_comparator.state_differences();
        assert_eq!(
            changed_fields,
            vec![FieldDifference::Removed(vec!["key_1_1".to_owned(),]),]
        );
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

        let changed_fields = state_comparator.state_differences();

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Added(vec!["key_1".to_owned(),]),]
        );
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

        let changed_fields = state_comparator.state_differences();

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Updated(vec!["key_1".to_owned(),]),]
        );
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

        let changed_fields = state_comparator.state_differences();

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Removed(vec!["key_1".to_owned(),]),]
        );
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

        let changed_fields = state_comparator.state_differences();

        assert!(changed_fields.is_empty());
    }

    #[test]
    fn utest_field_difference_added_agent() {
        let field_difference = FieldDifference::added_agent(AGENT_A);

        assert_eq!(
            field_difference,
            FieldDifference::Added(vec![
                FieldDifference::AGENT_KEY.to_owned(),
                AGENT_A.to_owned()
            ])
        );
    }

    #[test]
    fn utest_field_difference_removed_agent() {
        let field_difference = FieldDifference::removed_agent(AGENT_A);

        assert_eq!(
            field_difference,
            FieldDifference::Removed(vec![
                FieldDifference::AGENT_KEY.to_owned(),
                AGENT_A.to_owned()
            ])
        );
    }

    #[test]
    fn utest_field_difference_updated_agent_cpu() {
        let field_difference = FieldDifference::updated_agent_cpu(AGENT_A);
        assert_eq!(
            field_difference,
            FieldDifference::Updated(vec![
                FieldDifference::AGENT_KEY.to_owned(),
                AGENT_A.to_owned(),
                FieldDifference::CPU_RESOURCE_KEY.to_owned(),
            ])
        );
    }

    #[test]
    fn utest_field_difference_updated_agent_memory() {
        let field_difference = FieldDifference::updated_agent_memory(AGENT_A);
        assert_eq!(
            field_difference,
            FieldDifference::Updated(vec![
                FieldDifference::AGENT_KEY.to_owned(),
                AGENT_A.to_owned(),
                FieldDifference::MEMORY_RESOURCE_KEY.to_owned(),
            ])
        );
    }

    #[test]
    fn utest_field_difference_added_workload_state() {
        let instance_name = WorkloadInstanceName::new(AGENT_A, WORKLOAD_NAME_1, WORKLOAD_ID_1);
        let field_difference = FieldDifference::added_workload_state(&instance_name);
        assert_eq!(
            field_difference,
            FieldDifference::Added(vec![
                FieldDifference::WORKLOAD_STATES_KEY.to_owned(),
                instance_name.agent_name().to_owned(),
                instance_name.workload_name().to_owned(),
                instance_name.id().to_owned(),
            ])
        );
    }

    #[test]
    fn utest_field_difference_updated_workload_state() {
        let instance_name = WorkloadInstanceName::new(AGENT_A, WORKLOAD_NAME_1, WORKLOAD_ID_1);
        let field_difference = FieldDifference::updated_workload_state(&instance_name);
        assert_eq!(
            field_difference,
            FieldDifference::Updated(vec![
                FieldDifference::WORKLOAD_STATES_KEY.to_owned(),
                instance_name.agent_name().to_owned(),
                instance_name.workload_name().to_owned(),
                instance_name.id().to_owned(),
            ])
        );
    }

    #[test]
    fn utest_field_difference_removed_workload_state() {
        let instance_name = WorkloadInstanceName::new(AGENT_A, WORKLOAD_NAME_1, WORKLOAD_ID_1);
        let field_difference = FieldDifference::removed_workload_state(&instance_name);
        assert_eq!(
            field_difference,
            FieldDifference::Removed(vec![
                FieldDifference::WORKLOAD_STATES_KEY.to_owned(),
                instance_name.agent_name().to_owned(),
                instance_name.workload_name().to_owned(),
                instance_name.id().to_owned(),
            ])
        );
    }
}

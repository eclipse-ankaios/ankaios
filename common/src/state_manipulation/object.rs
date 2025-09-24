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

use std::collections::{HashSet, VecDeque};

use super::Path;
use crate::objects as ankaios;
use api::ank_base::{self as proto};
use serde_yaml::{
    Mapping, Value, from_value,
    mapping::Entry::{Occupied, Vacant},
    to_value,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Object {
    data: Value,
}

impl Default for Object {
    fn default() -> Self {
        Self {
            data: Value::Mapping(Default::default()),
        }
    }
}

impl From<serde_yaml::Value> for Object {
    fn from(data: serde_yaml::Value) -> Self {
        Object { data }
    }
}

impl TryFrom<&toml::Value> for Object {
    type Error = toml::de::Error;

    fn try_from(value: &toml::Value) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value).map_err(serde::de::Error::custom)?,
        })
    }
}

impl TryFrom<&ankaios::State> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: &ankaios::State) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryFrom<ankaios::State> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: ankaios::State) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<ankaios::CompleteState> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: ankaios::CompleteState) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryFrom<proto::CompleteState> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: proto::CompleteState) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryFrom<&ankaios::CompleteState> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: &ankaios::CompleteState) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryInto<ankaios::State> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<ankaios::State, Self::Error> {
        from_value(self.data)
    }
}

impl TryInto<ankaios::CompleteState> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<ankaios::CompleteState, Self::Error> {
        from_value(self.data)
    }
}

impl TryInto<proto::State> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<proto::State, Self::Error> {
        from_value(self.data)
    }
}

impl TryInto<proto::CompleteState> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<proto::CompleteState, Self::Error> {
        from_value(self.data)
    }
}

fn generate_paths_from_yaml_node(
    node: &Value,
    start_path: &str,
    paths: &mut HashSet<String>,
    includes_mappings_and_sequences: bool,
) {
    match node {
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let key_str = match key {
                    Value::String(key_str) => key_str.to_owned(),
                    Value::Number(key_number) if key_number.is_i64() || key_number.is_u64() => {
                        serde_yaml::to_string(key_number)
                            .unwrap()
                            .strip_suffix('\n')
                            .unwrap()
                            .to_owned()
                    }
                    _ => panic!("Unsupported mapping key '{key:?}'"),
                };
                let new_path = if start_path.is_empty() {
                    key_str
                } else {
                    format!("{start_path}.{key_str}")
                };

                if includes_mappings_and_sequences {
                    paths.insert(new_path.clone());
                }
                generate_paths_from_yaml_node(
                    value,
                    &new_path,
                    paths,
                    includes_mappings_and_sequences,
                );
            }
        }
        Value::Sequence(sequence) => {
            for (index, value) in sequence.iter().enumerate() {
                let new_path = format!("{start_path}.{index}");
                if includes_mappings_and_sequences {
                    paths.insert(new_path.clone());
                }
                generate_paths_from_yaml_node(
                    value,
                    &new_path,
                    paths,
                    includes_mappings_and_sequences,
                );
            }
        }
        _ => {
            // Leaf node (scalar value)
            paths.insert(start_path.to_string());
        }
    }
}

pub fn get_paths_from_yaml_node(node: &Value, includes_mappings_and_sequences: bool) -> Vec<Path> {
    let mut yaml_node_paths: HashSet<String> = HashSet::new();
    generate_paths_from_yaml_node(
        node,
        "",
        &mut yaml_node_paths,
        includes_mappings_and_sequences,
    );
    yaml_node_paths
        .into_iter()
        .map(|entry| Path::from(&entry))
        .collect()
}
impl From<&Object> for Vec<Path> {
    fn from(value: &Object) -> Self {
        get_paths_from_yaml_node(&value.data, true)
    }
}

type FieldMask = Vec<String>; // e.g. ["desiredState", "workloads", "workload_1", "agent"]

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldDifference {
    Added(FieldMask),
    Removed(FieldMask),
    Updated(FieldMask),
}

pub enum StackTask<'a> {
    VisitPair(&'a Mapping, &'a Mapping),
    PushField(String),
    PopField,
}

impl Object {
    pub fn set(&mut self, path: &Path, value: Value) -> Result<(), String> {
        let (path_head, path_last) = path.split_last()?;
        let mut current = self
            .data
            .as_mapping_mut()
            .ok_or("The root of the object is not a mapping")?;

        for path_part in path_head.parts() {
            let next = match current.entry(path_part.to_owned().into()) {
                Occupied(value) => &mut *value.into_mut(),
                Vacant(value) => &mut *value.insert(Value::Mapping(Mapping::default())),
            };

            current = next.as_mapping_mut().ok_or("object is not a mapping")?;
        }

        current.insert(path_last.into(), value);
        Ok(())
    }

    pub fn remove(&mut self, path: &Path) -> Result<Option<serde_yaml::Value>, String> {
        let (path_head, path_last) = path.split_last()?;

        Ok(self
            .get_as_mapping(&path_head)
            .ok_or_else(|| format!("{path_head:?} is not mapping"))?
            .remove(Value::String(path_last)))
    }

    fn get_as_mapping(&mut self, path: &Path) -> Option<&mut Mapping> {
        if let Value::Mapping(mapping) = self.get_mut(path)? {
            Some(mapping)
        } else {
            None
        }
    }

    pub fn get(&self, path: &Path) -> Option<&Value> {
        let mut current_obj = &self.data;
        for p in path.parts() {
            match current_obj {
                Value::Mapping(as_mapping) => {
                    current_obj = as_mapping.get(Value::String(p.to_owned()))?
                }
                Value::Sequence(as_sequence) => {
                    if let Ok(index) = p.parse::<usize>() {
                        current_obj = as_sequence.get(index)?
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        Some(current_obj)
    }

    fn get_mut(&mut self, path: &Path) -> Option<&mut Value> {
        let mut current_obj = &mut self.data;
        for p in path.parts() {
            if let Value::Mapping(as_mapping) = current_obj {
                current_obj = as_mapping.get_mut(Value::String(p.to_owned()))?
            } else {
                return None;
            }
        }
        Some(current_obj)
    }

    pub fn check_if_provided_path_exists(&self, path: &Path) -> bool {
        self.get(path).is_some()
    }

    /// Determine the added, updated and removed fields between self and other using a depth-first search (DFS) algorithm.
    ///
    /// ## Arguments
    ///
    /// - `other`: The [Object] containing the new state to compare against the current state.
    ///
    /// ## Returns
    ///
    /// - a [Vec<`FieldDifference`>] containing added, updated and removed fields and the corresponding field mask.
    ///
    pub fn calculate_state_differences(&self, other: &Object) -> Vec<FieldDifference> {
        let mut field_differences = Vec::new();
        let mut stack_tasks = VecDeque::new();

        let Value::Mapping(current_mapping) = &self.data else {
            return vec![];
        };

        let Value::Mapping(other_mapping) = &other.data else {
            return vec![];
        };

        stack_tasks.push_front(StackTask::VisitPair(current_mapping, other_mapping));
        let mut current_field_mask = Vec::new();
        while let Some(task) = stack_tasks.pop_front() {
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

                            let current_value = &current_node[key];
                            let other_value = &other_node[key];

                            match (current_value, other_value) {
                                (Value::Mapping(current_map), Value::Mapping(other_map)) => {
                                    stack_tasks.push_front(StackTask::PopField);
                                    stack_tasks
                                        .push_front(StackTask::VisitPair(current_map, other_map));
                                    stack_tasks.push_front(StackTask::PushField(key_str.clone()));
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
                                    let mut updated_field_mask = current_field_mask.clone();
                                    updated_field_mask.push(key_str.clone());
                                    if current_value != other_value {
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use crate::{
        objects::{
            CompleteState, ExecutionState, State, generate_test_agent_map_from_specs,
            generate_test_rendered_workload_files, generate_test_workload_spec_with_rendered_files,
            generate_test_workload_states_map_with_data,
        },
        state_manipulation::object::tests::object::Mapping,
        test_utils::generate_test_state_from_workloads,
    };
    use serde_yaml::Value;

    use super::{FieldDifference, Object};
    #[test]
    fn utest_object_from_state() {
        let state: State = generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_rendered_files(
                "agent".to_string(),
                "name".to_string(),
                "runtime".to_string(),
                generate_test_rendered_workload_files(),
            ),
        ]);

        let expected = Object {
            data: object::generate_test_state().into(),
        };
        let actual: Object = state.clone().try_into().unwrap();
        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_state_from_object() {
        let object = Object {
            data: object::generate_test_state().into(),
        };

        let actual: State = object.try_into().unwrap();
        let expected = generate_test_state_from_workloads(vec![
            generate_test_workload_spec_with_rendered_files(
                "agent".to_string(),
                "name".to_string(),
                "runtime".to_string(),
                generate_test_rendered_workload_files(),
            ),
        ]);

        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_object_from_complete_state() {
        let wl_spec = generate_test_workload_spec_with_rendered_files(
            "agent".to_string(),
            "name".to_string(),
            "runtime".to_string(),
            generate_test_rendered_workload_files(),
        );
        let specs = vec![wl_spec];
        let agent_map = generate_test_agent_map_from_specs(&specs);
        let state = generate_test_state_from_workloads(specs);

        let complete_state = CompleteState {
            desired_state: state,
            workload_states: generate_test_workload_states_map_with_data(
                "agent",
                "name",
                "404e2079115f592befb2c97fc2666aefc59a7309214828b18ff9f20f47a6ebed",
                ExecutionState::running(),
            ),
            agents: agent_map,
        };

        let expected = Object {
            data: object::generate_test_complete_state().into(),
        };
        let actual: Object = complete_state.try_into().unwrap();

        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_complete_state_from_object() {
        let object = Object {
            data: object::generate_test_complete_state().into(),
        };
        let wl_spec = generate_test_workload_spec_with_rendered_files(
            "agent".to_string(),
            "name".to_string(),
            "runtime".to_string(),
            generate_test_rendered_workload_files(),
        );
        let specs = vec![wl_spec];
        let agent_map = generate_test_agent_map_from_specs(&specs);

        let expected_state = generate_test_state_from_workloads(specs);
        let expected = CompleteState {
            desired_state: expected_state,
            workload_states: generate_test_workload_states_map_with_data(
                "agent",
                "name",
                "404e2079115f592befb2c97fc2666aefc59a7309214828b18ff9f20f47a6ebed",
                ExecutionState::running(),
            ),
            agents: agent_map,
        };
        let actual: CompleteState = object.try_into().unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_set_fails_on_empty() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };
        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(&"".into(), "value".into());

        assert!(res.is_err());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_set_fails_as_base_not_mapping() {
        let expected = Object {
            data: Value::String("not object".into()),
        };
        let mut actual = Object {
            data: Value::String("not object".into()),
        };

        let res = actual.set(
            &"workloads.workload_1.update_strategy.key".into(),
            "value".into(),
        );

        assert!(res.is_err());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_set_fails_as_not_mapping() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };
        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(&"workloads.name.tags.key".into(), "value".into());

        assert!(res.is_err());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_set_existing() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(workloads)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) = workloads.get_mut("name") {
                    workload_1.insert("update_strategy".into(), "AT_MOST_ONCE".into());
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(
            &"workloads.name.update_strategy".into(),
            "AT_MOST_ONCE".into(),
        );

        assert!(res.is_ok());
        assert_eq!(
            actual
                .get(&"workloads.name.update_strategy".into())
                .unwrap(),
            "AT_MOST_ONCE"
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_set_new() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(worklaods)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) = worklaods.get_mut("name") {
                    workload_1.insert("new_key".into(), "new value".into());
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(&"workloads.name.new_key".into(), "new value".into());

        assert!(res.is_ok());
        assert_eq!(
            actual.get(&"workloads.name.new_key".into()).unwrap(),
            "new value"
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_set_in_new_mapping() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(workloads)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) = workloads.get_mut("name") {
                    let new_entry = object::Mapping::default().entry("new_key", "new value");
                    workload_1.insert("new_map".into(), new_entry.into());
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(&"workloads.name.new_map.new_key".into(), "new value".into());

        assert!(res.is_ok());
        assert_eq!(
            actual
                .get(&"workloads.name.new_map.new_key".into())
                .unwrap(),
            "new value"
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_remove_existing() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(worklaods)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) = worklaods.get_mut("name") {
                    workload_1.remove("access_rights");
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.remove(&"workloads.name.access_rights".into());

        assert!(res.is_ok());
        assert!(actual.get(&"workloads.name.access_rights".into()).is_none());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_remove_non_existing_end_of_path() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.remove(&"workloads.name.non_existing".into());

        assert!(res.is_ok());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_remove_non_existing_in_path() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.remove(&"workloads.non_existing.access_rights".into());

        assert!(res.is_err());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_remove_non_map_in_path() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.remove(&"workloads.workload_1.agent.not_map.key".into());

        assert!(res.is_err());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_remove_empty_path() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.remove(&"".into());

        assert!(res.is_err());
        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_object_get_existing() {
        let data = Object {
            data: object::generate_test_state().into(),
        };

        let res = data.get(&"workloads.name.restartPolicy".into());

        assert!(res.is_some());
        assert_eq!(res.expect(""), &serde_yaml::Value::from("ALWAYS"));
    }

    #[test]
    fn utest_object_get_non_existing() {
        let data = Object {
            data: object::generate_test_state().into(),
        };

        let res = data.get(&"workloads.workload_1.non_existing".into());

        assert!(res.is_none());
    }

    #[test]
    fn utest_object_get_from_not_map() {
        let data = Object {
            data: object::generate_test_state().into(),
        };

        let res = data.get(&"workloads.workload_1.agent.not_map".into());

        assert!(res.is_none());
    }

    #[test]
    fn utest_object_get_from_sequence() {
        let data = Object {
            data: object::generate_test_value_object(),
        };

        let res = data.get(&"B.0".into());

        assert!(res.is_some());
    }

    #[test]
    fn utest_generate_paths_from_yaml_node_leaf_nodes_only() {
        let data: Value = object::generate_test_value_object();

        use std::collections::HashSet;

        let mut actual_paths: HashSet<String> = HashSet::new();
        super::generate_paths_from_yaml_node(&data, "", &mut actual_paths, false);

        let expected_set: HashSet<String> = HashSet::from([
            "A.AA".to_string(),
            "B.0".to_string(),
            "B.1".to_string(),
            "C".to_string(),
            "42".to_string(),
        ]);

        assert_eq!(actual_paths, expected_set)
    }

    #[test]
    fn utest_generate_paths_from_yaml_node_full() {
        let data: Value = object::generate_test_value_object();

        use std::collections::HashSet;

        let mut actual_paths: HashSet<String> = HashSet::new();
        super::generate_paths_from_yaml_node(&data, "", &mut actual_paths, true);

        let expected_set: HashSet<String> = HashSet::from([
            "A".to_string(),
            "A.AA".to_string(),
            "B".to_string(),
            "B.0".to_string(),
            "B.1".to_string(),
            "C".to_string(),
            "42".to_string(),
        ]);

        assert_eq!(actual_paths, expected_set)
    }
    #[test]
    fn utest_object_into_vec_of_path() {
        let data = Object {
            data: object::generate_test_value_object(),
        };

        use crate::state_manipulation::Path;
        let actual: Vec<Path> = Vec::<Path>::from(&data);
        let expected: Vec<Path> = vec![
            Path::from("A"),
            Path::from("A.AA"),
            Path::from("B"),
            Path::from("B.0"),
            Path::from("B.1"),
            Path::from("C"),
            Path::from("42"),
        ];

        // Convert lists to hash sets to compare lists without caring about the list order!!
        use std::collections::HashSet;
        let actual_set: HashSet<_> = actual.iter().collect();
        let expected_set: HashSet<_> = expected.iter().collect();

        assert_eq!(actual_set, expected_set)
    }

    #[test]
    fn utest_calculate_state_differences_no_differences_on_empty_states() {
        let old_state = Object {
            data: Mapping::default().into(),
        };
        let new_state = Object {
            data: Mapping::default().into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert!(changed_fields.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_no_differences_on_equal_states() {
        let old_state = Object {
            data: object::generate_test_complete_state().into(),
        };
        let new_state = &old_state;

        let changed_fields = old_state.calculate_state_differences(new_state);

        assert!(changed_fields.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_added_mapping() {
        let old_state = Object {
            data: Mapping::default()
                .entry("key_1_1", Mapping::default().entry("key_2_1", "value_2_1"))
                .into(),
        };
        let new_state = Object {
            data: Mapping::default()
                .entry(
                    "key_1_1",
                    Mapping::default()
                        .entry("key_2_1", "value_2_1")
                        .entry("key_2_2", "value_2_2"),
                )
                .entry("key_1_2", Mapping::default())
                .into(),
        };

        let mut changed_fields = old_state.calculate_state_differences(&new_state);
        changed_fields.sort();

        assert_eq!(
            changed_fields,
            vec![
                FieldDifference::Added(vec!["key_1_1".to_string(), "key_2_2".to_string()]),
                FieldDifference::Added(vec!["key_1_2".to_string()]),
            ]
        );
    }

    #[test]
    fn utest_calculate_state_differences_updated_mapping() {
        let old_state = Object {
            data: Mapping::default()
                .entry("key_1_1", Mapping::default().entry("key_2_1", "value_2_1"))
                .entry("key_1_2", Mapping::default())
                .into(),
        };

        let new_state = Object {
            data: Mapping::default()
                .entry(
                    "key_1_1",
                    Mapping::default().entry("key_2_1", "value_2_1_updated"),
                )
                .entry("key_1_2", Mapping::default())
                .into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Updated(vec![
                "key_1_1".to_string(),
                "key_2_1".to_string()
            ]),]
        );
    }

    #[test]
    fn utest_calculate_state_differences_removed_mapping() {
        let old_state = Object {
            data: Mapping::default()
                .entry("key_1_1", Mapping::default().entry("key_2_1", "value_2_1"))
                .entry("key_1_2", Mapping::default())
                .into(),
        };

        let new_state = Object {
            data: Mapping::default()
                .entry("key_1_1", Mapping::default())
                .entry("key_1_2", Mapping::default())
                .into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Removed(vec![
                "key_1_1".to_string(),
                "key_2_1".to_string()
            ]),]
        );
    }

    #[test]
    fn utest_calculate_state_differences_removed_nested_mapping() {
        let old_state = Object {
            data: Mapping::default()
                .entry("key_1_1", Mapping::default().entry("key_2_1", "value_2_1"))
                .entry("key_1_2", Mapping::default())
                .into(),
        };

        let new_state = Object {
            data: Mapping::default()
                .entry("key_1_2", Mapping::default())
                .into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);
        assert_eq!(
            changed_fields,
            vec![FieldDifference::Removed(vec!["key_1_1".to_string(),]),]
        );
    }

    #[test]
    fn utest_calculate_state_differences_added_sequence() {
        let old_state = Object {
            data: Mapping::default()
                .entry("key_1", vec![] as Vec<Value>)
                .into(),
        };

        let new_state = Object {
            data: Mapping::default()
                .entry(
                    "key_1",
                    vec![Mapping::default().entry("key_1_0", "value_1_0").into()] as Vec<Value>,
                )
                .into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Added(vec!["key_1".to_string(),]),]
        );
    }

    #[test]
    fn utest_calculate_state_differences_updated_sequence() {
        let old_state = Object {
            data: Mapping::default()
                .entry(
                    "key_1",
                    vec![Mapping::default().entry("key_1_0", "value_1_0").into()] as Vec<Value>,
                )
                .into(),
        };

        let new_state = Object {
            data: Mapping::default()
                .entry(
                    "key_1",
                    vec![
                        Mapping::default()
                            .entry("key_1_0", "value_1_0")
                            .entry("key_1_1", "value_1_1")
                            .into(),
                    ] as Vec<Value>,
                )
                .into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Updated(vec!["key_1".to_string(),]),]
        );
    }

    #[test]
    fn utest_calculate_state_differences_removed_sequence() {
        let old_state = Object {
            data: Mapping::default()
                .entry(
                    "key_1",
                    vec![Mapping::default().entry("key_1_0", "value_1_0").into()] as Vec<Value>,
                )
                .into(),
        };

        let new_state = Object {
            data: Mapping::default()
                .entry("key_1", vec![] as Vec<Value>)
                .into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert_eq!(
            changed_fields,
            vec![FieldDifference::Removed(vec!["key_1".to_string(),]),]
        );
    }

    #[test]
    fn utest_calculate_state_differences_data_is_not_mapping() {
        // owned data is not mapping
        let old_state = Object { data: Value::Null };

        let new_state = Object {
            data: Mapping::default().into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert!(changed_fields.is_empty());

        // other state is not mapping
        let old_state = Object {
            data: Mapping::default().into(),
        };

        let new_state = Object { data: Value::Null };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert!(changed_fields.is_empty());
    }

    #[test]
    fn utest_calculate_state_differences_key_is_not_string() {
        let old_state = Object {
            data: Mapping::default().entry(0, "value").into(),
        };

        let new_state = Object {
            data: Mapping::default().into(),
        };

        let changed_fields = old_state.calculate_state_differences(&new_state);

        assert!(changed_fields.is_empty());
    }

    mod object {
        use serde_yaml::Value;

        use crate::objects::generate_test_runtime_config;

        pub fn generate_test_complete_state() -> Mapping {
            let agent_name = "agent";
            let config_hash: &dyn crate::objects::ConfigHash = &generate_test_runtime_config();
            Mapping::default()
                .entry("desiredState", generate_test_state())
                .entry(
                    "workloadStates",
                    Mapping::default().entry(
                        agent_name,
                        Mapping::default().entry(
                            "name",
                            Mapping::default().entry(
                                config_hash.hash_config(),
                                Mapping::default()
                                    .entry("state", "Running")
                                    .entry("subState", "Ok")
                                    .entry("additionalInfo", ""),
                            ),
                        ),
                    ),
                )
                .entry(
                    "agents",
                    Mapping::default().entry(
                        agent_name,
                        Mapping::default()
                            .entry("cpu_usage", Mapping::default().entry("cpu_usage", 42))
                            .entry("free_memory", Mapping::default().entry("free_memory", 42)),
                    ),
                )
        }

        pub fn generate_test_state() -> Mapping {
            Mapping::default()
                .entry("apiVersion", "v0.1")
                .entry(
                    "workloads",
                    Mapping::default().entry(
                        "name",
                        Mapping::default()
                            .entry("agent", "agent")
                            .entry(
                                "tags",
                                vec![Mapping::default()
                                    .entry("key", "key")
                                    .entry("value", "value")
                                    .into()] as Vec<Value>,
                            )
                            .entry(
                                "dependencies",
                                Mapping::default()
                                    .entry("workload_A", "ADD_COND_RUNNING")
                                    .entry("workload_C", "ADD_COND_SUCCEEDED"),
                            )
                            .entry("restartPolicy", "ALWAYS")
                            .entry("runtime", "runtime")
                            .entry("runtimeConfig", "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n")
                            .entry(
                                "controlInterfaceAccess",
                                Mapping::default()
                                    .entry("allowRules", vec![] as Vec<Value>)
                                    .entry("denyRules", vec![] as Vec<Value>),
                            )
                            .entry(
                                "configs",
                                Mapping::default()
                                    .entry("ref1", "config_1")
                                    .entry("ref2", "config_2")
                            )
                            .entry("files", vec![
                                Mapping::default()
                                    .entry("mountPoint", "/file.json")
                                    .entry("data", "text data"),
                                Mapping::default()
                                    .entry("mountPoint", "/binary_file")
                                    .entry("binaryData", "base64_data"),
                            ]),
                    ),
                )
                .entry(
                    "configs",
                    Mapping::default()
                        .entry("config_1", "value 1")
                        .entry("config_2", "value 2")
                        .entry("config_3", "value 3")
                )
        }

        pub fn generate_test_value_object() -> Value {
            serde_yaml::from_str(
                r#"
                A:
                 AA: aaa
                B: [bb1, bb2]
                C: 666
                42: true # integer as object key
                "#,
            )
            .unwrap()
        }
        #[derive(Default)]
        pub struct Mapping {
            as_vec: Vec<(Value, Value)>,
        }

        impl Mapping {
            pub fn entry(mut self, key: impl Into<Value>, value: impl Into<Value>) -> Self {
                self.as_vec.push((key.into(), value.into()));
                self
            }
        }

        impl From<Mapping> for Value {
            fn from(value: Mapping) -> Self {
                Value::Mapping(value.as_vec.into_iter().collect())
            }
        }
    }
}

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

use super::Path;
use crate::std_extensions::UnreachableOption;

use ankaios_api::ank_base::{
    CompleteState, CompleteStateSpec, State, StateSpec, WILDCARD_SYMBOL, validate_field_pattern,
};

use serde_yaml::{
    Mapping, Value, from_value,
    mapping::Entry::{Occupied, Vacant},
    to_value,
};
use std::collections::{HashSet, VecDeque};

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

impl TryFrom<&StateSpec> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: &StateSpec) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryFrom<StateSpec> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: StateSpec) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<CompleteStateSpec> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: CompleteStateSpec) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryFrom<CompleteState> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: CompleteState) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryFrom<&CompleteStateSpec> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: &CompleteStateSpec) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryFrom<State> for Object {
    type Error = serde_yaml::Error;

    fn try_from(value: State) -> Result<Self, Self::Error> {
        Ok(Object {
            data: to_value(value)?,
        })
    }
}

impl TryInto<StateSpec> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<StateSpec, Self::Error> {
        from_value(self.data)
    }
}

impl TryInto<CompleteStateSpec> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<CompleteStateSpec, Self::Error> {
        from_value(self.data)
    }
}

impl TryInto<State> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<State, Self::Error> {
        from_value(self.data)
    }
}

impl TryInto<CompleteState> for Object {
    type Error = serde_yaml::Error;

    fn try_into(self) -> Result<CompleteState, Self::Error> {
        from_value(self.data)
    }
}

impl TryFrom<Object> for serde_yaml::Mapping {
    type Error = String;

    fn try_from(value: Object) -> Result<Self, Self::Error> {
        match value.data {
            Value::Mapping(map) => Ok(map),
            _ => Err("Object does not contain a mapping at the root".to_owned()),
        }
    }
}

fn generate_paths_from_yaml_node(
    node: &Value,
    start_path: &str,
    paths: &mut HashSet<String>,
    includes_mappings_and_sequences: bool,
) -> Result<(), String> {
    match node {
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let key_str = match key {
                    Value::String(key_str) => {
                        validate_field_pattern(key_str)
                            .map_err(|e| format!("Invalid mapping key '{key_str}': {e}"))?;
                        key_str.to_owned()
                    }
                    Value::Number(key_number) if key_number.is_i64() || key_number.is_u64() => {
                        serde_yaml::to_string(key_number)
                            .unwrap()
                            .strip_suffix('\n')
                            .unwrap()
                            .to_owned()
                    }
                    _ => return Err(format!("Unsupported mapping key '{key:?}'")),
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
                )?;
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
                )?;
            }
        }
        _ => {
            // Leaf node (scalar value)
            paths.insert(start_path.to_string());
        }
    }
    Ok(())
}

pub fn get_paths_from_yaml_node(
    node: &Value,
    includes_mappings_and_sequences: bool,
) -> Result<Vec<Path>, String> {
    let mut yaml_node_paths: HashSet<String> = HashSet::new();
    generate_paths_from_yaml_node(
        node,
        "",
        &mut yaml_node_paths,
        includes_mappings_and_sequences,
    )?;
    Ok(yaml_node_paths
        .into_iter()
        .map(|entry| Path::from(&entry))
        .collect())
}

impl TryFrom<&Object> for Vec<Path> {
    type Error = String;

    fn try_from(value: &Object) -> Result<Self, Self::Error> {
        get_paths_from_yaml_node(&value.data, true)
    }
}

impl Object {
    pub fn is_empty(&self) -> bool {
        self.data.as_mapping().unwrap_or_unreachable().is_empty()
    }

    //[impl->swdd~common-state-manipulation-set~1]
    pub fn set(&mut self, path: &Path, value: Value) -> Result<(), String> {
        let (path_head, path_last) = path.split_last()?;
        let mut current = self
            .data
            .as_mapping_mut()
            .ok_or("The root of the object is not a mapping")?;

        for path_part in path_head.parts() {
            let next = match current.entry(path_part.to_owned().into()) {
                Occupied(value) => &mut *value.into_mut(),
                //[impl->swdd~common-state-manipulation-set-add-missing-objects~1]
                Vacant(value) => &mut *value.insert(Value::Mapping(Mapping::default())),
            };

            current = next.as_mapping_mut().ok_or("object is not a mapping")?;
        }

        current.insert(path_last.into(), value);
        Ok(())
    }

    //[impl->swdd~common-state-manipulation-remove~1]
    pub fn remove(&mut self, path: &Path) -> Result<Option<serde_yaml::Value>, String> {
        let (path_head, path_last) = path.split_last()?;

        Ok(self
            .get_as_mapping_mut(&path_head)
            .ok_or_else(|| format!("{path_head:?} is not mapping"))?
            .remove(Value::String(path_last)))
    }

    fn get_as_mapping_mut(&mut self, path: &Path) -> Option<&mut Mapping> {
        if let Value::Mapping(mapping) = self.get_mut(path)? {
            Some(mapping)
        } else {
            None
        }
    }

    //[impl->swdd~common-state-manipulation-get~1]
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

    /// Expands wildcard paths into all concrete paths present in the object.
    ///
    /// For each input path, this method performs a depth-first search, replacing any wildcard segment (e.g., "*") with all available keys at that level.
    /// ## Arguments
    ///
    /// - `path`: A slice of [`Path`] that may contain wildcards
    ///
    /// ## Returns
    ///
    /// - a [Vec<`Path`>] containing every concrete path that matches the provided wildcard patterns.
    ///
    //[impl->swdd~common-state-manipulation-expand-wildcards~1]
    pub fn expand_wildcards(&self, path: &[Path]) -> Vec<Path> {
        let value = &self.data;
        let mut result = Vec::new();
        let mut to_do = path
            .iter()
            .map(|p| (Vec::<String>::new(), value, p.parts().as_slice()))
            .collect::<VecDeque<_>>();

        while let Some((mut current_prefix, mut current_value, mut remaining_path)) =
            to_do.pop_front()
        {
            let mut current_result_valid = true;
            while !remaining_path.is_empty() {
                let path_element;
                (path_element, remaining_path) =
                    remaining_path.split_first().unwrap_or_unreachable();
                if path_element == WILDCARD_SYMBOL {
                    current_result_valid = false;
                    if let Value::Mapping(map) = current_value {
                        for (key, value) in map {
                            let key = match key {
                                Value::String(s) => s.clone(),
                                Value::Number(n) if n.is_i64() || n.is_u64() => n.to_string(),
                                _ => continue,
                            };
                            let mut new_prefix = current_prefix.clone();
                            new_prefix.push(key);
                            to_do.push_front((new_prefix, value, remaining_path));
                        }
                    }

                    break;
                } else {
                    current_prefix.push(path_element.clone());
                    current_value = if let Some(next_element) = current_value.get(path_element) {
                        next_element
                    } else {
                        current_result_valid = false;
                        break;
                    }
                }
            }
            if current_result_valid {
                result.push(current_prefix.into());
            }
        }

        result
    }

    pub fn check_if_provided_path_exists(&self, path: &Path) -> bool {
        self.get(path).is_some()
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
    use super::{Object, generate_paths_from_yaml_node};

    use ankaios_api::ank_base::{CompleteStateSpec, ExecutionStateSpec, StateSpec};
    use ankaios_api::test_utils::{
        fixtures, generate_test_agent_map_from_workloads, generate_test_state_from_workloads,
        generate_test_workload_named, generate_test_workload_states_map_with_data,
    };

    use serde_yaml::{Mapping, Value};
    use std::collections::HashSet;

    #[test]
    fn utest_object_from_state() {
        let state: StateSpec =
            generate_test_state_from_workloads(vec![generate_test_workload_named()]);

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

        let actual: StateSpec = object.try_into().unwrap();
        let expected = generate_test_state_from_workloads(vec![generate_test_workload_named()]);

        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_object_from_complete_state() {
        let wl_named = generate_test_workload_named();
        let agent_map = generate_test_agent_map_from_workloads(&vec![wl_named.workload.clone()]);
        let workloads = vec![wl_named];
        let state = generate_test_state_from_workloads(workloads);

        let complete_state = CompleteStateSpec {
            desired_state: state,
            workload_states: generate_test_workload_states_map_with_data(
                fixtures::AGENT_NAMES[0],
                fixtures::WORKLOAD_NAMES[0],
                fixtures::WORKLOAD_IDS[0],
                ExecutionStateSpec::running(),
            ),
            agents: agent_map,
        };

        let expected = Object {
            data: object::generate_test_complete_state_mapping().into(),
        };
        let actual: Object = complete_state.try_into().unwrap();

        assert_eq!(actual, expected)
    }

    #[test]
    fn utest_complete_state_from_object() {
        let object = Object {
            data: object::generate_test_complete_state_mapping().into(),
        };
        let wl_named = generate_test_workload_named();
        let agent_map = generate_test_agent_map_from_workloads(&vec![wl_named.workload.clone()]);
        let workloads = vec![wl_named];

        let expected_state = generate_test_state_from_workloads(workloads);
        let expected = CompleteStateSpec {
            desired_state: expected_state,
            workload_states: generate_test_workload_states_map_with_data(
                fixtures::AGENT_NAMES[0],
                fixtures::WORKLOAD_NAMES[0],
                fixtures::WORKLOAD_IDS[0],
                ExecutionStateSpec::running(),
            ),
            agents: agent_map,
        };

        let actual: CompleteStateSpec = object.clone().try_into().unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn utest_mapping_from_object() {
        let object = Object {
            data: object::generate_test_state().into(),
        };

        let mapping = object.clone().try_into();

        let expected = match object.data {
            Value::Mapping(map) => map,
            _ => Mapping::default(),
        };

        assert_eq!(mapping, Ok(expected));
    }

    //[utest->swdd~common-state-manipulation-set~1]
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

    //[utest->swdd~common-state-manipulation-set~1]
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

    //[utest->swdd~common-state-manipulation-set~1]
    #[test]
    fn utest_object_set_existing() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(workloads)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) =
                    workloads.get_mut(fixtures::WORKLOAD_NAMES[0])
                {
                    workload_1.insert("update_strategy".into(), "AT_MOST_ONCE".into());
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(
            &format!("workloads.{}.update_strategy", fixtures::WORKLOAD_NAMES[0]).into(),
            "AT_MOST_ONCE".into(),
        );

        assert!(res.is_ok());
        assert_eq!(
            actual
                .get(&format!("workloads.{}.update_strategy", fixtures::WORKLOAD_NAMES[0]).into())
                .unwrap(),
            "AT_MOST_ONCE"
        );
        assert_eq!(actual, expected);
    }

    //[utest->swdd~common-state-manipulation-set~1]
    #[test]
    fn utest_object_set_new() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(workloads)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) =
                    workloads.get_mut(fixtures::WORKLOAD_NAMES[0])
                {
                    workload_1.insert("new_key".into(), "new value".into());
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(
            &format!("workloads.{}.new_key", fixtures::WORKLOAD_NAMES[0]).into(),
            "new value".into(),
        );

        assert!(res.is_ok());
        assert_eq!(
            actual
                .get(&format!("workloads.{}.new_key", fixtures::WORKLOAD_NAMES[0]).into())
                .unwrap(),
            "new value"
        );
        assert_eq!(actual, expected);
    }

    //[utest->swdd~common-state-manipulation-set-add-missing-objects~1]
    #[test]
    fn utest_object_set_in_new_mapping() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(workloads)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) =
                    workloads.get_mut(fixtures::WORKLOAD_NAMES[0])
                {
                    let new_entry = object::Mapping::default().entry("new_key", "new value");
                    workload_1.insert("new_map".into(), new_entry.into());
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.set(
            &format!("workloads.{}.new_map.new_key", fixtures::WORKLOAD_NAMES[0]).into(),
            "new value".into(),
        );

        assert!(res.is_ok());
        assert_eq!(
            actual
                .get(&format!("workloads.{}.new_map.new_key", fixtures::WORKLOAD_NAMES[0]).into())
                .unwrap(),
            "new value"
        );
        assert_eq!(actual, expected);
    }

    //[utest->swdd~common-state-manipulation-remove~1]
    #[test]
    fn utest_object_remove_existing() {
        let mut expected = Object {
            data: object::generate_test_state().into(),
        };
        if let Value::Mapping(state) = &mut expected.data {
            if let Some(Value::Mapping(workloads)) = state.get_mut("workloads") {
                if let Some(Value::Mapping(workload_1)) =
                    workloads.get_mut(fixtures::WORKLOAD_NAMES[0])
                {
                    workload_1.remove("access_rights");
                }
            }
        }

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual
            .remove(&format!("workloads.{}.access_rights", fixtures::WORKLOAD_NAMES[0]).into());

        assert!(res.is_ok());
        assert!(
            actual
                .get(&format!("workloads.{}.access_rights", fixtures::WORKLOAD_NAMES[0]).into())
                .is_none()
        );
        assert_eq!(actual, expected);
    }

    //[utest->swdd~common-state-manipulation-remove~1]
    #[test]
    fn utest_object_remove_non_existing_end_of_path() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual
            .remove(&format!("workloads.{}.non_existing", fixtures::WORKLOAD_NAMES[0]).into());

        assert!(res.is_ok());
        assert_eq!(actual, expected);
    }

    //[utest->swdd~common-state-manipulation-remove~1]
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

    //[utest->swdd~common-state-manipulation-remove~1]
    #[test]
    fn utest_object_remove_non_map_in_path() {
        let expected = Object {
            data: object::generate_test_state().into(),
        };

        let mut actual = Object {
            data: object::generate_test_state().into(),
        };

        let res = actual.remove(
            &format!(
                "workloads.{}.agent.not_map.key",
                fixtures::WORKLOAD_NAMES[0]
            )
            .into(),
        );

        assert!(res.is_err());
        assert_eq!(actual, expected);
    }

    //[utest->swdd~common-state-manipulation-remove~1]
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

    //[utest->swdd~common-state-manipulation-get~1]
    #[test]
    fn utest_object_get_existing() {
        let data = Object {
            data: object::generate_test_state().into(),
        };

        let res =
            data.get(&format!("workloads.{}.restartPolicy", fixtures::WORKLOAD_NAMES[0]).into());

        assert!(res.is_some());
        assert_eq!(res.expect(""), &serde_yaml::Value::from("ALWAYS"));
    }

    //[utest->swdd~common-state-manipulation-get~1]
    #[test]
    fn utest_object_get_non_existing() {
        let data = Object {
            data: object::generate_test_state().into(),
        };

        let res =
            data.get(&format!("workloads.{}.non_existing", fixtures::WORKLOAD_NAMES[0]).into());

        assert!(res.is_none());
    }

    //[utest->swdd~common-state-manipulation-get~1]
    #[test]
    fn utest_object_get_from_not_map() {
        let data = Object {
            data: object::generate_test_state().into(),
        };

        let res =
            data.get(&format!("workloads.{}.agent.not_map", fixtures::WORKLOAD_NAMES[0]).into());

        assert!(res.is_none());
    }

    //[utest->swdd~common-state-manipulation-get~1]
    #[test]
    fn utest_object_get_from_sequence() {
        let data = Object {
            data: object::generate_test_value_object(),
        };

        let res = data.get(&"B.0".into());

        assert!(res.is_some());
    }

    //[utest->swdd~common-state-manipulation-expand-wildcards~1]
    #[test]
    fn utest_object_expand_wildcards_with_no_wildcards() {
        let data = Object {
            data: object::generate_test_value_object_for_wildcards(),
        };

        let paths = ["A.a.age".into(), "B.b.name".into()];

        let expanded = data.expand_wildcards(&paths);

        let x = expanded
            .iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        assert_eq!(
            x,
            (paths
                .iter()
                .map(ToString::to_string)
                .collect::<HashSet<_>>())
        );
    }

    //[utest->swdd~common-state-manipulation-expand-wildcards~1]
    #[test]
    fn utest_object_expand_wildcards_with_one_wildcard() {
        let data = Object {
            data: object::generate_test_value_object_for_wildcards(),
        };

        let expanded = data.expand_wildcards(&["A.*.age".into(), "B.*.name".into()]);

        let x = expanded
            .iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        assert_eq!(
            x,
            HashSet::from([
                "A.a.age".into(),
                "A.b.age".into(),
                "A.c.age".into(),
                "B.a.name".into(),
                "B.b.name".into(),
                "B.c.name".into(),
            ])
        );
    }

    //[utest->swdd~common-state-manipulation-expand-wildcards~1]
    #[test]
    fn utest_object_expand_wildcards_with_two_wildcard() {
        let data = Object {
            data: object::generate_test_value_object_for_wildcards(),
        };

        let expanded = data.expand_wildcards(&["*.*.name".into()]);

        let x = expanded
            .iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        assert_eq!(
            x,
            HashSet::from([
                "A.a.name".into(),
                "A.b.name".into(),
                "A.c.name".into(),
                "B.a.name".into(),
                "B.b.name".into(),
                "B.c.name".into(),
            ])
        );
    }

    //[utest->swdd~common-state-manipulation-expand-wildcards~1]
    #[test]
    fn utest_object_expand_wildcards_with_two_wildcard_exclude_intermediate_missing() {
        let data = Object {
            data: object::generate_test_value_object_for_wildcards(),
        };

        let expanded = data.expand_wildcards(&["*.a.*".into()]);

        let x = expanded
            .iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        assert_eq!(
            x,
            HashSet::from(["A.a.name".into(), "A.a.age".into(), "B.a.name".into(),])
        );
    }

    //[utest->swdd~common-state-manipulation-expand-wildcards~1]
    #[test]
    fn utest_object_expand_wildcards_ignore_non_string_keys() {
        let data = Object {
            data: object::generate_test_value_object_for_wildcards(),
        };

        let expanded = data.expand_wildcards(&["C.*.*".into()]);

        let x = expanded
            .iter()
            .map(ToString::to_string)
            .collect::<HashSet<_>>();

        assert_eq!(x, HashSet::from(["C.d.a".into(), "C.d.c".into(),]));
    }

    #[test]
    fn utest_generate_paths_from_yaml_node_leaf_nodes_only() {
        let data: Value = object::generate_test_value_object();

        use std::collections::HashSet;

        let mut actual_paths: HashSet<String> = HashSet::new();
        generate_paths_from_yaml_node(&data, "", &mut actual_paths, false).unwrap();

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
        generate_paths_from_yaml_node(&data, "", &mut actual_paths, true).unwrap();

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
        let actual: Vec<Path> = Vec::<Path>::try_from(&data).unwrap();
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

    mod object {
        use serde_yaml::Value;

        use ankaios_api::CURRENT_API_VERSION;
        use ankaios_api::ank_base::ConfigHash;
        use ankaios_api::test_utils::fixtures;

        pub fn generate_test_complete_state_mapping() -> Mapping {
            let agent_name = fixtures::AGENT_NAMES[0];
            let config_hash: &dyn ConfigHash = &String::from(fixtures::RUNTIME_CONFIGS[0]);
            Mapping::default()
                .entry("desiredState", generate_test_state())
                .entry(
                    "workloadStates",
                    Mapping::default().entry(
                        agent_name,
                        Mapping::default().entry(
                            fixtures::WORKLOAD_NAMES[0],
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
                            .entry(
                                "status",
                                Mapping::default()
                                    .entry("cpu_usage", 50)
                                    .entry("free_memory", 1024),
                            )
                            .entry("tags", Mapping::default().entry("type", "agent")),
                    ),
                )
        }

        pub fn generate_test_state() -> Mapping {
            Mapping::default()
                .entry("apiVersion", CURRENT_API_VERSION)
                .entry(
                    "workloads",
                    Mapping::default().entry(
                        fixtures::WORKLOAD_NAMES[0],
                        Mapping::default()
                            .entry("agent", fixtures::AGENT_NAMES[0])
                            .entry(
                                "tags",
                                Mapping::default()
                                    .entry("tag1", "val_1")
                                    .entry("tag2", "val_2"),
                            )
                            .entry(
                                "dependencies",
                                Mapping::default()
                                    .entry(fixtures::WORKLOAD_NAMES[1], "ADD_COND_RUNNING")
                                    .entry(fixtures::WORKLOAD_NAMES[2], "ADD_COND_SUCCEEDED"),
                            )
                            .entry("restartPolicy", "ALWAYS")
                            .entry("runtime", "runtime_A")
                            .entry("runtimeConfig", fixtures::RUNTIME_CONFIGS[0])
                            .entry(
                                "controlInterfaceAccess",
                                Mapping::default()
                                    .entry(
                                        "allowRules",
                                        vec![
                                            Mapping::default()
                                                .entry("type", "StateRule")
                                                .entry("operation", "ReadWrite")
                                                .entry("filterMasks", vec!["desiredState"]),
                                            Mapping::default().entry("type", "LogRule").entry(
                                                "workloadNames",
                                                vec![fixtures::WORKLOAD_NAMES[0]],
                                            ),
                                        ],
                                    )
                                    .entry(
                                        "denyRules",
                                        vec![
                                            Mapping::default()
                                                .entry("type", "StateRule")
                                                .entry("operation", "Write")
                                                .entry(
                                                    "filterMasks",
                                                    vec![format!(
                                                        "desiredState.workloads.{}",
                                                        fixtures::WORKLOAD_NAMES[1]
                                                    )],
                                                ),
                                        ],
                                    ),
                            )
                            .entry(
                                "configs",
                                Mapping::default()
                                    .entry("ref1", "config_1")
                                    .entry("ref2", "config_2"),
                            )
                            .entry(
                                "files",
                                vec![
                                    Mapping::default()
                                        .entry("mountPoint", fixtures::FILE_TEXT_PATH)
                                        .entry("data", fixtures::FILE_TEXT_DATA),
                                    Mapping::default()
                                        .entry("mountPoint", fixtures::FILE_BINARY_PATH)
                                        .entry("binaryData", fixtures::FILE_BINARY_DATA),
                                ],
                            ),
                    ),
                )
                .entry(
                    "configs",
                    Mapping::default()
                        .entry("config_1", "value 1")
                        .entry("config_2", "value 2")
                        .entry("config_3", "value 3"),
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

        pub fn generate_test_value_object_for_wildcards() -> Value {
            serde_yaml::from_str(
                r#"
                A:
                    a:
                        name: Anton
                        age: 42
                    b:
                        name: Berta
                        age: 36
                    c:
                        name: Caesar
                        age: 12
                B:
                    a:
                        name: Alpha
                    b:
                        name: Beta
                    c:
                        name: Charlie
                C:
                    d:
                        a: b
                        c: d
                    32: "number as key"
                    23:
                    - one
                    - two
                    - three
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

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

use crate::ank_base::{WorkloadInstanceNameSpec, WorkloadSpec};
use crate::helpers::serialize_to_ordered_map;
use crate::{CURRENT_API_VERSION, PREVIOUS_API_VERSION};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;

const MAX_CHARACTERS_WORKLOAD_NAME: usize = 63;
pub const ALLOWED_SYMBOLS: &str = "[a-zA-Z0-9_-]";
pub const STR_RE_WORKLOAD: &str = r"^[a-zA-Z0-9_-]*$";
pub const STR_RE_AGENT: &str = r"^[a-zA-Z0-9_-]*$";
pub const STR_RE_CONFIG_REFERENCES: &str = r"^[a-zA-Z0-9_-]*$";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkloadNamed {
    #[serde(skip)]
    pub instance_name: WorkloadInstanceNameSpec,
    #[serde(flatten)]
    pub workload: WorkloadSpec,
}

impl From<(String, WorkloadSpec)> for WorkloadNamed {
    fn from((workload_name, workload): (String, WorkloadSpec)) -> Self {
        let instance_name = WorkloadInstanceNameSpec::builder()
            .agent_name(workload.agent.clone())
            .workload_name(workload_name)
            .config(&workload.runtime_config)
            .build();

        WorkloadNamed {
            instance_name,
            workload,
        }
    }
}

// [impl->swdd~api-access-rules-logs-workload-names-convention~1]
pub fn validate_wildcard_workload_name_format(
    workload_name: &str,
    wildcard_pos: usize,
) -> Result<(), String> {
    let prefix = &workload_name[..wildcard_pos];
    let suffix = &workload_name[wildcard_pos + 1..];

    validate_workload_name_pattern(prefix)
        .and_then(|_| validate_workload_name_pattern(suffix))
        .and_then(|_| validate_workload_name_length(prefix.len() + suffix.len()))
        .map_err(|err| format!("Unsupported workload name with wildcard '{workload_name}'. {err}"))
}

// [impl->swdd~api-workload-naming-convention~1]
pub fn validate_workload_name_format(workload_name: &str) -> Result<(), String> {
    let length = workload_name.len();
    validate_workload_name_pattern(workload_name)
        .and_then(|_| validate_workload_name_not_empty(length))
        .and_then(|_| validate_workload_name_length(length))
        .map_err(|err| format!("Unsupported workload name '{workload_name}'. {err}"))
}

fn validate_workload_name_pattern(workload_name: &str) -> Result<(), String> {
    let re_workloads = Regex::new(STR_RE_WORKLOAD).unwrap();
    if !re_workloads.is_match(workload_name) {
        Err(format!("Expected to have characters in {ALLOWED_SYMBOLS}."))
    } else {
        Ok(())
    }
}

fn validate_workload_name_length(length: usize) -> Result<(), String> {
    if length > MAX_CHARACTERS_WORKLOAD_NAME {
        Err(format!(
            "Length {length} exceeds the maximum limit of {MAX_CHARACTERS_WORKLOAD_NAME} characters."
        ))
    } else {
        Ok(())
    }
}

fn validate_workload_name_not_empty(length: usize) -> Result<(), String> {
    if length == 0 {
        Err("Is empty.".into())
    } else {
        Ok(())
    }
}

// [impl->swdd~api-agent-naming-convention~1]
fn validate_agent_name_format(agent_name: &str) -> Result<(), String> {
    let re_agent = Regex::new(STR_RE_AGENT).unwrap();
    if !re_agent.is_match(agent_name) {
        Err(format!(
            "Unsupported agent name. Received '{agent_name}', expected to have characters in {ALLOWED_SYMBOLS}"
        ))
    } else {
        Ok(())
    }
}

impl WorkloadSpec {
    // [impl->swdd~api-workload-needs-control-interface~1]
    pub fn needs_control_interface(&self) -> bool {
        !self.control_interface_access.allow_rules.is_empty()
    }

    // [impl->swdd~api-config-aliases-and-config-reference-keys-naming-convention~1]
    pub fn validate_config_reference_format(&self) -> Result<(), String> {
        let re_config_references = Regex::new(STR_RE_CONFIG_REFERENCES).unwrap();
        for (config_alias, referenced_config) in self.configs.configs.iter() {
            if !re_config_references.is_match(config_alias) {
                return Err(format!(
                    "Unsupported config alias. Received '{config_alias}', expected to have characters in {STR_RE_CONFIG_REFERENCES}"
                ));
            }

            if !re_config_references.is_match(referenced_config) {
                return Err(format!(
                    "Unsupported config reference key. Received '{referenced_config}', expected to have characters in {STR_RE_CONFIG_REFERENCES}"
                ));
            }
        }
        Ok(())
    }
}

impl WorkloadNamed {
    // [impl->swdd~api-workload-naming-convention~1]
    // [impl->swdd~api-agent-naming-convention~1]
    // [impl->swdd~api-access-rules-filter-mask-convention~1]
    pub fn validate_fields_format(&self) -> Result<(), String> {
        validate_workload_name_format(self.instance_name.workload_name())?;
        validate_agent_name_format(self.instance_name.agent_name())?;
        validate_agent_name_format(&self.workload.agent)?;
        self.workload.control_interface_access.validate_format()?;
        Ok(())
    }
}

// [impl->swdd~api-object-serialization~1]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct DeletedWorkload {
    pub instance_name: WorkloadInstanceNameSpec,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub dependencies: HashMap<String, DeleteCondition>,
}

// [impl->swdd~api-delete-conditions-for-dependencies~1]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeleteCondition {
    DelCondRunning = 0,
    DelCondNotPendingNorRunning = 1,
}

impl TryFrom<i32> for DeleteCondition {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            x if x == DeleteCondition::DelCondRunning as i32 => Ok(DeleteCondition::DelCondRunning),
            x if x == DeleteCondition::DelCondNotPendingNorRunning as i32 => {
                Ok(DeleteCondition::DelCondNotPendingNorRunning)
            }
            _ => Err(format!(
                "Received an unknown value '{value}' as DeleteCondition."
            )),
        }
    }
}

// This method is used for backwards compatibility to older versions and can be deleted later
pub fn validate_tags(
    api_version: &str,
    tags_value: &Value,
    workload_name: &str,
) -> Result<(), String> {
    match api_version {
        CURRENT_API_VERSION => {
            if !tags_value.is_mapping() {
                return Err(format!(
                    "For API version '{CURRENT_API_VERSION}', tags must be specified as a mapping (key-value pairs). Found tags as sequence for workload '{workload_name}'.",
                ));
            }
        }
        PREVIOUS_API_VERSION => {
            if !tags_value.is_sequence() {
                return Err(format!(
                    "For API version '{PREVIOUS_API_VERSION}', tags must be specified as a sequence (list of key-value entries). Found tags as mapping for workload '{workload_name}'.",
                ));
            }
        }
        _ => {}
    }

    Ok(())
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "test_utils", test))]
use crate::ank_base::{
    ConfigMappingsSpec, DependenciesSpec, FileContentSpec, FileSpec, FilesSpec, TagsSpec, Workload,
};
#[cfg(any(feature = "test_utils", test))]
use crate::test_utils::generate_test_control_interface_access;

#[cfg(any(feature = "test_utils", test))]
impl WorkloadNamed {
    pub fn name<T: Into<String>>(mut self, name: T) -> Self {
        self.instance_name.workload_name = name.into();
        self
    }
}

#[cfg(any(feature = "test_utils", test))]
pub trait TestWorkloadFixture: Default {
    fn generate_workload() -> Self;
    fn generate_workload_with_params(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
    ) -> Self;
    fn generate_workload_with_runtime_config(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
        runtime_config: impl Into<String>,
    ) -> Self;
}

#[cfg(any(feature = "test_utils", test))]
impl TestWorkloadFixture for WorkloadSpec {
    fn generate_workload() -> Self {
        generate_test_workload_with_param("agent_A", "runtime_A")
    }

    fn generate_workload_with_params(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
    ) -> Self {
        generate_test_workload_with_runtime_config(
            agent_name,
            runtime_name,
            generate_test_runtime_config(),
        )
    }

    fn generate_workload_with_runtime_config(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
        runtime_config: impl Into<String>,
    ) -> Self {
        use crate::ank_base::AddCondition;

        WorkloadSpec {
            agent: agent_name.into(),
            dependencies: DependenciesSpec {
                dependencies: HashMap::from([
                    (String::from("workload_B"), AddCondition::AddCondRunning),
                    (String::from("workload_C"), AddCondition::AddCondSucceeded),
                ]),
            },
            restart_policy: crate::ank_base::RestartPolicy::Always,
            runtime: runtime_name.into(),
            tags: TagsSpec {
                tags: HashMap::from([
                    ("tag1".into(), "val_1".into()),
                    ("tag2".into(), "val_2".into()),
                ]),
            },
            runtime_config: runtime_config.into(),
            configs: ConfigMappingsSpec {
                configs: HashMap::from([
                    ("ref1".into(), "config_1".into()),
                    ("ref2".into(), "config_2".into()),
                ]),
            },
            control_interface_access: generate_test_control_interface_access(),
            files: FilesSpec {
                files: vec![
                    FileSpec {
                        mount_point: "/file.json".to_string(),
                        file_content: FileContentSpec::Data {
                            data: "text data".into(),
                        },
                    },
                    FileSpec {
                        mount_point: "/binary_file".to_string(),
                        file_content: FileContentSpec::BinaryData {
                            binary_data: "base64_data".into(),
                        },
                    },
                ],
            },
        }
    }
}

#[cfg(any(feature = "test_utils", test))]
impl TestWorkloadFixture for Workload {
    fn generate_workload() -> Self {
        WorkloadSpec::generate_workload().into()
    }

    fn generate_workload_with_params(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
    ) -> Self {
        WorkloadSpec::generate_workload_with_params(agent_name, runtime_name).into()
    }

    fn generate_workload_with_runtime_config(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
        runtime_config: impl Into<String>,
    ) -> Self {
        WorkloadSpec::generate_workload_with_runtime_config(
            agent_name,
            runtime_name,
            runtime_config,
        )
        .into()
    }
}

#[cfg(any(feature = "test_utils", test))]
impl TestWorkloadFixture for WorkloadNamed {
    fn generate_workload() -> Self {
        (
            String::from("workload_A"),
            WorkloadSpec::generate_workload(),
        )
            .into()
    }

    fn generate_workload_with_params(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
    ) -> Self {
        (
            String::from("workload_A"),
            WorkloadSpec::generate_workload_with_params(agent_name, runtime_name),
        )
            .into()
    }

    fn generate_workload_with_runtime_config(
        agent_name: impl Into<String>,
        runtime_name: impl Into<String>,
        runtime_config: impl Into<String>,
    ) -> Self {
        (
            String::from("workload_A"),
            WorkloadSpec::generate_workload_with_runtime_config(
                agent_name,
                runtime_name,
                runtime_config,
            ),
        )
            .into()
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload<T: TestWorkloadFixture>() -> T {
    T::generate_workload()
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_with_param<T: TestWorkloadFixture>(
    agent_name: impl Into<String>,
    runtime_name: impl Into<String>,
) -> T {
    T::generate_workload_with_params(agent_name, runtime_name)
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_with_runtime_config<T: TestWorkloadFixture>(
    agent_name: impl Into<String>,
    runtime_name: impl Into<String>,
    runtime_config: impl Into<String>,
) -> T {
    T::generate_workload_with_runtime_config(agent_name, runtime_name, runtime_config)
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_runtime_config() -> String {
    "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::validate_workload_name_format;

    use crate::ank_base::{AddCondition, DeleteCondition, WorkloadNamed, WorkloadSpec};
    use crate::test_utils::{
        generate_test_deleted_workload, generate_test_workload, generate_test_workload_with_param,
    };

    // one test for a failing case, other cases are tested on the caller side to not repeat test code
    // [utest->swdd~api-config-aliases-and-config-reference-keys-naming-convention~1]
    #[test]
    fn utest_validate_config_reference_format_invalid_config_reference_key() {
        let invalid_config_reference_key = "invalid%key";
        let mut workload_spec: WorkloadSpec = generate_test_workload();
        workload_spec.configs.configs.insert(
            "config_alias_1".to_owned(),
            invalid_config_reference_key.to_owned(),
        );
        assert_eq!(
            workload_spec.validate_config_reference_format(),
            Err(format!(
                "Unsupported config reference key. Received '{}', expected to have characters in {}",
                invalid_config_reference_key,
                super::STR_RE_CONFIG_REFERENCES
            ))
        );
    }

    // [utest->swdd~api-add-conditions-for-dependencies~1]
    #[test]
    fn utest_add_condition_from_int() {
        assert_eq!(
            AddCondition::try_from(0).unwrap(),
            AddCondition::AddCondRunning
        );
        assert_eq!(
            AddCondition::try_from(1).unwrap(),
            AddCondition::AddCondSucceeded
        );
        assert_eq!(
            AddCondition::try_from(2).unwrap(),
            AddCondition::AddCondFailed
        );
        assert_eq!(
            AddCondition::try_from(100),
            Err(prost::UnknownEnumValue(100))
        );
    }

    // [utest->swdd~api-delete-conditions-for-dependencies~1]
    #[test]
    fn utest_delete_condition_from_int() {
        assert_eq!(
            DeleteCondition::try_from(0).unwrap(),
            DeleteCondition::DelCondRunning
        );
        assert_eq!(
            DeleteCondition::try_from(1).unwrap(),
            DeleteCondition::DelCondNotPendingNorRunning
        );
        assert_eq!(
            DeleteCondition::try_from(100),
            Err::<DeleteCondition, String>(
                "Received an unknown value '100' as DeleteCondition.".to_string()
            )
        );
    }

    // [utest->swdd~api-object-serialization~1]
    #[test]
    fn utest_serialize_deleted_workload_into_ordered_output() {
        let mut deleted_workload =
            generate_test_deleted_workload("agent X".to_string(), "workload X".to_string());

        deleted_workload.dependencies.insert(
            "workload_C".to_string(),
            DeleteCondition::DelCondNotPendingNorRunning,
        );

        let serialized_deleted_workload = serde_yaml::to_string(&deleted_workload).unwrap();
        let indices = [
            serialized_deleted_workload.find("workload_A").unwrap(),
            serialized_deleted_workload.find("workload_C").unwrap(),
        ];
        assert!(
            indices.windows(2).all(|window| window[0] < window[1]),
            "expected ordered dependencies."
        );
    }

    // [utest->swdd~api-workload-needs-control-interface~1]
    #[test]
    fn utest_needs_control_interface() {
        let mut workload: WorkloadSpec = generate_test_workload(); // Generates with control interface access
        assert!(workload.needs_control_interface());

        workload.control_interface_access = Default::default(); // No rules
        assert!(!workload.needs_control_interface());
    }

    // [utest->swdd~api-workload-naming-convention~1]
    // [utest->swdd~api-agent-naming-convention~1]
    // [utest->swdd~api-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_workload_validate_fields_format_success() {
        let compatible_workload: WorkloadNamed = generate_test_workload();
        assert!(compatible_workload.validate_fields_format().is_ok());
    }

    // [utest->swdd~api-workload-naming-convention~1]
    #[test]
    fn utest_validate_workload_name_format_empty_workload_name() {
        let workload_name = "".to_string();
        assert_eq!(
            validate_workload_name_format(&workload_name),
            Err("Unsupported workload name ''. Is empty.".into())
        );
    }

    // [utest->swdd~api-workload-naming-convention~1]
    #[test]
    fn utest_workload_validate_fields_incompatible_workload_name() {
        let workload_with_wrong_name =
            generate_test_workload::<WorkloadNamed>().name("incompatible.workload_name");

        assert_eq!(
            workload_with_wrong_name.validate_fields_format(),
            Err(format!(
                "Unsupported workload name '{}'. Expected to have characters in {}.",
                workload_with_wrong_name.instance_name.workload_name(),
                super::ALLOWED_SYMBOLS
            ))
        );
    }

    // [utest->swdd~api-agent-naming-convention~1]
    #[test]
    fn utest_workload_validate_fields_incompatible_agent_name() {
        let workload_with_wrong_agent_name: WorkloadNamed =
            generate_test_workload_with_param("incompatible.agent_name", "runtime");

        assert_eq!(
            workload_with_wrong_agent_name.validate_fields_format(),
            Err(format!(
                "Unsupported agent name. Received '{}', expected to have characters in {}",
                workload_with_wrong_agent_name.instance_name.agent_name(),
                super::ALLOWED_SYMBOLS
            ))
        );
    }

    // [utest->swdd~api-agent-naming-convention~1]
    #[test]
    fn utest_workload_with_valid_empty_agent_name() {
        let workload_with_valid_missing_agent_name: WorkloadNamed =
            generate_test_workload_with_param("", "runtime");

        assert!(
            workload_with_valid_missing_agent_name
                .validate_fields_format()
                .is_ok()
        );
    }

    // [utest->swdd~api-workload-naming-convention~1]
    #[test]
    fn utest_validate_workload_name_format_inordinately_long_workload_name() {
        let workload_name = "workload_name_is_too_long_for_ankaios_to_accept_it_and_I_don_t_know_what_else_to_write".to_string();
        assert_eq!(
            validate_workload_name_format(&workload_name),
            Err(format!(
                "Unsupported workload name '{}'. Length {} exceeds the maximum limit of {} characters.",
                workload_name,
                workload_name.len(),
                super::MAX_CHARACTERS_WORKLOAD_NAME,
            ))
        );
    }

    // [utest->swdd~api-access-rules-logs-workload-names-convention~1]
    #[test]
    fn utest_validate_wildcard_workload_name_format_success() {
        let workload_name = "valid*Workload_Name-1";
        assert!(super::validate_wildcard_workload_name_format(workload_name, 5).is_ok());
    }

    // [utest->swdd~api-access-rules-logs-workload-names-convention~1]
    #[test]
    fn utest_validate_wildcard_workload_name_format_failure() {
        let workload_name = "inva!lid*Workload+Name@1";
        assert_eq!(
            super::validate_wildcard_workload_name_format(workload_name, 6),
            Err(format!(
                "Unsupported workload name with wildcard '{}'. Expected to have characters in {}.",
                workload_name,
                super::ALLOWED_SYMBOLS
            ))
        );
    }
}

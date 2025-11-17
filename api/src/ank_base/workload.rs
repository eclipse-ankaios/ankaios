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

use crate::ank_base::{
    AddCondition, DependenciesSpec, ExecutionStateSpec, RestartPolicy, WorkloadInstanceNameSpec,
    WorkloadSpec,
};
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

impl From<WorkloadNamed> for WorkloadSpec {
    fn from(item: WorkloadNamed) -> Self {
        item.workload
    }
}

// [impl->swdd~common-workload-naming-convention~1]
pub fn verify_workload_name_format(workload_name: &str) -> Result<(), String> {
    let length = workload_name.len();
    verify_workload_name_pattern(workload_name)
        .and_then(|_| verify_workload_name_not_empty(length))
        .and_then(|_| verify_workload_name_length(length))
        .map_err(|err| format!("Unsupported workload name '{workload_name}'. {err}"))
}

pub fn verify_workload_name_pattern(workload_name: &str) -> Result<(), String> {
    let re_workloads = Regex::new(STR_RE_WORKLOAD).unwrap();
    if !re_workloads.is_match(workload_name) {
        Err(format!("Expected to have characters in {ALLOWED_SYMBOLS}."))
    } else {
        Ok(())
    }
}

pub fn verify_workload_name_length(length: usize) -> Result<(), String> {
    if length > MAX_CHARACTERS_WORKLOAD_NAME {
        Err(format!(
            "Length {length} exceeds the maximum limit of {MAX_CHARACTERS_WORKLOAD_NAME} characters."
        ))
    } else {
        Ok(())
    }
}

pub fn verify_workload_name_not_empty(length: usize) -> Result<(), String> {
    if length == 0 {
        Err("Is empty.".into())
    } else {
        Ok(())
    }
}

// [impl->swdd~common-agent-naming-convention~3]
fn verify_agent_name_format(agent_name: &str) -> Result<(), String> {
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
    pub fn needs_control_interface(&self) -> bool {
        !self.control_interface_access.allow_rules.is_empty()
    }

    // [impl->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
    pub fn verify_config_reference_format(
        config_references: &HashMap<String, String>,
    ) -> Result<(), String> {
        let re_config_references = Regex::new(STR_RE_CONFIG_REFERENCES).unwrap();
        for (config_alias, referenced_config) in config_references {
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

impl From<HashMap<String, AddCondition>> for DependenciesSpec {
    fn from(value: HashMap<String, AddCondition>) -> Self {
        DependenciesSpec {
            dependencies: value,
        }
    }
}

impl WorkloadNamed {
    // [impl->swdd~common-workload-naming-convention~1]
    // [impl->swdd~common-agent-naming-convention~3]
    // [impl->swdd~common-access-rules-filter-mask-convention~1]
    pub fn verify_fields_format(&self) -> Result<(), String> {
        verify_workload_name_format(self.instance_name.workload_name())?;
        verify_agent_name_format(self.instance_name.agent_name())?;
        verify_agent_name_format(&self.workload.agent)?;
        self.workload.control_interface_access.verify_format()?;
        Ok(())
    }
}

pub trait FulfilledBy<T> {
    fn fulfilled_by(&self, other: &T) -> bool;
}

// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct DeletedWorkload {
    pub instance_name: WorkloadInstanceNameSpec,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub dependencies: HashMap<String, DeleteCondition>,
}

// [impl->swdd~workload-delete-conditions-for-dependencies~1]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeleteCondition {
    DelCondRunning = 0,
    DelCondNotPendingNorRunning = 1,
}

impl FulfilledBy<ExecutionStateSpec> for AddCondition {
    // [impl->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    fn fulfilled_by(&self, other: &ExecutionStateSpec) -> bool {
        match self {
            AddCondition::AddCondRunning => (*other).is_running(),
            AddCondition::AddCondSucceeded => (*other).is_succeeded(),
            AddCondition::AddCondFailed => (*other).is_failed(),
        }
    }
}

impl FulfilledBy<ExecutionStateSpec> for DeleteCondition {
    // [impl->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    fn fulfilled_by(&self, other: &ExecutionStateSpec) -> bool {
        if other.is_waiting_to_start() {
            return true;
        }

        match self {
            DeleteCondition::DelCondNotPendingNorRunning => (*other).is_not_pending_nor_running(),
            DeleteCondition::DelCondRunning => (*other).is_running(),
        }
    }
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

impl std::fmt::Display for RestartPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestartPolicy::Never => write!(f, "Never"),
            RestartPolicy::OnFailure => write!(f, "OnFailure"),
            RestartPolicy::Always => write!(f, "Always"),
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
    ConfigMappingsSpec, FileContentSpec, FileSpec, FilesSpec, TagsSpec, Workload,
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
        WorkloadSpec {
            agent: agent_name.into(),
            dependencies: DependenciesSpec {
                dependencies: HashMap::from([
                    (String::from("workload_B"), AddCondition::AddCondRunning),
                    (String::from("workload_C"), AddCondition::AddCondSucceeded),
                ]),
            },
            restart_policy: RestartPolicy::Always,
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

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {
    use crate::ank_base::{
        AddCondition, DeleteCondition, ExecutionStateSpec, FulfilledBy, RestartPolicy,
        WorkloadNamed, WorkloadSpec, verify_workload_name_format,
    };
    use crate::test_utils::{
        generate_test_deleted_workload, generate_test_workload, generate_test_workload_with_param,
    };
    use std::collections::HashMap;

    // one test for a failing case, other cases are tested on the caller side to not repeat test code
    // [utest->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
    #[test]
    fn utest_verify_config_reference_format_invalid_config_reference_key() {
        let invalid_config_reference_key = "invalid%key";
        let mut configs = HashMap::new();
        configs.insert(
            "config_alias_1".to_owned(),
            invalid_config_reference_key.to_owned(),
        );
        assert_eq!(
            WorkloadSpec::verify_config_reference_format(&configs),
            Err(format!(
                "Unsupported config reference key. Received '{}', expected to have characters in {}",
                invalid_config_reference_key,
                super::STR_RE_CONFIG_REFERENCES
            ))
        );
    }

    // [utest->swdd~workload-add-conditions-for-dependencies~1]
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

    // [utest->swdd~workload-delete-conditions-for-dependencies~1]
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

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    #[test]
    fn utest_add_condition_fulfilled_by_fulfilled() {
        let add_condition = AddCondition::AddCondRunning;
        assert!(add_condition.fulfilled_by(&ExecutionStateSpec::running()));

        let add_condition = AddCondition::AddCondSucceeded;
        assert!(add_condition.fulfilled_by(&ExecutionStateSpec::succeeded()));

        let add_condition = AddCondition::AddCondFailed;
        assert!(
            add_condition.fulfilled_by(&ExecutionStateSpec::failed("some failure".to_string()))
        );
    }

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_condition_fulfilled_by() {
        let delete_condition = DeleteCondition::DelCondNotPendingNorRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionStateSpec::succeeded()));

        let delete_condition = DeleteCondition::DelCondRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionStateSpec::running()));

        let delete_condition = DeleteCondition::DelCondNotPendingNorRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionStateSpec::waiting_to_start()));
    }

    // [utest->swdd~agent-supports-restart-policies~1]
    #[test]
    fn utest_restart_to_int() {
        assert_eq!(RestartPolicy::try_from(0).unwrap(), RestartPolicy::Never);
        assert_eq!(
            RestartPolicy::try_from(1).unwrap(),
            RestartPolicy::OnFailure
        );
        assert_eq!(RestartPolicy::try_from(2).unwrap(), RestartPolicy::Always);

        assert_eq!(
            RestartPolicy::try_from(100),
            Err(prost::UnknownEnumValue(100))
        );
    }

    #[test]
    fn utest_restart_display() {
        assert_eq!(RestartPolicy::Never.to_string(), "Never");
        assert_eq!(RestartPolicy::OnFailure.to_string(), "OnFailure");
        assert_eq!(RestartPolicy::Always.to_string(), "Always");
    }

    // [utest->swdd~common-workload-needs-control-interface~1]
    #[test]
    fn utest_needs_control_interface() {
        let mut workload: WorkloadSpec = generate_test_workload(); // Generates with control interface access
        assert!(workload.needs_control_interface());

        workload.control_interface_access = Default::default(); // No rules
        assert!(!workload.needs_control_interface());
    }

    // [utest->swdd~common-workload-naming-convention~1]
    // [utest->swdd~common-agent-naming-convention~3]
    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_workload_verify_fields_format_success() {
        let compatible_workload: WorkloadNamed = generate_test_workload();
        assert!(compatible_workload.verify_fields_format().is_ok());
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_verify_workload_name_format_empty_workload_name() {
        let workload_name = "".to_string();
        assert_eq!(
            verify_workload_name_format(&workload_name),
            Err("Unsupported workload name ''. Is empty.".into())
        );
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_workload_verify_fields_incompatible_workload_name() {
        let workload_with_wrong_name =
            generate_test_workload::<WorkloadNamed>().name("incompatible.workload_name");

        assert_eq!(
            workload_with_wrong_name.verify_fields_format(),
            Err(format!(
                "Unsupported workload name '{}'. Expected to have characters in {}.",
                workload_with_wrong_name.instance_name.workload_name(),
                super::ALLOWED_SYMBOLS
            ))
        );
    }

    // [utest->swdd~common-agent-naming-convention~3]
    #[test]
    fn utest_workload_verify_fields_incompatible_agent_name() {
        let workload_with_wrong_agent_name: WorkloadNamed =
            generate_test_workload_with_param("incompatible.agent_name", "runtime");

        assert_eq!(
            workload_with_wrong_agent_name.verify_fields_format(),
            Err(format!(
                "Unsupported agent name. Received '{}', expected to have characters in {}",
                workload_with_wrong_agent_name.instance_name.agent_name(),
                super::ALLOWED_SYMBOLS
            ))
        );
    }

    // [utest->swdd~common-agent-naming-convention~3]
    #[test]
    fn utest_workload_with_valid_empty_agent_name() {
        let workload_with_valid_missing_agent_name: WorkloadNamed =
            generate_test_workload_with_param("", "runtime");

        assert!(
            workload_with_valid_missing_agent_name
                .verify_fields_format()
                .is_ok()
        );
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_verify_workload_name_format_inordinately_long_workload_name() {
        let workload_name = "workload_name_is_too_long_for_ankaios_to_accept_it_and_I_don_t_know_what_else_to_write".to_string();
        assert_eq!(
            verify_workload_name_format(&workload_name),
            Err(format!(
                "Unsupported workload name '{}'. Length {} exceeds the maximum limit of {} characters.",
                workload_name,
                workload_name.len(),
                super::MAX_CHARACTERS_WORKLOAD_NAME,
            ))
        );
    }
}

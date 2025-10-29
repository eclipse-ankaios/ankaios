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

#[cfg(any(feature = "test_utils", test))]
use crate::ank_base::TagsInternal;
use crate::ank_base::{
    AddCondition, DependenciesInternal, ExecutionStateInternal, Workload,
    WorkloadInstanceNameInternal, WorkloadInternal, RestartPolicy,
};
use crate::helpers::serialize_to_ordered_map;
#[cfg(any(feature = "test_utils", test))]
use crate::test_utils::generate_test_control_interface_access;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MAX_CHARACTERS_WORKLOAD_NAME: usize = 63;
pub const ALLOWED_SYMBOLS: &str = "[a-zA-Z0-9_-]";
pub const STR_RE_WORKLOAD: &str = r"^[a-zA-Z0-9_-]*$";
pub const STR_RE_AGENT: &str = r"^[a-zA-Z0-9_-]*$";
pub const STR_RE_CONFIG_REFERENCES: &str = r"^[a-zA-Z0-9_-]*$";

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

impl WorkloadInternal {
    pub fn needs_control_interface(&self) -> bool {
        !self.control_interface_access.allow_rules.is_empty()
    }

    // [impl->swdd~common-workload-has-files~1]
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    // [impl->swdd~common-workload-naming-convention~1]
    // [impl->swdd~common-agent-naming-convention~3]
    // [impl->swdd~common-access-rules-filter-mask-convention~1]
    pub fn verify_fields_format(&self) -> Result<(), String> {
        verify_workload_name_format(self.instance_name.workload_name())?;
        verify_agent_name_format(self.instance_name.agent_name())?;
        self.control_interface_access.verify_format()?;
        Ok(())
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

impl TryFrom<(String, Workload)> for WorkloadInternal {
    type Error = String;

    fn try_from((name, orig): (String, Workload)) -> Result<Self, Self::Error> {
        let agent = orig
            .agent
            .clone()
            .ok_or_else(|| "Missing field agent".to_string())?;
        let res =
            WorkloadInternal {
                instance_name: WorkloadInstanceNameInternal::builder()
                    .workload_name(name)
                    .agent_name(agent.clone())
                    .config(
                        orig.runtime_config
                            .as_ref()
                            .ok_or_else(|| "Missing field runtime_config".to_string())?,
                    )
                    .build(),
                agent,
                tags: match orig.tags {
                    Some(t) => t.try_into()?,
                    None => Default::default(),
                },
                dependencies: match orig.dependencies {
                    Some(deps) => deps.try_into()?,
                    None => Default::default(),
                },
                restart_policy: orig.restart_policy.unwrap_or_default().try_into().map_err(
                    |e| {
                        format!(
                            "Failed to convert restart_policy '{:?}': {e}",
                            orig.restart_policy
                        )
                    },
                )?,
                runtime: orig
                    .runtime
                    .as_ref()
                    .ok_or_else(|| "Missing field runtime".to_string())?
                    .to_string(),
                runtime_config: orig
                    .runtime_config
                    .as_ref()
                    .ok_or_else(|| "Missing field runtime config".to_string())?
                    .clone(),
                files: match orig.files {
                    Some(files) => files.try_into()?,
                    None => Default::default(),
                },
                configs: match orig.configs {
                    Some(configs) => configs.try_into()?,
                    None => Default::default(),
                },
                // control_interface_access: orig
                //     .control_interface_access
                //     .clone()
                //     .ok_or_else(|| "Missing field control_interface_access".to_string())?
                //     .try_into()
                //     .map_err(|e| {
                //         format!(
                //             "Failed to convert control_interface_access '{:?}': {e}",
                //             orig.control_interface_access
                //         )
                //     })?,
                control_interface_access: orig
                    .control_interface_access
                    .unwrap_or_default()
                    .try_into()?,
            };
        Ok(res)
    }
}

impl From<HashMap<String, AddCondition>> for DependenciesInternal {
    fn from(value: HashMap<String, AddCondition>) -> Self {
        DependenciesInternal {
            dependencies: value,
        }
    }
}

pub trait FulfilledBy<T> {
    fn fulfilled_by(&self, other: &T) -> bool;
}

// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct DeletedWorkload {
    pub instance_name: WorkloadInstanceNameInternal,
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

impl FulfilledBy<ExecutionStateInternal> for AddCondition {
    // [impl->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    fn fulfilled_by(&self, other: &ExecutionStateInternal) -> bool {
        match self {
            AddCondition::AddCondRunning => (*other).is_running(),
            AddCondition::AddCondSucceeded => (*other).is_succeeded(),
            AddCondition::AddCondFailed => (*other).is_failed(),
        }
    }
}

impl FulfilledBy<ExecutionStateInternal> for DeleteCondition {
    // [impl->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    fn fulfilled_by(&self, other: &ExecutionStateInternal) -> bool {
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "test_utils", test))]
use crate::ank_base::{ConfigMappingsInternal, FilesInternal};

#[cfg(any(feature = "test_utils", test))]
fn generate_test_dependencies() -> DependenciesInternal {
    DependenciesInternal {
        dependencies: HashMap::from([
            (String::from("workload_A"), AddCondition::AddCondRunning),
            (String::from("workload_C"), AddCondition::AddCondSucceeded),
        ]),
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_runtime_config() -> String {
    "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n".to_string()
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_with_param(
    agent_name: impl Into<String>,
    workload_name: impl Into<String>,
    runtime_name: impl Into<String>,
) -> WorkloadInternal {
    let runtime_config = generate_test_runtime_config();

    generate_test_workload_with_runtime_config(
        agent_name,
        workload_name,
        runtime_name,
        runtime_config,
    )
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_with_runtime_config(
    agent_name: impl Into<String>,
    workload_name: impl Into<String>,
    runtime_name: impl Into<String>,
    runtime_config: impl Into<String>,
) -> WorkloadInternal {
    let agent_name = agent_name.into();
    let runtime_config = runtime_config.into();
    let instance_name = WorkloadInstanceNameInternal::builder()
        .agent_name(agent_name.clone())
        .workload_name(workload_name)
        .config(&runtime_config)
        .build();

    WorkloadInternal {
        instance_name,
        agent: agent_name,
        dependencies: generate_test_dependencies(),
        restart_policy: RestartPolicy::Always,
        runtime: runtime_name.into(),
        tags: TagsInternal {
            tags: HashMap::from([("key".into(), "value".into())]),
        },
        runtime_config,
        configs: ConfigMappingsInternal {
            configs: HashMap::from([
                ("ref1".into(), "config_1".into()),
                ("ref2".into(), "config_2".into()),
            ]),
        },
        control_interface_access: Default::default(),
        files: Default::default(),
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_with_control_interface_access(
    agent_name: impl Into<String>,
    workload_name: impl Into<String>,
    runtime_name: impl Into<String>,
) -> WorkloadInternal {
    let mut workload_spec =
        generate_test_workload_with_param(agent_name, workload_name, runtime_name);
    workload_spec.control_interface_access = generate_test_control_interface_access();
    workload_spec
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_with_files(
    agent_name: impl Into<String>,
    workload_name: impl Into<String>,
    runtime_name: impl Into<String>,
    files: FilesInternal,
) -> WorkloadInternal {
    let mut workload_spec =
        generate_test_workload_with_param(agent_name, workload_name, runtime_name);
    workload_spec.files = files;
    workload_spec
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload() -> WorkloadInternal {
    generate_test_workload_with_param("agent", "name", "runtime")
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_with_dependencies(
    agent_name: impl Into<String>,
    workload_name: impl Into<String>,
    runtime_name: impl Into<String>,
    dependencies: HashMap<String, AddCondition>,
) -> WorkloadInternal {
    let mut workload_spec =
        generate_test_workload_with_param(agent_name, workload_name, runtime_name);
    workload_spec.dependencies = DependenciesInternal {
        dependencies: dependencies.iter().map(|(k, v)| (k.clone(), *v)).collect(),
    };
    workload_spec
}

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {
    use crate::ank_base::{
        AddCondition, DeleteCondition, ExecutionStateInternal, FulfilledBy, RestartPolicy,
        WorkloadInternal, verify_workload_name_format,
    };
    use crate::test_utils::{
        generate_test_control_interface_access, generate_test_deleted_workload,
        generate_test_rendered_workload_files, generate_test_workload,
        generate_test_workload_with_files, generate_test_workload_with_param,
    };
    use std::collections::HashMap;

    const RUNTIME: &str = "runtime";

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
            WorkloadInternal::verify_config_reference_format(&configs),
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
        assert!(add_condition.fulfilled_by(&ExecutionStateInternal::running()));

        let add_condition = AddCondition::AddCondSucceeded;
        assert!(add_condition.fulfilled_by(&ExecutionStateInternal::succeeded()));

        let add_condition = AddCondition::AddCondFailed;
        assert!(
            add_condition.fulfilled_by(&ExecutionStateInternal::failed("some failure".to_string()))
        );
    }

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_condition_fulfilled_by() {
        let delete_condition = DeleteCondition::DelCondNotPendingNorRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionStateInternal::succeeded()));

        let delete_condition = DeleteCondition::DelCondRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionStateInternal::running()));

        let delete_condition = DeleteCondition::DelCondNotPendingNorRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionStateInternal::waiting_to_start()));
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
        let mut workload_spec = generate_test_workload();
        assert!(!workload_spec.needs_control_interface());

        workload_spec.control_interface_access = generate_test_control_interface_access();
        assert!(workload_spec.needs_control_interface());
    }

    // [utest->swdd~common-workload-naming-convention~1]
    // [utest->swdd~common-agent-naming-convention~3]
    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_workload_verify_fields_format_success() {
        let compatible_workload_spec = generate_test_workload();
        assert!(compatible_workload_spec.verify_fields_format().is_ok());
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
        let spec_with_wrong_workload_name = generate_test_workload_with_param(
            "agent_A".to_owned(),
            "incompatible.workload_name".to_owned(),
            RUNTIME.to_owned(),
        );

        assert_eq!(
            spec_with_wrong_workload_name.verify_fields_format(),
            Err(format!(
                "Unsupported workload name '{}'. Expected to have characters in {}.",
                spec_with_wrong_workload_name.instance_name.workload_name(),
                super::ALLOWED_SYMBOLS
            ))
        );
    }

    // [utest->swdd~common-agent-naming-convention~3]
    #[test]
    fn utest_workload_verify_fields_incompatible_agent_name() {
        let spec_with_wrong_agent_name = generate_test_workload_with_param(
            "incompatible.agent_name".to_owned(),
            "workload_1".to_owned(),
            RUNTIME.to_owned(),
        );

        assert_eq!(
            spec_with_wrong_agent_name.verify_fields_format(),
            Err(format!(
                "Unsupported agent name. Received '{}', expected to have characters in {}",
                spec_with_wrong_agent_name.instance_name.agent_name(),
                super::ALLOWED_SYMBOLS
            ))
        );
    }

    // [utest->swdd~common-agent-naming-convention~3]
    #[test]
    fn utest_workload_spec_with_valid_empty_agent_name() {
        let spec_with_wrong_agent_name = generate_test_workload_with_param(
            "".to_owned(),
            "workload_1".to_owned(),
            RUNTIME.to_owned(),
        );

        assert!(spec_with_wrong_agent_name.verify_fields_format().is_ok());
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

    // [utest->swdd~common-workload-has-files~1]
    #[test]
    fn utest_workload_has_files() {
        let workload_spec = generate_test_workload_with_files(
            "agent",
            "name",
            "runtime",
            generate_test_rendered_workload_files(),
        );
        assert!(workload_spec.has_files());
    }

    // [utest->swdd~common-workload-has-files~1]
    #[test]
    fn utest_workload_has_files_empty() {
        let workload_spec = generate_test_workload();
        assert!(!workload_spec.has_files());
    }
}

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

use crate::ank_base::{AddCondition, ExecutionStateSpec, WorkloadInstanceNameSpec, WorkloadSpec};
use crate::helpers::{
    serialize_to_ordered_map, validate_field_not_empty, validate_field_pattern,
    validate_max_field_length, validate_max_length_filter,
};
use crate::{CONSTRAINT_FIELD_DESCRIPTION, CURRENT_API_VERSION, PREVIOUS_API_VERSION};

use serde::{Deserialize, Serialize};

use serde_yaml::Value;
use std::collections::HashMap;

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

// [impl->swdd~common-access-rules-logs-workload-names-convention~1]
pub fn validate_wildcard_workload_filter_format(
    workload_filter: &str,
    wildcard_pos: usize,
) -> Result<(), String> {
    let prefix = &workload_filter[..wildcard_pos];
    let suffix = &workload_filter[wildcard_pos + 1..];

    validate_field_not_empty(workload_filter)
        .and_then(|_| validate_max_length_filter(workload_filter))
        .and_then(|_| validate_field_pattern(prefix))
        .and_then(|_| validate_field_pattern(suffix))
        .map_err(|err| {
            format!("Unsupported workload name filter with wildcard '{workload_filter}'. {err}")
        })
}

// [impl->swdd~common-workload-naming-convention~1]
pub fn validate_workload_name_format(workload_name: &str) -> Result<(), String> {
    validate_field_not_empty(workload_name)
        .and_then(|_| validate_field_pattern(workload_name))
        .and_then(|_| validate_max_field_length(workload_name))
        .map_err(|err| format!("Unsupported workload name '{workload_name}'. {err}"))
}

// [impl->swdd~common-agent-naming-convention~3]
fn validate_agent_name_format(agent_name: &str) -> Result<(), String> {
    // Empty agent names are allowed and indicate a not scheduled workload
    if validate_field_pattern(agent_name).is_err() {
        Err(format!(
            "Unsupported agent name '{agent_name}'. {CONSTRAINT_FIELD_DESCRIPTION}"
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
    pub fn validate_config_reference_format(&self) -> Result<(), String> {
        for (config_alias, referenced_config) in self.configs.configs.iter() {
            validate_field_not_empty(config_alias)
                .and_then(|_| validate_max_field_length(config_alias))
                .and_then(|_| validate_field_pattern(config_alias))
                .map_err(|_| {
                    format!(
                        "Unsupported config alias '{config_alias}'. {CONSTRAINT_FIELD_DESCRIPTION}"
                    )
                })?;

            validate_field_not_empty(referenced_config)
                .and_then(|_| validate_max_field_length(referenced_config))
                .and_then(|_| validate_field_pattern(referenced_config))
                .map_err(|_| format!("Unsupported config reference key '{referenced_config}'. {CONSTRAINT_FIELD_DESCRIPTION}"))?;
        }
        Ok(())
    }
}

impl WorkloadNamed {
    // [impl->swdd~common-workload-naming-convention~1]
    // [impl->swdd~common-agent-naming-convention~3]
    // [impl->swdd~common-access-rules-filter-mask-convention~1]
    pub fn validate_fields_format(&self) -> Result<(), String> {
        validate_workload_name_format(self.instance_name.workload_name())?;
        validate_agent_name_format(&self.workload.agent)?;
        if self.instance_name.agent_name() != self.workload.agent {
            return Err(format!(
                "Internal error. Mismatch between workload instance name agent '{}' and workload agent field '{}'.",
                self.instance_name.agent_name(),
                self.workload.agent
            ));
        }
        self.workload.control_interface_access.validate_format()?;
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

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {
    use super::{CONSTRAINT_FIELD_DESCRIPTION, validate_workload_name_format};

    use crate::ank_base::{
        AddCondition, DeleteCondition, ExecutionStateSpec, FulfilledBy, RestartPolicy,
    };
    use crate::test_utils::generate_test_deleted_workload_with_params;
    use crate::test_utils::{
        fixtures, generate_test_workload, generate_test_workload_named,
        generate_test_workload_named_with_params,
    };

    // one test for a failing case, other cases are tested on the caller side to not repeat test code
    // [utest->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
    #[test]
    fn utest_validate_config_reference_format_invalid_config_reference_key() {
        let invalid_config_reference_key = "invalid%key";
        let mut workload_spec = generate_test_workload();
        workload_spec.configs.configs.insert(
            "config_alias_1".to_owned(),
            invalid_config_reference_key.to_owned(),
        );
        assert_eq!(
            workload_spec.validate_config_reference_format(),
            Err(format!(
                "Unsupported config reference key '{invalid_config_reference_key}'. {CONSTRAINT_FIELD_DESCRIPTION}",
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
        let mut deleted_workload = generate_test_deleted_workload_with_params(
            "agent X".to_string(),
            "workload X".to_string(),
        );

        deleted_workload.dependencies.insert(
            fixtures::WORKLOAD_NAMES[2].to_owned(),
            DeleteCondition::DelCondNotPendingNorRunning,
        );

        let serialized_deleted_workload = serde_yaml::to_string(&deleted_workload).unwrap();
        let indices = [
            serialized_deleted_workload
                .find(fixtures::WORKLOAD_NAMES[0])
                .unwrap(),
            serialized_deleted_workload
                .find(fixtures::WORKLOAD_NAMES[2])
                .unwrap(),
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

    // [utest->swdd~common-workload-needs-control-interface~1]
    #[test]
    fn utest_needs_control_interface() {
        let mut workload = generate_test_workload(); // Generates with control interface access
        assert!(workload.needs_control_interface());

        workload.control_interface_access = Default::default(); // No rules
        assert!(!workload.needs_control_interface());
    }

    // [utest->swdd~common-workload-naming-convention~1]
    // [utest->swdd~common-agent-naming-convention~3]
    // [utest->swdd~common-access-rules-filter-mask-convention~1]
    #[test]
    fn utest_workload_validate_fields_format_success() {
        let compatible_workload = generate_test_workload_named();
        assert!(compatible_workload.validate_fields_format().is_ok());
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_validate_workload_name_format_empty_workload_name() {
        let workload_name = "".to_string();
        assert_eq!(
            validate_workload_name_format(&workload_name),
            Err("Unsupported workload name ''. Is empty.".into())
        );
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_workload_validate_fields_incompatible_workload_name() {
        let workload_with_wrong_name = generate_test_workload_named_with_params(
            "incompatible.workload_name",
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        assert_eq!(
            workload_with_wrong_name.validate_fields_format(),
            Err(format!(
                "Unsupported workload name '{}'. Expected to have characters in {}.",
                workload_with_wrong_name.instance_name.workload_name(),
                crate::ALLOWED_CHAR_SET
            ))
        );
    }

    // [utest->swdd~common-agent-naming-convention~3]
    #[test]
    fn utest_workload_validate_fields_incompatible_agent_name() {
        let workload_with_wrong_agent_name = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            "incompatible.agent_name",
            fixtures::RUNTIME_NAMES[0],
        );

        assert_eq!(
            workload_with_wrong_agent_name.validate_fields_format(),
            Err(format!(
                "Unsupported agent name '{}'. {}",
                workload_with_wrong_agent_name.instance_name.agent_name(),
                CONSTRAINT_FIELD_DESCRIPTION
            ))
        );
    }

    // [utest->swdd~common-agent-naming-convention~3]
    #[test]
    fn utest_workload_with_valid_empty_agent_name() {
        let workload_with_valid_missing_agent_name = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            "",
            fixtures::RUNTIME_NAMES[0],
        );

        assert!(
            workload_with_valid_missing_agent_name
                .validate_fields_format()
                .is_ok()
        );
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_validate_workload_name_format_inordinately_long_workload_name() {
        let workload_name = "workload_name_is_too_long_for_ankaios_to_accept_it_and_I_don_t_know_what_else_to_write".to_string();
        assert_eq!(
            validate_workload_name_format(&workload_name),
            Err(format!(
                "Unsupported workload name '{}'. Length {} exceeds the maximum limit of {} characters.",
                workload_name,
                workload_name.len(),
                crate::MAX_FIELD_LENGTH,
            ))
        );
    }

    #[test]
    fn utest_validate_wildcard_workload_name_format_success() {
        let workload_name = "valid*Workload_Name-1";
        assert!(super::validate_wildcard_workload_filter_format(workload_name, 5).is_ok());
    }

    #[test]
    fn utest_validate_wildcard_workload_name_format_failure() {
        let workload_name = "inva!lid*Workload+Name@1";
        assert_eq!(
            super::validate_wildcard_workload_filter_format(workload_name, 6),
            Err(format!(
                "Unsupported workload name filter with wildcard '{}'. Expected to have characters in {}.",
                workload_name,
                crate::ALLOWED_CHAR_SET
            ))
        );
    }
}

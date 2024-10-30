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

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::helpers::serialize_to_ordered_map;
use crate::objects::Tag;

use super::control_interface_access::ControlInterfaceAccess;
use super::ExecutionState;
use super::WorkloadInstanceName;

pub type WorkloadCollection = Vec<WorkloadSpec>;
pub type DeletedWorkloadCollection = Vec<DeletedWorkload>;

const MAX_CHARACTERS_WORKLOAD_NAME: usize = 63;
pub const STR_RE_WORKLOAD: &str = r"^[a-zA-Z0-9_-]+*$";
pub const STR_RE_AGENT: &str = r"^[a-zA-Z0-9_-]*$";

// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DeletedWorkload {
    pub instance_name: WorkloadInstanceName,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub dependencies: HashMap<String, DeleteCondition>,
}

// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkloadSpec {
    pub instance_name: WorkloadInstanceName,
    pub tags: Vec<Tag>,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub dependencies: HashMap<String, AddCondition>,
    pub restart_policy: RestartPolicy,
    pub runtime: String,
    pub runtime_config: String,
    pub control_interface_access: ControlInterfaceAccess,
}

// [impl->swdd~common-workload-needs-control-interface~1]
impl WorkloadSpec {
    pub fn needs_control_interface(&self) -> bool {
        !self.control_interface_access.allow_rules.is_empty()
    }

    // [impl->swdd~common-workload-naming-convention~1]
    // [impl->swdd~common-agent-naming-convention~1]
    pub fn verify_fields_format(workload_spec: &WorkloadSpec) -> Result<(), String> {
        Self::verify_workload_name_format(workload_spec.instance_name.workload_name())?;
        Self::verify_agent_name_format(workload_spec.instance_name.agent_name())?;
        Ok(())
    }

    // [impl->swdd~common-workload-naming-convention~1]
    fn verify_workload_name_format(workload_name: &str) -> Result<(), String> {
        let re_workloads = Regex::new(STR_RE_WORKLOAD).unwrap();
        if !re_workloads.is_match(workload_name) {
            return Err(format!(
                "Unsupported workload name. Received '{}', expected to have characters in {}",
                workload_name, STR_RE_WORKLOAD
            ));
        }

        if workload_name.len() > MAX_CHARACTERS_WORKLOAD_NAME {
            Err(format!(
                "Workload name length {} exceeds the maximum limit of {} characters",
                workload_name.len(),
                MAX_CHARACTERS_WORKLOAD_NAME
            ))
        } else {
            Ok(())
        }
    }

    // [impl->swdd~common-agent-naming-convention~1]
    fn verify_agent_name_format(agent_name: &str) -> Result<(), String> {
        let re_agent = Regex::new(STR_RE_AGENT).unwrap();
        if !re_agent.is_match(agent_name) {
            Err(format!(
                "Unsupported agent name. Received '{}', expected to have characters in {}",
                agent_name, STR_RE_AGENT
            ))
        } else {
            Ok(())
        }
    }
}

pub type AgentWorkloadMap = HashMap<String, (WorkloadCollection, DeletedWorkloadCollection)>;

pub fn get_workloads_per_agent(
    added_workloads: WorkloadCollection,
    deleted_workloads: DeletedWorkloadCollection,
) -> AgentWorkloadMap {
    let mut agent_workloads: AgentWorkloadMap = HashMap::new();

    for added_workload in added_workloads {
        if let Some((added_workload_vector, _)) =
            agent_workloads.get_mut(added_workload.instance_name.agent_name())
        {
            added_workload_vector.push(added_workload);
        } else if !added_workload.instance_name.agent_name().is_empty() {
            agent_workloads.insert(
                added_workload.instance_name.agent_name().to_owned(),
                (vec![added_workload], vec![]),
            );
        }
    }

    for deleted_workload in deleted_workloads {
        if let Some((_, deleted_workload_vector)) =
            agent_workloads.get_mut(deleted_workload.instance_name.agent_name())
        {
            deleted_workload_vector.push(deleted_workload);
        } else {
            agent_workloads.insert(
                deleted_workload.instance_name.agent_name().to_owned(),
                (vec![], vec![deleted_workload]),
            );
        }
    }

    agent_workloads
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
// [impl->swdd~agent-supports-restart-policies~1]
pub enum RestartPolicy {
    #[default]
    Never,
    OnFailure,
    Always,
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

impl TryFrom<i32> for RestartPolicy {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            x if x == RestartPolicy::Never as i32 => Ok(RestartPolicy::Never),
            x if x == RestartPolicy::OnFailure as i32 => Ok(RestartPolicy::OnFailure),
            x if x == RestartPolicy::Always as i32 => Ok(RestartPolicy::Always),
            _ => Err(format!(
                "Received an unknown value '{value}' as restart policy."
            )),
        }
    }
}

pub trait FulfilledBy<T> {
    fn fulfilled_by(&self, other: &T) -> bool;
}

// [impl->swdd~workload-add-conditions-for-dependencies~1]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AddCondition {
    AddCondRunning = 0,
    AddCondSucceeded = 1,
    AddCondFailed = 2,
}

impl FulfilledBy<ExecutionState> for AddCondition {
    // [impl->swdd~execution-states-of-workload-dependencies-fulfill-add-conditions~1]
    fn fulfilled_by(&self, other: &ExecutionState) -> bool {
        match self {
            AddCondition::AddCondRunning => (*other).is_running(),
            AddCondition::AddCondSucceeded => (*other).is_succeeded(),
            AddCondition::AddCondFailed => (*other).is_failed(),
        }
    }
}

impl TryFrom<i32> for AddCondition {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            x if x == AddCondition::AddCondRunning as i32 => Ok(AddCondition::AddCondRunning),
            x if x == AddCondition::AddCondSucceeded as i32 => Ok(AddCondition::AddCondSucceeded),
            x if x == AddCondition::AddCondFailed as i32 => Ok(AddCondition::AddCondFailed),
            _ => Err(format!(
                "Received an unknown value '{value}' as AddCondition."
            )),
        }
    }
}

// [impl->swdd~workload-delete-conditions-for-dependencies~1]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeleteCondition {
    DelCondRunning = 0,
    DelCondNotPendingNorRunning = 1,
}

impl FulfilledBy<ExecutionState> for DeleteCondition {
    // [impl->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    fn fulfilled_by(&self, other: &ExecutionState) -> bool {
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "test_utils", test))]
use crate::objects::generate_test_control_interface_access;

#[cfg(any(feature = "test_utils", test))]
fn generate_test_dependencies() -> HashMap<String, AddCondition> {
    HashMap::from([
        (String::from("workload_A"), AddCondition::AddCondRunning),
        (String::from("workload_C"), AddCondition::AddCondSucceeded),
    ])
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_runtime_config() -> String {
    "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n".to_string()
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_spec_with_param(
    agent_name: String,
    workload_name: String,
    runtime_name: String,
) -> crate::objects::WorkloadSpec {
    let runtime_config = generate_test_runtime_config();

    generate_test_workload_spec_with_runtime_config(
        agent_name,
        workload_name,
        runtime_name,
        runtime_config,
    )
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_spec_with_runtime_config(
    agent_name: String,
    workload_name: String,
    runtime_name: String,
    runtime_config: String,
) -> crate::objects::WorkloadSpec {
    let instance_name = WorkloadInstanceName::builder()
        .agent_name(agent_name)
        .workload_name(workload_name)
        .config(&runtime_config)
        .build();

    WorkloadSpec {
        instance_name,
        dependencies: generate_test_dependencies(),
        restart_policy: RestartPolicy::Always,
        runtime: runtime_name,
        tags: vec![Tag {
            key: "key".into(),
            value: "value".into(),
        }],
        runtime_config,
        control_interface_access: Default::default(),
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_spec_with_control_interface_access(
    agent_name: String,
    workload_name: String,
    runtime_name: String,
) -> WorkloadSpec {
    let mut workload_spec = generate_test_workload_spec_with_param(
        agent_name.to_owned(),
        workload_name.to_owned(),
        runtime_name.to_owned(),
    );
    workload_spec.control_interface_access = generate_test_control_interface_access();
    workload_spec
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_spec() -> WorkloadSpec {
    generate_test_workload_spec_with_param(
        "agent".to_string(),
        "name".to_string(),
        "runtime".to_string(),
    )
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_spec_with_dependencies(
    agent_name: &str,
    workload_name: &str,
    runtime_name: &str,
    dependencies: HashMap<String, AddCondition>,
) -> WorkloadSpec {
    let mut workload_spec = generate_test_workload_spec_with_param(
        agent_name.to_owned(),
        workload_name.to_owned(),
        runtime_name.to_owned(),
    );
    workload_spec.dependencies = dependencies;
    workload_spec
}

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {
    use crate::objects::*;
    use crate::test_utils::*;
    const RUNTIME: &str = "runtime";

    #[test]
    fn utest_get_workloads_per_agent_one_agent_one_workload() {
        let added_workloads = vec![
            generate_test_workload_spec_with_param(
                "agent1".to_string(),
                "name 1".to_string(),
                "runtime1".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent1".to_string(),
                "name 2".to_string(),
                "runtime2".to_string(),
            ),
            generate_test_workload_spec_with_param(
                "agent2".to_string(),
                "name 3".to_string(),
                "runtime3".to_string(),
            ),
        ];

        let deleted_workloads = vec![
            generate_test_deleted_workload("agent1".to_string(), "workload 8".to_string()),
            generate_test_deleted_workload("agent4".to_string(), "workload 9".to_string()),
        ];

        let workload_map = get_workloads_per_agent(added_workloads, deleted_workloads);
        assert_eq!(workload_map.len(), 3);

        let (agent1_added_workloads, agent1_deleted_workloads) =
            workload_map.get("agent1").unwrap();
        assert_eq!(agent1_added_workloads.len(), 2);
        assert_eq!(agent1_deleted_workloads.len(), 1);

        let workload1 = &agent1_added_workloads[0];
        let workload2 = &agent1_added_workloads[1];
        assert_eq!(workload1.instance_name.agent_name(), "agent1");
        assert_eq!(workload1.runtime, "runtime1");
        assert_eq!(workload2.instance_name.agent_name(), "agent1");
        assert_eq!(workload2.runtime, "runtime2");

        let deleted_workload1 = &agent1_deleted_workloads[0];
        assert_eq!(deleted_workload1.instance_name.agent_name(), "agent1");
        assert_eq!(
            deleted_workload1.instance_name.workload_name(),
            "workload 8"
        );

        let (agent2_added_workloads, agent2_deleted_workloads) =
            workload_map.get("agent2").unwrap();
        assert_eq!(agent2_added_workloads.len(), 1);
        assert_eq!(agent2_deleted_workloads.len(), 0);

        let workload3 = &agent2_added_workloads[0];
        assert_eq!(workload3.instance_name.agent_name(), "agent2");
        assert_eq!(workload3.runtime, "runtime3");

        assert!(!workload_map.contains_key("agent3"));

        let (agent4_added_workloads, agent4_deleted_workloads) =
            workload_map.get("agent4").unwrap();
        assert_eq!(agent4_added_workloads.len(), 0);
        assert_eq!(agent4_deleted_workloads.len(), 1);

        let workload3 = &agent4_deleted_workloads[0];
        assert_eq!(workload3.instance_name.agent_name(), "agent4");
        assert_eq!(workload3.instance_name.workload_name(), "workload 9");
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
            Err::<AddCondition, String>(
                "Received an unknown value '100' as AddCondition.".to_string()
            )
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
        assert!(add_condition.fulfilled_by(&ExecutionState::running()));

        let add_condition = AddCondition::AddCondSucceeded;
        assert!(add_condition.fulfilled_by(&ExecutionState::succeeded()));

        let add_condition = AddCondition::AddCondFailed;
        assert!(add_condition.fulfilled_by(&ExecutionState::failed("some failure".to_string())));
    }

    // [utest->swdd~execution-states-of-workload-dependencies-fulfill-delete-conditions~1]
    #[test]
    fn utest_delete_condition_fulfilled_by() {
        let delete_condition = DeleteCondition::DelCondNotPendingNorRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionState::succeeded()));

        let delete_condition = DeleteCondition::DelCondRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionState::running()));

        let delete_condition = DeleteCondition::DelCondNotPendingNorRunning;
        assert!(delete_condition.fulfilled_by(&ExecutionState::waiting_to_start()));
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
            Err::<RestartPolicy, String>(
                "Received an unknown value '100' as restart policy.".to_string()
            )
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
        let mut workload_spec = generate_test_workload_spec();
        assert!(!workload_spec.needs_control_interface());

        workload_spec.control_interface_access = generate_test_control_interface_access();
        assert!(workload_spec.needs_control_interface());
    }

    // [utest->swdd~common-workload-naming-convention~1]
    // [utest->swdd~common-agent-naming-convention~1]
    #[test]
    fn utest_workload_verify_fields_format_success() {
        let compatible_workload_spec = generate_test_workload_spec();
        assert_eq!(
            WorkloadSpec::verify_fields_format(&compatible_workload_spec),
            Ok(())
        );
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_workload_verify_fields_incompatible_workload_name() {
        let spec_with_wrong_workload_name = generate_test_workload_spec_with_param(
            "agent_A".to_owned(),
            "incompatible.workload_name".to_owned(),
            RUNTIME.to_owned(),
        );

        assert_eq!(
            WorkloadSpec::verify_fields_format(&spec_with_wrong_workload_name),
            Err(format!(
                "Unsupported workload name. Received '{}', expected to have characters in {}",
                spec_with_wrong_workload_name.instance_name.workload_name(),
                super::STR_RE_WORKLOAD
            ))
        );
    }

    // [utest->swdd~common-agent-naming-convention~1]
    #[test]
    fn utest_workload_verify_fields_incompatible_agent_name() {
        let spec_with_wrong_agent_name = generate_test_workload_spec_with_param(
            "incompatible.agent_name".to_owned(),
            "workload_1".to_owned(),
            RUNTIME.to_owned(),
        );

        assert_eq!(
            WorkloadSpec::verify_fields_format(&spec_with_wrong_agent_name),
            Err(format!(
                "Unsupported agent name. Received '{}', expected to have characters in {}",
                spec_with_wrong_agent_name.instance_name.agent_name(),
                super::STR_RE_AGENT
            ))
        );
    }

    // [utest->swdd~common-workload-naming-convention~1]
    #[test]
    fn utest_verify_workload_name_format_inordinately_long_workload_name() {
        let workload_name = "workload_name_is_too_long_for_ankaios_to_accept_it_and_I_don_t_know_what_else_to_write".to_string();
        assert_eq!(
            WorkloadSpec::verify_workload_name_format(&workload_name),
            Err(format!(
                "Workload name length {} exceeds the maximum limit of {} characters",
                workload_name.len(),
                super::MAX_CHARACTERS_WORKLOAD_NAME,
            ))
        );
    }
}

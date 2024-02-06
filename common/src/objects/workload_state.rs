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

use std::fmt::Display;

use serde::{ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};

use api::proto;

use super::WorkloadExecutionInstanceName;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PendingSubstate {
    Initial = 0,
    WaitingToStart = 1,
    Starting = 2,
    StartingFailed = 8,
}

impl From<i32> for PendingSubstate {
    fn from(x: i32) -> Self {
        match x {
            x if x == PendingSubstate::Initial as i32 => PendingSubstate::Initial,
            x if x == PendingSubstate::WaitingToStart as i32 => PendingSubstate::WaitingToStart,
            x if x == PendingSubstate::Starting as i32 => PendingSubstate::Starting,
            _ => PendingSubstate::StartingFailed,
        }
    }
}

impl Display for PendingSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PendingSubstate::Initial => write!(f, "Initial"),
            PendingSubstate::WaitingToStart => write!(f, "WaitingToStart"),
            PendingSubstate::Starting => write!(f, "Starting"),
            PendingSubstate::StartingFailed => write!(f, "StartingFailed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunningSubstate {
    Ok = 0,
    WaitingToStop = 1,
    Stopping = 2,
    DeleteFailed = 8,
}

impl From<i32> for RunningSubstate {
    fn from(x: i32) -> Self {
        match x {
            x if x == RunningSubstate::Ok as i32 => RunningSubstate::Ok,
            x if x == RunningSubstate::WaitingToStop as i32 => RunningSubstate::WaitingToStop,
            x if x == RunningSubstate::Stopping as i32 => RunningSubstate::Stopping,
            _ => RunningSubstate::DeleteFailed,
        }
    }
}

impl Display for RunningSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunningSubstate::Ok => write!(f, "Ok"),
            RunningSubstate::WaitingToStop => write!(f, "WaitingToStop"),
            RunningSubstate::Stopping => write!(f, "Stopping"),
            RunningSubstate::DeleteFailed => write!(f, "DeleteFailed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SucceededSubstate {
    Ok = 0,
    DeleteFailed = 8,
}

impl From<i32> for SucceededSubstate {
    fn from(x: i32) -> Self {
        match x {
            x if x == SucceededSubstate::Ok as i32 => SucceededSubstate::Ok,
            _ => SucceededSubstate::DeleteFailed,
        }
    }
}

impl Display for SucceededSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SucceededSubstate::Ok => write!(f, "Ok"),
            SucceededSubstate::DeleteFailed => write!(f, "DeleteFailed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailedSubstate {
    Nok = 0,
    Unknown = 1,
    Lost = 2,
    DeleteFailed = 8,
}

impl From<i32> for FailedSubstate {
    fn from(x: i32) -> Self {
        match x {
            x if x == FailedSubstate::Nok as i32 => FailedSubstate::Nok,
            x if x == FailedSubstate::Unknown as i32 => FailedSubstate::Unknown,
            x if x == FailedSubstate::Lost as i32 => FailedSubstate::Lost,
            _ => FailedSubstate::DeleteFailed,
        }
    }
}

impl Display for FailedSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailedSubstate::Nok => write!(f, "Nok"),
            FailedSubstate::Unknown => write!(f, "Unknown"),
            FailedSubstate::Lost => write!(f, "Lost"),
            FailedSubstate::DeleteFailed => write!(f, "DeleteFailed"),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStateEnum {
    AgentDisconnected,
    Pending(PendingSubstate),
    Running(RunningSubstate),
    Succeeded(SucceededSubstate),
    Failed(FailedSubstate),
    #[default]
    NotScheduled,
    Removed,
}

impl From<ExecutionStateEnum> for proto::execution_state::ExecutionStateEnum {
    fn from(item: ExecutionStateEnum) -> Self {
        match item {
            ExecutionStateEnum::AgentDisconnected => {
                proto::execution_state::ExecutionStateEnum::AgentDisconnected(
                    proto::AgentDisconnected::AgentDisconnected as i32,
                )
            }
            ExecutionStateEnum::Pending(value) => {
                proto::execution_state::ExecutionStateEnum::Pending(value as i32)
            }
            ExecutionStateEnum::Running(value) => {
                proto::execution_state::ExecutionStateEnum::Running(value as i32)
            }
            ExecutionStateEnum::Succeeded(value) => {
                proto::execution_state::ExecutionStateEnum::Succeeded(value as i32)
            }
            ExecutionStateEnum::Failed(value) => {
                proto::execution_state::ExecutionStateEnum::Failed(value as i32)
            }
            ExecutionStateEnum::NotScheduled => {
                proto::execution_state::ExecutionStateEnum::NotScheduled(
                    proto::NotScheduled::NotScheduled as i32,
                )
            }
            ExecutionStateEnum::Removed => {
                proto::execution_state::ExecutionStateEnum::Removed(proto::Removed::Removed as i32)
            }
        }
    }
}

impl From<proto::execution_state::ExecutionStateEnum> for ExecutionStateEnum {
    fn from(item: proto::execution_state::ExecutionStateEnum) -> Self {
        match item {
            proto::execution_state::ExecutionStateEnum::AgentDisconnected(_) => {
                ExecutionStateEnum::AgentDisconnected
            }
            proto::execution_state::ExecutionStateEnum::Pending(value) => {
                ExecutionStateEnum::Pending(value.into())
            }
            proto::execution_state::ExecutionStateEnum::Running(value) => {
                ExecutionStateEnum::Running(value.into())
            }
            proto::execution_state::ExecutionStateEnum::Succeeded(value) => {
                ExecutionStateEnum::Succeeded(value.into())
            }
            proto::execution_state::ExecutionStateEnum::Failed(value) => {
                ExecutionStateEnum::Failed(value.into())
            }
            proto::execution_state::ExecutionStateEnum::NotScheduled(_) => {
                ExecutionStateEnum::NotScheduled
            }
            proto::execution_state::ExecutionStateEnum::Removed(_) => ExecutionStateEnum::Removed,
        }
    }
}

// [impl->swdd~common-supported-workload-states~1]
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]

pub struct ExecutionState {
    pub additional_info: String,
    pub state: ExecutionStateEnum,
}

impl ExecutionStateEnum {
    pub fn main_state_to_string(&self) -> String {
        match self {
            ExecutionStateEnum::AgentDisconnected => "AgentDisconnected",
            ExecutionStateEnum::Pending(_) => "Pending",
            ExecutionStateEnum::Running(_) => "Running",
            ExecutionStateEnum::Succeeded(_) => "Succeeded",
            ExecutionStateEnum::Failed(_) => "Failed",
            ExecutionStateEnum::NotScheduled => "NotScheduled",
            ExecutionStateEnum::Removed => "Removed",
        }
        .to_string()
    }

    pub fn sub_state_to_string(&self) -> Option<String> {
        match self {
            ExecutionStateEnum::Pending(substate) => Some(substate.to_string()),
            ExecutionStateEnum::Running(substate) => Some(substate.to_string()),
            ExecutionStateEnum::Succeeded(substate) => Some(substate.to_string()),
            ExecutionStateEnum::Failed(substate) => Some(substate.to_string()),
            _ => None,
        }
    }
}
impl ExecutionState {
    pub fn agent_disconnected() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::AgentDisconnected,
            ..Default::default()
        }
    }

    pub fn restart_failed_no_retry() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::StartingFailed),
            additional_info: "No more retries.".to_string(),
        }
    }

    pub fn removed() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Removed,
            ..Default::default()
        }
    }

    pub fn unknown(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Failed(FailedSubstate::Unknown),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn starting(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::Starting),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn failed(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Failed(FailedSubstate::Nok),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn succeeded() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Succeeded(SucceededSubstate::Ok),
            ..Default::default()
        }
    }

    pub fn running() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Running(RunningSubstate::Ok),
            ..Default::default()
        }
    }

    pub fn stopping(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Running(RunningSubstate::Stopping),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn lost() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Failed(FailedSubstate::Lost),
            ..Default::default()
        }
    }
}

impl From<ExecutionState> for proto::ExecutionState {
    fn from(item: ExecutionState) -> Self {
        proto::ExecutionState {
            additional_info: item.additional_info,
            execution_state_enum: Some(item.state.into()),
        }
    }
}

impl From<proto::ExecutionState> for ExecutionState {
    fn from(item: proto::ExecutionState) -> Self {
        ExecutionState {
            additional_info: item.additional_info,
            state: item
                .execution_state_enum
                .unwrap_or(proto::execution_state::ExecutionStateEnum::NotScheduled(
                    proto::NotScheduled::NotScheduled as i32,
                ))
                .into(),
        }
    }
}

impl Display for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.state.main_state_to_string())?;
        if let Some(sub_state) = self.state.sub_state_to_string() {
            write!(f, "({})", sub_state)?
        }
        if !self.additional_info.is_empty() {
            write!(f, ": '{}'", self.additional_info)
        } else {
            Ok(())
        }
    }
}

fn serialize_execution_state<S>(value: &ExecutionState, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(2))?;
    map.serialize_entry("state", &value.state.main_state_to_string())?;
    map.serialize_entry("substate", &value.state.sub_state_to_string())?;
    map.serialize_entry("additional_info", &value.additional_info)?;
    map.end()
}

fn deserialize_execution_state<'a, D>(deserializer: D) -> Result<ExecutionState, D::Error>
where
    D: Deserializer<'a>,
{
    let _buf = String::deserialize(deserializer)?;
    //TODO
    Ok(ExecutionState::default())
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkloadState {
    pub instance_name: WorkloadExecutionInstanceName,
    pub workload_id: String,
    #[serde(serialize_with = "serialize_execution_state")]
    #[serde(deserialize_with = "deserialize_execution_state")]
    pub execution_state: ExecutionState,
}

impl From<WorkloadState> for proto::WorkloadState {
    fn from(item: WorkloadState) -> Self {
        proto::WorkloadState {
            instance_name: Some(item.instance_name.into()),
            workload_id: item.workload_id,
            execution_state: Some(item.execution_state.into()),
        }
    }
}

impl From<proto::WorkloadState> for WorkloadState {
    fn from(item: proto::WorkloadState) -> Self {
        WorkloadState {
            instance_name: item.instance_name.unwrap_or_default().into(),
            workload_id: item.workload_id,
            execution_state: item.execution_state.unwrap_or_default().into(),
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
pub fn generate_test_workload_state_with_agent(
    workload_name: &str,
    agent_name: &str,
    execution_state: ExecutionState,
) -> WorkloadState {
    WorkloadState {
        instance_name: WorkloadExecutionInstanceName::builder()
            .workload_name(workload_name)
            .agent_name(agent_name)
            .config(&"config".to_string())
            .build(),
        workload_id: "some strange Id".to_string(),
        execution_state,
    }
}
#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_workload_spec(
    workload_spec: &super::WorkloadSpec,
    workload_id: &str,
    execution_state: ExecutionState,
) -> WorkloadState {
    WorkloadState {
        instance_name: WorkloadExecutionInstanceName::builder()
            .workload_name(workload_spec.name.clone())
            .agent_name(workload_spec.agent.clone())
            .config(workload_spec)
            .build(),
        workload_id: workload_id.to_string(),
        execution_state,
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state(
    workload_name: &str,
    execution_state: ExecutionState,
) -> WorkloadState {
    generate_test_workload_state_with_agent(workload_name, "agent_name", execution_state)
}

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
#[cfg(test)]
mod tests {
    use api::proto::{self, WorkloadInstanceName};

    use crate::objects::{ExecutionState, WorkloadExecutionInstanceName, WorkloadState};

    #[test]
    fn utest_converts_to_proto_workload_state() {
        let ankaios_wl_state = WorkloadState {
            execution_state: ExecutionState::running(),
            instance_name: WorkloadExecutionInstanceName::builder()
                .workload_name("john")
                .agent_name("strange")
                .build(),
            workload_id: "id2".to_string(),
        };

        let proto_wl_state = proto::WorkloadState {
            execution_state: proto::ExecutionState::ExecRunning.into(),
            instance_name: Some(WorkloadInstanceName {
                workload_name: "john".to_string(),
                agent_name: "strange".to_string(),
                config_id: "".to_string(),
            }),
            workload_id: "id2".to_string(),
        };

        assert_eq!(proto::WorkloadState::from(ankaios_wl_state), proto_wl_state);
    }

    #[test]
    fn utest_converts_to_ankaios_workload_state() {
        let ankaios_wl_state = WorkloadState {
            execution_state: ExecutionState::running(),
            instance_name: WorkloadExecutionInstanceName::builder()
                .workload_name("john")
                .agent_name("strange")
                .build(),
            workload_id: "id2".to_string(),
        };

        let proto_wl_state = proto::WorkloadState {
            execution_state: proto::ExecutionState::ExecRunning.into(),
            instance_name: Some(WorkloadInstanceName {
                workload_name: "john".to_string(),
                agent_name: "strange".to_string(),
                config_id: "".to_string(),
            }),
            workload_id: "id2".to_string(),
        };

        assert_eq!(WorkloadState::from(proto_wl_state), ankaios_wl_state);
    }

    // [utest->// [impl->swdd~common-supported-workload-states~1]]
    #[test]
    fn utest_execution_state_from_int_mapping() {
        assert_eq!(ExecutionState::ExecUnknown, ExecutionState::from(0));
        assert_eq!(ExecutionState::ExecPending, ExecutionState::from(1));
        assert_eq!(ExecutionState::ExecWaitingToStart, ExecutionState::from(2));
        assert_eq!(ExecutionState::ExecStarting, ExecutionState::from(3));
        assert_eq!(ExecutionState::ExecRunning, ExecutionState::from(4));
        assert_eq!(ExecutionState::ExecSucceeded, ExecutionState::from(5));
        assert_eq!(ExecutionState::ExecFailed, ExecutionState::from(6));
        assert_eq!(ExecutionState::ExecWaitingToStop, ExecutionState::from(7));
        assert_eq!(ExecutionState::ExecStopping, ExecutionState::from(8));
        assert_eq!(ExecutionState::ExecRemoved, ExecutionState::from(10));
        assert_eq!(ExecutionState::ExecUnknown, ExecutionState::from(100));
    }

    // [utest->// [impl->swdd~common-supported-workload-states~1]]
    #[test]
    fn utest_execution_state_to_string_basic_mapping() {
        assert_eq!(
            ExecutionState::ExecPending.to_string(),
            String::from("Pending")
        );
        assert_eq!(
            ExecutionState::ExecWaitingToStart.to_string(),
            String::from("WaitingToStart")
        );
        assert_eq!(
            ExecutionState::ExecStarting.to_string(),
            String::from("Starting")
        );
        assert_eq!(
            ExecutionState::ExecRunning.to_string(),
            String::from("Running")
        );
        assert_eq!(
            ExecutionState::ExecSucceeded.to_string(),
            String::from("Succeeded")
        );
        assert_eq!(
            ExecutionState::ExecFailed.to_string(),
            String::from("Failed")
        );
        assert_eq!(
            ExecutionState::ExecWaitingToStop.to_string(),
            String::from("WaitingToStop")
        );
        assert_eq!(
            ExecutionState::ExecRemoved.to_string(),
            String::from("Removed")
        );
        assert_eq!(
            ExecutionState::ExecStopping.to_string(),
            String::from("Stopping")
        );
        assert_eq!(
            ExecutionState::ExecUnknown.to_string(),
            String::from("Unknown")
        );
    }
}

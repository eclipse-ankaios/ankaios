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

use serde::{Deserialize, Serialize};

use api::ank_base;

use crate::std_extensions::UnreachableOption;

use super::WorkloadInstanceName;

const TRIGGERED_MSG: &str = "Triggered at runtime.";
pub const NO_MORE_RETRIES_MSG: &str = "No more retries";

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
}

impl From<i32> for RunningSubstate {
    fn from(_x: i32) -> Self {
        RunningSubstate::Ok
    }
}

impl Display for RunningSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunningSubstate::Ok => write!(f, "Ok"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StoppingSubstate {
    Stopping = 0,
    WaitingToStop = 1,
    RequestedAtRuntime = 2,
    DeleteFailed = 8,
}

impl From<i32> for StoppingSubstate {
    fn from(x: i32) -> Self {
        match x {
            x if x == StoppingSubstate::WaitingToStop as i32 => StoppingSubstate::WaitingToStop,
            x if x == StoppingSubstate::RequestedAtRuntime as i32 => {
                StoppingSubstate::RequestedAtRuntime
            }
            x if x == StoppingSubstate::DeleteFailed as i32 => StoppingSubstate::DeleteFailed,
            _ => StoppingSubstate::Stopping,
        }
    }
}

impl Display for StoppingSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoppingSubstate::Stopping => write!(f, "Stopping"),
            StoppingSubstate::WaitingToStop => write!(f, "WaitingToStop"),
            StoppingSubstate::RequestedAtRuntime => write!(f, "RequestedAtRuntime"),
            StoppingSubstate::DeleteFailed => write!(f, "DeleteFailed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SucceededSubstate {
    Ok = 0,
}

impl From<i32> for SucceededSubstate {
    fn from(_x: i32) -> Self {
        SucceededSubstate::Ok
    }
}

impl Display for SucceededSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SucceededSubstate::Ok => write!(f, "Ok"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailedSubstate {
    ExecFailed = 0,
    Unknown = 1,
    Lost = 2,
}

impl From<i32> for FailedSubstate {
    fn from(x: i32) -> Self {
        match x {
            x if x == FailedSubstate::ExecFailed as i32 => FailedSubstate::ExecFailed,
            x if x == FailedSubstate::Unknown as i32 => FailedSubstate::Unknown,
            x if x == FailedSubstate::Lost as i32 => FailedSubstate::Lost,
            _ => FailedSubstate::Unknown,
        }
    }
}

impl Display for FailedSubstate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailedSubstate::ExecFailed => write!(f, "ExecFailed"),
            FailedSubstate::Unknown => write!(f, "Unknown"),
            FailedSubstate::Lost => write!(f, "Lost"),
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", content = "subState")]
pub enum ExecutionStateEnum {
    AgentDisconnected,
    Pending(PendingSubstate),
    Running(RunningSubstate),
    Stopping(StoppingSubstate),
    Succeeded(SucceededSubstate),
    Failed(FailedSubstate),
    #[default]
    NotScheduled,
    Removed,
}

// [impl->swdd~common-workload-state-transitions~1]
impl ExecutionState {
    pub fn transition(&self, incoming: ExecutionState) -> ExecutionState {
        match (&self.state, &incoming.state) {
            (
                ExecutionStateEnum::Stopping(StoppingSubstate::RequestedAtRuntime)
                | ExecutionStateEnum::Stopping(StoppingSubstate::WaitingToStop),
                ExecutionStateEnum::Running(RunningSubstate::Ok)
                | ExecutionStateEnum::Succeeded(SucceededSubstate::Ok)
                | ExecutionStateEnum::Failed(FailedSubstate::ExecFailed)
                | ExecutionStateEnum::Failed(FailedSubstate::Lost)
                | ExecutionStateEnum::Failed(FailedSubstate::Unknown),
            ) => {
                log::trace!(
                    "Skipping transition from '{}' to '{}' state.",
                    self,
                    incoming
                );
                self.clone()
            }
            _ => incoming,
        }
    }
}

impl From<ExecutionStateEnum> for ank_base::execution_state::ExecutionStateEnum {
    fn from(item: ExecutionStateEnum) -> Self {
        match item {
            ExecutionStateEnum::AgentDisconnected => {
                ank_base::execution_state::ExecutionStateEnum::AgentDisconnected(
                    ank_base::AgentDisconnected::AgentDisconnected as i32,
                )
            }
            ExecutionStateEnum::Pending(value) => {
                ank_base::execution_state::ExecutionStateEnum::Pending(value as i32)
            }
            ExecutionStateEnum::Running(value) => {
                ank_base::execution_state::ExecutionStateEnum::Running(value as i32)
            }
            ExecutionStateEnum::Succeeded(value) => {
                ank_base::execution_state::ExecutionStateEnum::Succeeded(value as i32)
            }
            ExecutionStateEnum::Failed(value) => {
                ank_base::execution_state::ExecutionStateEnum::Failed(value as i32)
            }
            ExecutionStateEnum::NotScheduled => {
                ank_base::execution_state::ExecutionStateEnum::NotScheduled(
                    ank_base::NotScheduled::NotScheduled as i32,
                )
            }
            ExecutionStateEnum::Removed => ank_base::execution_state::ExecutionStateEnum::Removed(
                ank_base::Removed::Removed as i32,
            ),
            ExecutionStateEnum::Stopping(value) => {
                ank_base::execution_state::ExecutionStateEnum::Stopping(value as i32)
            }
        }
    }
}

impl From<ank_base::execution_state::ExecutionStateEnum> for ExecutionStateEnum {
    fn from(item: ank_base::execution_state::ExecutionStateEnum) -> Self {
        match item {
            ank_base::execution_state::ExecutionStateEnum::AgentDisconnected(_) => {
                ExecutionStateEnum::AgentDisconnected
            }
            ank_base::execution_state::ExecutionStateEnum::Pending(value) => {
                ExecutionStateEnum::Pending(value.into())
            }
            ank_base::execution_state::ExecutionStateEnum::Running(value) => {
                ExecutionStateEnum::Running(value.into())
            }
            ank_base::execution_state::ExecutionStateEnum::Stopping(value) => {
                ExecutionStateEnum::Stopping(value.into())
            }
            ank_base::execution_state::ExecutionStateEnum::Succeeded(value) => {
                ExecutionStateEnum::Succeeded(value.into())
            }
            ank_base::execution_state::ExecutionStateEnum::Failed(value) => {
                ExecutionStateEnum::Failed(value.into())
            }
            ank_base::execution_state::ExecutionStateEnum::NotScheduled(_) => {
                ExecutionStateEnum::NotScheduled
            }
            ank_base::execution_state::ExecutionStateEnum::Removed(_) => {
                ExecutionStateEnum::Removed
            }
        }
    }
}

// [impl->swdd~common-workload-states-supported-states~1]
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ExecutionState {
    #[serde(flatten)]
    pub state: ExecutionStateEnum,
    // [impl->swdd~common-workload-state-additional-information~1]
    pub additional_info: String,
}

impl ExecutionState {
    pub fn is_removed(&self) -> bool {
        ExecutionStateEnum::Removed == self.state
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.state, ExecutionStateEnum::Pending(_))
    }

    pub fn is_pending_initial(&self) -> bool {
        ExecutionStateEnum::Pending(PendingSubstate::Initial) == self.state
    }

    pub fn is_running(&self) -> bool {
        ExecutionStateEnum::Running(RunningSubstate::Ok) == self.state
    }

    pub fn is_succeeded(&self) -> bool {
        ExecutionStateEnum::Succeeded(SucceededSubstate::Ok) == self.state
    }

    pub fn is_failed(&self) -> bool {
        ExecutionStateEnum::Failed(FailedSubstate::ExecFailed) == self.state
    }

    pub fn is_not_pending_nor_running(&self) -> bool {
        !self.is_pending() && !self.is_running()
    }

    pub fn is_waiting_to_start(&self) -> bool {
        ExecutionStateEnum::Pending(PendingSubstate::WaitingToStart) == self.state
    }

    pub fn is_waiting_to_stop(&self) -> bool {
        ExecutionStateEnum::Stopping(StoppingSubstate::WaitingToStop) == self.state
    }

    pub fn agent_disconnected() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::AgentDisconnected,
            ..Default::default()
        }
    }

    pub fn starting_failed(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::StartingFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn retry_starting(
        current_retry: usize,
        max_retries: usize,
        additional_info: impl ToString,
    ) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::Starting),
            additional_info: format!(
                "Retry {} of {}: {}",
                current_retry,
                max_retries,
                additional_info.to_string()
            ),
        }
    }

    pub fn retry_failed_no_retry(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::StartingFailed),
            additional_info: format!("{}: {}", NO_MORE_RETRIES_MSG, additional_info.to_string()),
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

    pub fn starting_triggered() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::Starting),
            additional_info: TRIGGERED_MSG.to_string(),
        }
    }

    pub fn failed(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Failed(FailedSubstate::ExecFailed),
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
            state: ExecutionStateEnum::Stopping(StoppingSubstate::Stopping),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn stopping_requested() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Stopping(StoppingSubstate::RequestedAtRuntime),
            ..Default::default()
        }
    }

    pub fn delete_failed(additional_info: impl ToString) -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Stopping(StoppingSubstate::DeleteFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn lost() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Failed(FailedSubstate::Lost),
            ..Default::default()
        }
    }

    pub fn waiting_to_start() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::WaitingToStart),
            ..Default::default()
        }
    }

    pub fn waiting_to_stop() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Stopping(StoppingSubstate::WaitingToStop),
            ..Default::default()
        }
    }

    pub fn initial() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::Pending(PendingSubstate::Initial),
            ..Default::default()
        }
    }

    pub fn not_scheduled() -> Self {
        ExecutionState {
            state: ExecutionStateEnum::NotScheduled,
            ..Default::default()
        }
    }
}

impl From<ExecutionState> for ank_base::ExecutionState {
    fn from(item: ExecutionState) -> Self {
        ank_base::ExecutionState {
            additional_info: item.additional_info,
            execution_state_enum: Some(item.state.into()),
        }
    }
}

impl From<ank_base::ExecutionState> for ExecutionState {
    fn from(item: ank_base::ExecutionState) -> Self {
        ExecutionState {
            additional_info: item.additional_info,
            state: item
                .execution_state_enum
                .unwrap_or(ank_base::execution_state::ExecutionStateEnum::Failed(
                    ank_base::Failed::Unknown as i32,
                ))
                .into(),
        }
    }
}

impl Display for ExecutionStateEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            ExecutionStateEnum::AgentDisconnected => write!(f, "AgentDisconnected"),
            ExecutionStateEnum::Pending(substate) => write!(f, "Pending({substate})"),
            ExecutionStateEnum::Running(substate) => write!(f, "Running({substate})"),
            ExecutionStateEnum::Stopping(substate) => write!(f, "Stopping({substate})"),
            ExecutionStateEnum::Succeeded(substate) => {
                write!(f, "Succeeded({substate})")
            }
            ExecutionStateEnum::Failed(substate) => write!(f, "Failed({substate})"),
            ExecutionStateEnum::NotScheduled => write!(f, "NotScheduled"),
            ExecutionStateEnum::Removed => write!(f, "Removed"),
        }
    }
}

impl Display for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.additional_info.is_empty() {
            write!(f, "{}: '{}'", self.state, self.additional_info)
        } else {
            write!(f, "{}", self.state)
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkloadState {
    // [impl->swdd~common-workload-state-identification~1]
    pub instance_name: WorkloadInstanceName,
    pub execution_state: ExecutionState,
}

impl From<WorkloadState> for ank_base::WorkloadState {
    fn from(item: WorkloadState) -> Self {
        ank_base::WorkloadState {
            instance_name: Some(item.instance_name.into()),
            execution_state: Some(item.execution_state.into()),
        }
    }
}

impl From<ank_base::WorkloadState> for WorkloadState {
    fn from(item: ank_base::WorkloadState) -> Self {
        WorkloadState {
            instance_name: item.instance_name.unwrap_or_unreachable().into(),
            execution_state: item
                .execution_state
                .unwrap_or(ank_base::ExecutionState {
                    additional_info: "Cannot covert, proceeding with unknown".to_owned(),
                    execution_state_enum: Some(
                        ank_base::execution_state::ExecutionStateEnum::Failed(
                            ank_base::Failed::Unknown as i32,
                        ),
                    ),
                })
                .into(),
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
        instance_name: WorkloadInstanceName::builder()
            .workload_name(workload_name)
            .agent_name(agent_name)
            .config(&"config".to_string())
            .build(),
        execution_state,
    }
}
#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_workload_spec(
    workload_spec: &super::WorkloadSpec,
    execution_state: ExecutionState,
) -> WorkloadState {
    WorkloadState {
        instance_name: workload_spec.instance_name.clone(),
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
    use api::ank_base::{self};

    use crate::objects::{
        workload_state::NO_MORE_RETRIES_MSG, ExecutionState, WorkloadInstanceName, WorkloadState,
    };

    // [utest->swdd~common-workload-state-transitions~1]
    #[test]
    fn utest_execution_state_transition_hysteresis() {
        assert_eq!(
            ExecutionState::waiting_to_stop().transition(ExecutionState::running()),
            ExecutionState::waiting_to_stop()
        );
        assert_eq!(
            ExecutionState::stopping_requested().transition(ExecutionState::running()),
            ExecutionState::stopping_requested()
        );
        assert_eq!(
            ExecutionState::stopping_requested().transition(ExecutionState::succeeded()),
            ExecutionState::stopping_requested()
        );
        assert_eq!(
            ExecutionState::stopping_requested()
                .transition(ExecutionState::failed("failed for some reason")),
            ExecutionState::stopping_requested()
        );
        assert_eq!(
            ExecutionState::stopping_requested().transition(ExecutionState::lost()),
            ExecutionState::stopping_requested()
        );
        assert_eq!(
            ExecutionState::stopping_requested()
                .transition(ExecutionState::unknown("I lost the thing")),
            ExecutionState::stopping_requested()
        );
        assert_eq!(
            ExecutionState::stopping_requested().transition(ExecutionState::delete_failed(
                "mi mi mi, I could not delete it..."
            )),
            ExecutionState::delete_failed("mi mi mi, I could not delete it...")
        );
        assert_eq!(
            ExecutionState::delete_failed("mi mi mi, I could not delete it...")
                .transition(ExecutionState::running()),
            ExecutionState::running()
        );
        assert_eq!(
            ExecutionState::running().transition(ExecutionState::failed("crashed")),
            ExecutionState::failed("crashed")
        );
    }

    // [utest->swdd~common-workload-state-identification~1]
    #[test]
    fn utest_converts_to_proto_workload_state() {
        let additional_info = "some additional info";
        let ankaios_wl_state = WorkloadState {
            execution_state: ExecutionState::starting(additional_info),
            instance_name: WorkloadInstanceName::builder()
                .workload_name("john")
                .agent_name("strange")
                .build(),
        };

        let proto_wl_state = ank_base::WorkloadState {
            execution_state: Some(ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Pending(
                    ank_base::Pending::Starting.into(),
                )),
            }),
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "john".to_string(),
                agent_name: "strange".to_string(),
                ..Default::default()
            }),
        };

        assert_eq!(
            ank_base::WorkloadState::from(ankaios_wl_state),
            proto_wl_state
        );
    }

    // [utest->swdd~common-workload-state-identification~1]
    #[test]
    fn utest_converts_to_ankaios_workload_state() {
        let ankaios_wl_state = WorkloadState {
            execution_state: ExecutionState::running(),
            instance_name: WorkloadInstanceName::builder()
                .workload_name("john")
                .agent_name("strange")
                .build(),
        };

        let proto_wl_state = ank_base::WorkloadState {
            execution_state: Some(ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Running(
                    ank_base::Running::Ok.into(),
                )),
            }),
            instance_name: Some(ank_base::WorkloadInstanceName {
                workload_name: "john".to_string(),
                agent_name: "strange".to_string(),
                ..Default::default()
            }),
        };

        assert_eq!(WorkloadState::from(proto_wl_state), ankaios_wl_state);
    }

    // [utest->swdd~common-workload-state-additional-information~1]
    // [utest->swdd~common-workload-states-supported-states~1]
    #[test]
    fn utest_execution_state_to_proto_mapping() {
        let additional_info = "some additional info";

        assert_eq!(
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(
                    ank_base::execution_state::ExecutionStateEnum::AgentDisconnected(
                        ank_base::AgentDisconnected::AgentDisconnected.into(),
                    )
                ),
            },
            ExecutionState::agent_disconnected().into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: format!("{}: {}", NO_MORE_RETRIES_MSG, additional_info),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Pending(
                    ank_base::Pending::StartingFailed.into(),
                )),
            },
            ExecutionState::retry_failed_no_retry(additional_info).into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Removed(
                    ank_base::Removed::Removed.into(),
                )),
            },
            ExecutionState::removed().into(),
        );

        assert_eq!(
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Failed(
                    ank_base::Failed::Unknown.into(),
                )),
            },
            ExecutionState::unknown(additional_info).into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Pending(
                    ank_base::Pending::Starting.into(),
                )),
            },
            ExecutionState::starting(additional_info).into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Failed(
                    ank_base::Failed::ExecFailed.into(),
                )),
            },
            ExecutionState::failed(additional_info).into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(
                    ank_base::execution_state::ExecutionStateEnum::Succeeded(
                        ank_base::Succeeded::Ok.into(),
                    )
                ),
            },
            ExecutionState::succeeded().into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Running(
                    ank_base::Running::Ok.into(),
                )),
            },
            ExecutionState::running().into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(
                    ank_base::execution_state::ExecutionStateEnum::Stopping(
                        ank_base::Stopping::Stopping.into(),
                    )
                ),
            },
            ExecutionState::stopping(additional_info).into(),
        );
        assert_eq!(
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Failed(
                    ank_base::Failed::Lost.into(),
                )),
            },
            ExecutionState::lost().into(),
        );
    }

    // [utest->swdd~common-workload-state-additional-information~1]
    // [utest->swdd~common-workload-states-supported-states~1]
    #[test]
    fn utest_execution_state_from_proto_mapping() {
        let additional_info = "some additional info";

        assert_eq!(
            ExecutionState::agent_disconnected(),
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(
                    ank_base::execution_state::ExecutionStateEnum::AgentDisconnected(
                        ank_base::AgentDisconnected::AgentDisconnected.into(),
                    )
                ),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::retry_failed_no_retry(additional_info),
            ank_base::ExecutionState {
                additional_info: format!("{}: {}", NO_MORE_RETRIES_MSG, additional_info),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Pending(
                    ank_base::Pending::StartingFailed.into(),
                )),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::removed(),
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Removed(
                    ank_base::Removed::Removed.into(),
                )),
            }
            .into(),
        );

        assert_eq!(
            ExecutionState::unknown(additional_info),
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Failed(
                    ank_base::Failed::Unknown.into(),
                )),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::starting(additional_info),
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Pending(
                    ank_base::Pending::Starting.into(),
                )),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::failed(additional_info),
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Failed(
                    ank_base::Failed::ExecFailed.into(),
                )),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::succeeded(),
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(
                    ank_base::execution_state::ExecutionStateEnum::Succeeded(
                        ank_base::Succeeded::Ok.into(),
                    )
                ),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::running(),
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Running(
                    ank_base::Running::Ok.into(),
                )),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::stopping(additional_info),
            ank_base::ExecutionState {
                additional_info: additional_info.to_string(),
                execution_state_enum: Some(
                    ank_base::execution_state::ExecutionStateEnum::Stopping(
                        ank_base::Stopping::Stopping.into(),
                    )
                ),
            }
            .into(),
        );
        assert_eq!(
            ExecutionState::lost(),
            ank_base::ExecutionState {
                additional_info: "".to_string(),
                execution_state_enum: Some(ank_base::execution_state::ExecutionStateEnum::Failed(
                    ank_base::Failed::Lost.into(),
                )),
            }
            .into(),
        );
    }

    // [utest->swdd~common-workload-state-additional-information~1]
    // [utest->swdd~common-workload-states-supported-states~1]
    #[test]
    fn utest_execution_state_to_string_basic_mapping() {
        let additional_info = "some additional info";

        assert_eq!(
            ExecutionState::agent_disconnected().to_string(),
            String::from("AgentDisconnected")
        );
        assert_eq!(
            ExecutionState::retry_starting(1, 2, additional_info).to_string(),
            format!("Pending(Starting): 'Retry 1 of 2: {additional_info}'")
        );
        assert_eq!(
            ExecutionState::retry_failed_no_retry(additional_info).to_string(),
            format!(
                "Pending(StartingFailed): '{}: {}'",
                NO_MORE_RETRIES_MSG, additional_info
            )
        );
        assert_eq!(
            ExecutionState::removed().to_string(),
            String::from("Removed")
        );
        assert_eq!(
            ExecutionState::unknown(additional_info).to_string(),
            format!("Failed(Unknown): '{additional_info}'")
        );
        assert_eq!(
            ExecutionState::starting(additional_info).to_string(),
            format!("Pending(Starting): '{additional_info}'")
        );
        assert_eq!(
            ExecutionState::failed(additional_info).to_string(),
            format!("Failed(ExecFailed): '{additional_info}'")
        );
        assert_eq!(
            ExecutionState::succeeded().to_string(),
            String::from("Succeeded(Ok)")
        );
        assert_eq!(
            ExecutionState::running().to_string(),
            String::from("Running(Ok)")
        );
        assert_eq!(
            ExecutionState::stopping(additional_info).to_string(),
            format!("Stopping(Stopping): '{additional_info}'")
        );
        assert_eq!(
            ExecutionState::lost().to_string(),
            String::from("Failed(Lost)")
        );
    }
}

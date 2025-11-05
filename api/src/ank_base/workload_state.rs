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
    AgentDisconnected, ExecutionStateEnumInternal, ExecutionStateInternal, Failed, NotScheduled,
    Pending, Removed, Running, Stopping, Succeeded, ExecutionState, ExecutionStateEnum,
};
use std::fmt::Display;

const TRIGGERED_MSG: &str = "Triggered at runtime.";
pub const NO_MORE_RETRIES_MSG: &str = "No more retries";

impl Default for ExecutionStateEnumInternal {
    fn default() -> Self {
        ExecutionStateEnumInternal::NotScheduled(NotScheduled::NotScheduled)
    }
}

impl Display for ExecutionStateEnumInternal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            ExecutionStateEnumInternal::AgentDisconnected(_) => write!(f, "AgentDisconnected"),
            ExecutionStateEnumInternal::Pending(substate) => write!(f, "Pending({substate:?})"),
            ExecutionStateEnumInternal::Running(substate) => write!(f, "Running({substate:?})"),
            ExecutionStateEnumInternal::Stopping(substate) => write!(f, "Stopping({substate:?})"),
            ExecutionStateEnumInternal::Succeeded(substate) => {
                write!(f, "Succeeded({substate:?})")
            }
            ExecutionStateEnumInternal::Failed(substate) => write!(f, "Failed({substate:?})"),
            ExecutionStateEnumInternal::NotScheduled(_) => write!(f, "NotScheduled"),
            ExecutionStateEnumInternal::Removed(_) => write!(f, "Removed"),
        }
    }
}

impl ExecutionStateInternal {
    pub fn state(&self) -> &ExecutionStateEnumInternal {
        &self.execution_state_enum
    }

    pub fn transition(&self, incoming: ExecutionStateInternal) -> ExecutionStateInternal {
        match (&self.state(), &incoming.state()) {
            (
                ExecutionStateEnumInternal::Stopping(Stopping::RequestedAtRuntime)
                | ExecutionStateEnumInternal::Stopping(Stopping::WaitingToStop),
                ExecutionStateEnumInternal::Running(Running::Ok)
                | ExecutionStateEnumInternal::Succeeded(Succeeded::Ok)
                | ExecutionStateEnumInternal::Failed(Failed::ExecFailed)
                | ExecutionStateEnumInternal::Failed(Failed::Lost)
                | ExecutionStateEnumInternal::Failed(Failed::Unknown),
            ) => {
                // log::trace!("Skipping transition from '{self}' to '{incoming}' state.");
                self.clone()
            }
            _ => incoming,
        }
    }
}

impl ExecutionStateInternal {
    pub fn is_removed(&self) -> bool {
        matches!(self.state(), ExecutionStateEnumInternal::Removed(_))
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.state(), ExecutionStateEnumInternal::Pending(_))
    }

    pub fn is_pending_initial(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumInternal::Pending(Pending::Initial)
        )
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumInternal::Running(Running::Ok)
        )
    }

    pub fn is_succeeded(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumInternal::Succeeded(Succeeded::Ok)
        )
    }

    pub fn is_failed(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumInternal::Failed(Failed::ExecFailed)
        )
    }

    pub fn is_not_pending_nor_running(&self) -> bool {
        !self.is_pending() && !self.is_running()
    }

    pub fn is_waiting_to_start(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumInternal::Pending(Pending::WaitingToStart)
        )
    }

    pub fn is_waiting_to_stop(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumInternal::Stopping(Stopping::WaitingToStop)
        )
    }

    pub fn agent_disconnected() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::AgentDisconnected(
                AgentDisconnected::AgentDisconnected,
            ),
            ..Default::default()
        }
    }

    pub fn starting_failed(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Pending(Pending::StartingFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn retry_starting(retry_count: u32, additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Pending(Pending::Starting),
            additional_info: format!("Retry {}: {}", retry_count, additional_info.to_string()),
        }
    }

    pub fn retry_failed_no_retry(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Pending(Pending::StartingFailed),
            additional_info: format!("{}: {}", NO_MORE_RETRIES_MSG, additional_info.to_string()),
        }
    }

    pub fn removed() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Removed(Removed::Removed),
            ..Default::default()
        }
    }

    pub fn unknown(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Failed(Failed::Unknown),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn starting(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Pending(Pending::Starting),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn starting_triggered() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Pending(Pending::Starting),
            additional_info: TRIGGERED_MSG.to_string(),
        }
    }

    pub fn failed(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Failed(Failed::ExecFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn succeeded() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Succeeded(Succeeded::Ok),
            ..Default::default()
        }
    }

    pub fn running() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Running(Running::Ok),
            ..Default::default()
        }
    }

    pub fn stopping(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Stopping(Stopping::Stopping),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn stopping_requested() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Stopping(
                Stopping::RequestedAtRuntime,
            ),
            ..Default::default()
        }
    }

    pub fn delete_failed(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Stopping(Stopping::DeleteFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn lost() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Failed(Failed::Lost),
            ..Default::default()
        }
    }

    pub fn waiting_to_start() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Pending(Pending::WaitingToStart),
            ..Default::default()
        }
    }

    pub fn waiting_to_stop() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Stopping(Stopping::WaitingToStop),
            ..Default::default()
        }
    }

    pub fn initial() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::Pending(Pending::Initial),
            ..Default::default()
        }
    }

    pub fn not_scheduled() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumInternal::NotScheduled(
                NotScheduled::NotScheduled,
            ),
            ..Default::default()
        }
    }
}

impl ExecutionState{
    pub fn agent_disconnected() -> Self {
        Self {
            execution_state_enum: Some(ExecutionStateEnum::AgentDisconnected(
                AgentDisconnected::AgentDisconnected as i32),
            ),
            ..Default::default()
        }
    }
}

impl Display for ExecutionStateInternal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.additional_info.is_empty() {
            write!(f, "{}: '{}'", self.state(), self.additional_info)
        } else {
            write!(f, "{}", self.state())
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
use crate::ank_base::{WorkloadInstanceNameInternal, WorkloadStateInternal, WorkloadNamed};
#[cfg(any(feature = "test_utils", test))]
use crate::test_utils::generate_test_runtime_config;

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_agent(
    workload_name: &str,
    agent_name: &str,
    execution_state: ExecutionStateInternal,
) -> WorkloadStateInternal {
    WorkloadStateInternal {
        instance_name: WorkloadInstanceNameInternal::builder()
            .workload_name(workload_name)
            .agent_name(agent_name)
            .config(&generate_test_runtime_config())
            .build(),
        execution_state,
    }
}
#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_workload_named(
    workload_named: &WorkloadNamed,
    execution_state: ExecutionStateInternal,
) -> WorkloadStateInternal {
    WorkloadStateInternal {
        instance_name: workload_named.instance_name.clone(),
        execution_state,
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state(
    workload_name: &str,
    execution_state: ExecutionStateInternal,
) -> WorkloadStateInternal {
    generate_test_workload_state_with_agent(workload_name, "agent_name", execution_state)
}

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
#[cfg(test)]
mod tests {
    use super::NO_MORE_RETRIES_MSG;
    use crate::ank_base::ExecutionStateInternal;

    // [utest->swdd~common-workload-state-transitions~1]
    #[test]
    fn utest_execution_state_transition_hysteresis() {
        assert_eq!(
            ExecutionStateInternal::waiting_to_stop().transition(ExecutionStateInternal::running()),
            ExecutionStateInternal::waiting_to_stop()
        );
        assert_eq!(
            ExecutionStateInternal::stopping_requested()
                .transition(ExecutionStateInternal::running()),
            ExecutionStateInternal::stopping_requested()
        );
        assert_eq!(
            ExecutionStateInternal::stopping_requested()
                .transition(ExecutionStateInternal::succeeded()),
            ExecutionStateInternal::stopping_requested()
        );
        assert_eq!(
            ExecutionStateInternal::stopping_requested()
                .transition(ExecutionStateInternal::failed("failed for some reason")),
            ExecutionStateInternal::stopping_requested()
        );
        assert_eq!(
            ExecutionStateInternal::stopping_requested().transition(ExecutionStateInternal::lost()),
            ExecutionStateInternal::stopping_requested()
        );
        assert_eq!(
            ExecutionStateInternal::stopping_requested()
                .transition(ExecutionStateInternal::unknown("I lost the thing")),
            ExecutionStateInternal::stopping_requested()
        );
        assert_eq!(
            ExecutionStateInternal::stopping_requested().transition(
                ExecutionStateInternal::delete_failed("mi mi mi, I could not delete it...")
            ),
            ExecutionStateInternal::delete_failed("mi mi mi, I could not delete it...")
        );
        assert_eq!(
            ExecutionStateInternal::delete_failed("mi mi mi, I could not delete it...")
                .transition(ExecutionStateInternal::running()),
            ExecutionStateInternal::running()
        );
        assert_eq!(
            ExecutionStateInternal::running().transition(ExecutionStateInternal::failed("crashed")),
            ExecutionStateInternal::failed("crashed")
        );
    }

    // [utest->swdd~common-workload-state-additional-information~1]
    // [utest->swdd~common-workload-states-supported-states~1]
    #[test]
    fn utest_execution_state_to_string_basic_mapping() {
        let additional_info = "some additional info";

        assert_eq!(
            ExecutionStateInternal::agent_disconnected().to_string(),
            String::from("AgentDisconnected")
        );
        assert_eq!(
            ExecutionStateInternal::retry_failed_no_retry(additional_info).to_string(),
            format!("Pending(StartingFailed): '{NO_MORE_RETRIES_MSG}: {additional_info}'")
        );
        assert_eq!(
            ExecutionStateInternal::removed().to_string(),
            String::from("Removed")
        );
        assert_eq!(
            ExecutionStateInternal::unknown(additional_info).to_string(),
            format!("Failed(Unknown): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateInternal::starting(additional_info).to_string(),
            format!("Pending(Starting): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateInternal::failed(additional_info).to_string(),
            format!("Failed(ExecFailed): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateInternal::succeeded().to_string(),
            String::from("Succeeded(Ok)")
        );
        assert_eq!(
            ExecutionStateInternal::running().to_string(),
            String::from("Running(Ok)")
        );
        assert_eq!(
            ExecutionStateInternal::stopping(additional_info).to_string(),
            format!("Stopping(Stopping): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateInternal::lost().to_string(),
            String::from("Failed(Lost)")
        );
    }
}

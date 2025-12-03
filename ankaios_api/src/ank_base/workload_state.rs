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
    AgentDisconnected, ExecutionStateEnumSpec, ExecutionStateSpec, Failed, NotScheduled, Pending,
    Removed, Running, Stopping, Succeeded,
};
use std::fmt::Display;

const TRIGGERED_MSG: &str = "Triggered at runtime.";
pub const NO_MORE_RETRIES_MSG: &str = "No more retries";

impl Default for ExecutionStateEnumSpec {
    fn default() -> Self {
        ExecutionStateEnumSpec::NotScheduled(NotScheduled::NotScheduled)
    }
}

impl Display for ExecutionStateEnumSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            ExecutionStateEnumSpec::AgentDisconnected(_) => write!(f, "AgentDisconnected"),
            ExecutionStateEnumSpec::Pending(substate) => write!(f, "Pending({substate:?})"),
            ExecutionStateEnumSpec::Running(substate) => write!(f, "Running({substate:?})"),
            ExecutionStateEnumSpec::Stopping(substate) => write!(f, "Stopping({substate:?})"),
            ExecutionStateEnumSpec::Succeeded(substate) => {
                write!(f, "Succeeded({substate:?})")
            }
            ExecutionStateEnumSpec::Failed(substate) => write!(f, "Failed({substate:?})"),
            ExecutionStateEnumSpec::NotScheduled(_) => write!(f, "NotScheduled"),
            ExecutionStateEnumSpec::Removed(_) => write!(f, "Removed"),
        }
    }
}

impl ExecutionStateSpec {
    pub fn state(&self) -> &ExecutionStateEnumSpec {
        &self.execution_state_enum
    }

    // [impl->swdd~api-workload-state-transitions~1]
    pub fn transition(&self, incoming: ExecutionStateSpec) -> ExecutionStateSpec {
        match (&self.state(), &incoming.state()) {
            (
                // Skip transitions from stopping states:
                ExecutionStateEnumSpec::Stopping(Stopping::RequestedAtRuntime)
                | ExecutionStateEnumSpec::Stopping(Stopping::WaitingToStop),
                // to these states:
                ExecutionStateEnumSpec::Running(Running::Ok)
                | ExecutionStateEnumSpec::Succeeded(Succeeded::Ok)
                | ExecutionStateEnumSpec::Failed(Failed::ExecFailed)
                | ExecutionStateEnumSpec::Failed(Failed::Lost)
                | ExecutionStateEnumSpec::Failed(Failed::Unknown),
            ) => {
                log::trace!("Skipping transition from '{self}' to '{incoming}' state.");
                self.clone()
            }
            _ => incoming,
        }
    }
}

impl ExecutionStateSpec {
    pub fn is_removed(&self) -> bool {
        matches!(self.state(), ExecutionStateEnumSpec::Removed(_))
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.state(), ExecutionStateEnumSpec::Pending(_))
    }

    pub fn is_pending_initial(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumSpec::Pending(Pending::Initial)
        )
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state(), ExecutionStateEnumSpec::Running(Running::Ok))
    }

    pub fn is_succeeded(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumSpec::Succeeded(Succeeded::Ok)
        )
    }

    pub fn is_failed(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumSpec::Failed(Failed::ExecFailed)
        )
    }

    pub fn is_not_pending_nor_running(&self) -> bool {
        !self.is_pending() && !self.is_running()
    }

    pub fn is_waiting_to_start(&self) -> bool {
        matches!(
            self.state(),
            ExecutionStateEnumSpec::Pending(Pending::WaitingToStart)
        )
    }

    pub fn agent_disconnected() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::AgentDisconnected(
                AgentDisconnected::AgentDisconnected,
            ),
            ..Default::default()
        }
    }

    pub fn starting_failed(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Pending(Pending::StartingFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn retry_starting(retry_count: u32, additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Pending(Pending::Starting),
            additional_info: format!("Retry {}: {}", retry_count, additional_info.to_string()),
        }
    }

    pub fn retry_failed_no_retry(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Pending(Pending::StartingFailed),
            additional_info: format!("{}: {}", NO_MORE_RETRIES_MSG, additional_info.to_string()),
        }
    }

    pub fn removed() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Removed(Removed::Removed),
            ..Default::default()
        }
    }

    pub fn unknown(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Failed(Failed::Unknown),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn starting(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Pending(Pending::Starting),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn starting_triggered() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Pending(Pending::Starting),
            additional_info: TRIGGERED_MSG.to_string(),
        }
    }

    pub fn failed(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Failed(Failed::ExecFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn succeeded() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Succeeded(Succeeded::Ok),
            ..Default::default()
        }
    }

    pub fn running() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Running(Running::Ok),
            ..Default::default()
        }
    }

    pub fn stopping(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Stopping(Stopping::Stopping),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn stopping_requested() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Stopping(Stopping::RequestedAtRuntime),
            ..Default::default()
        }
    }

    pub fn delete_failed(additional_info: impl ToString) -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Stopping(Stopping::DeleteFailed),
            additional_info: additional_info.to_string(),
        }
    }

    pub fn lost() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Failed(Failed::Lost),
            ..Default::default()
        }
    }

    pub fn waiting_to_start() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Pending(Pending::WaitingToStart),
            ..Default::default()
        }
    }

    pub fn waiting_to_stop() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Stopping(Stopping::WaitingToStop),
            ..Default::default()
        }
    }

    pub fn initial() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::Pending(Pending::Initial),
            ..Default::default()
        }
    }

    pub fn not_scheduled() -> Self {
        Self {
            execution_state_enum: ExecutionStateEnumSpec::NotScheduled(NotScheduled::NotScheduled),
            ..Default::default()
        }
    }
}

impl Display for ExecutionStateSpec {
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
use crate::ank_base::{WorkloadInstanceNameSpec, WorkloadNamed, WorkloadStateSpec};
#[cfg(any(feature = "test_utils", test))]
use crate::test_utils::generate_test_runtime_config;

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_agent(
    workload_name: &str,
    agent_name: &str,
    execution_state: ExecutionStateSpec,
) -> WorkloadStateSpec {
    WorkloadStateSpec {
        instance_name: WorkloadInstanceNameSpec::builder()
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
    execution_state: ExecutionStateSpec,
) -> WorkloadStateSpec {
    WorkloadStateSpec {
        instance_name: workload_named.instance_name.clone(),
        execution_state,
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state(
    workload_name: &str,
    execution_state: ExecutionStateSpec,
) -> WorkloadStateSpec {
    generate_test_workload_state_with_agent(workload_name, "agent_name", execution_state)
}

#[cfg(test)]
mod tests {
    use super::NO_MORE_RETRIES_MSG;
    use crate::ank_base::ExecutionStateSpec;

    // [utest->swdd~api-workload-state-transitions~1]
    #[test]
    fn utest_execution_state_transition_hysteresis() {
        assert_eq!(
            ExecutionStateSpec::waiting_to_stop().transition(ExecutionStateSpec::running()),
            ExecutionStateSpec::waiting_to_stop()
        );
        assert_eq!(
            ExecutionStateSpec::stopping_requested().transition(ExecutionStateSpec::running()),
            ExecutionStateSpec::stopping_requested()
        );
        assert_eq!(
            ExecutionStateSpec::stopping_requested().transition(ExecutionStateSpec::succeeded()),
            ExecutionStateSpec::stopping_requested()
        );
        assert_eq!(
            ExecutionStateSpec::stopping_requested()
                .transition(ExecutionStateSpec::failed("failed for some reason")),
            ExecutionStateSpec::stopping_requested()
        );
        assert_eq!(
            ExecutionStateSpec::stopping_requested().transition(ExecutionStateSpec::lost()),
            ExecutionStateSpec::stopping_requested()
        );
        assert_eq!(
            ExecutionStateSpec::stopping_requested()
                .transition(ExecutionStateSpec::unknown("I lost the thing")),
            ExecutionStateSpec::stopping_requested()
        );
        assert_eq!(
            ExecutionStateSpec::stopping_requested().transition(ExecutionStateSpec::delete_failed(
                "mi mi mi, I could not delete it..."
            )),
            ExecutionStateSpec::delete_failed("mi mi mi, I could not delete it...")
        );
        assert_eq!(
            ExecutionStateSpec::delete_failed("mi mi mi, I could not delete it...")
                .transition(ExecutionStateSpec::running()),
            ExecutionStateSpec::running()
        );
        assert_eq!(
            ExecutionStateSpec::running().transition(ExecutionStateSpec::failed("crashed")),
            ExecutionStateSpec::failed("crashed")
        );
    }

    // [utest->swdd~api-workload-state-additional-information~1]
    // [utest->swdd~api-workload-states-supported-states~1]
    #[test]
    fn utest_execution_state_to_string_basic_mapping() {
        let additional_info = "some additional info";

        assert_eq!(
            ExecutionStateSpec::agent_disconnected().to_string(),
            String::from("AgentDisconnected")
        );
        assert_eq!(
            ExecutionStateSpec::retry_failed_no_retry(additional_info).to_string(),
            format!("Pending(StartingFailed): '{NO_MORE_RETRIES_MSG}: {additional_info}'")
        );
        assert_eq!(
            ExecutionStateSpec::removed().to_string(),
            String::from("Removed")
        );
        assert_eq!(
            ExecutionStateSpec::not_scheduled().to_string(),
            String::from("NotScheduled")
        );
        assert_eq!(
            ExecutionStateSpec::unknown(additional_info).to_string(),
            format!("Failed(Unknown): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateSpec::starting(additional_info).to_string(),
            format!("Pending(Starting): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateSpec::failed(additional_info).to_string(),
            format!("Failed(ExecFailed): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateSpec::succeeded().to_string(),
            String::from("Succeeded(Ok)")
        );
        assert_eq!(
            ExecutionStateSpec::running().to_string(),
            String::from("Running(Ok)")
        );
        assert_eq!(
            ExecutionStateSpec::stopping(additional_info).to_string(),
            format!("Stopping(Stopping): '{additional_info}'")
        );
        assert_eq!(
            ExecutionStateSpec::lost().to_string(),
            String::from("Failed(Lost)")
        );
    }
}

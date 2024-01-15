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

use api::proto;

// [impl->swdd~common-supported-workload-states~1]
#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum ExecutionState {
    #[default]
    ExecUnknown = 0,
    ExecPending = 1, // this is the default initial state of Ankaios, it shall not be set or send by anybody else
    ExecWaitingToStart = 2,
    ExecStarting = 3,
    ExecRunning = 4,
    ExecSucceeded = 5,
    ExecFailed = 6,
    ExecWaitingToStop = 7,
    ExecStopping = 8,
    ExecStoppingFailed = 9,
    ExecRemoved = 10,
}

// [impl->swdd~common-workload-state-transitions~1]
impl ExecutionState {
    pub fn transition(self, incoming: ExecutionState) -> ExecutionState {
        match (self, incoming) {
            (ExecutionState::ExecStopping, ExecutionState::ExecRunning)
            | (ExecutionState::ExecStopping, ExecutionState::ExecSucceeded)
            | (ExecutionState::ExecStopping, ExecutionState::ExecFailed) => {
                ExecutionState::ExecStopping
            }
            (_, incoming) => incoming,
        }
    }
}

impl From<i32> for ExecutionState {
    fn from(x: i32) -> Self {
        match x {
            x if x == ExecutionState::ExecPending as i32 => ExecutionState::ExecPending,
            x if x == ExecutionState::ExecWaitingToStart as i32 => {
                ExecutionState::ExecWaitingToStart
            }
            x if x == ExecutionState::ExecStarting as i32 => ExecutionState::ExecStarting,
            x if x == ExecutionState::ExecRunning as i32 => ExecutionState::ExecRunning,
            x if x == ExecutionState::ExecSucceeded as i32 => ExecutionState::ExecSucceeded,
            x if x == ExecutionState::ExecFailed as i32 => ExecutionState::ExecFailed,
            x if x == ExecutionState::ExecWaitingToStop as i32 => ExecutionState::ExecWaitingToStop,
            x if x == ExecutionState::ExecStopping as i32 => ExecutionState::ExecStopping,
            x if x == ExecutionState::ExecStoppingFailed as i32 => {
                ExecutionState::ExecStoppingFailed
            }
            x if x == ExecutionState::ExecRemoved as i32 => ExecutionState::ExecRemoved,
            _ => ExecutionState::ExecUnknown,
        }
    }
}

impl Display for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionState::ExecPending => write!(f, "Pending"),
            ExecutionState::ExecWaitingToStart => write!(f, "WaitingToStart"),
            ExecutionState::ExecStarting => write!(f, "Starting"),
            ExecutionState::ExecRunning => write!(f, "Running"),
            ExecutionState::ExecSucceeded => write!(f, "Succeeded"),
            ExecutionState::ExecFailed => write!(f, "Failed"),
            ExecutionState::ExecWaitingToStop => write!(f, "WaitingToStop"),
            ExecutionState::ExecStopping => write!(f, "Stopping"),
            ExecutionState::ExecStoppingFailed => write!(f, "StoppingFailed"),
            ExecutionState::ExecRemoved => write!(f, "Removed"),
            ExecutionState::ExecUnknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkloadState {
    pub workload_name: String,
    pub agent_name: String,
    pub execution_state: ExecutionState,
}

impl From<WorkloadState> for proto::WorkloadState {
    fn from(item: WorkloadState) -> Self {
        proto::WorkloadState {
            agent_name: item.agent_name,
            workload_name: item.workload_name,
            execution_state: item.execution_state as i32,
        }
    }
}

impl From<proto::WorkloadState> for WorkloadState {
    fn from(item: proto::WorkloadState) -> Self {
        WorkloadState {
            agent_name: item.agent_name,
            workload_name: item.workload_name,
            execution_state: item.execution_state.into(),
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

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
#[cfg(test)]
mod tests {
    use api::proto;

    use crate::objects::{ExecutionState, WorkloadState};

    #[test]
    fn utest_converts_to_proto_workload_state() {
        let ankaios_wl_state = WorkloadState {
            workload_name: "john".to_string(),
            agent_name: "strange".to_string(),
            execution_state: ExecutionState::ExecRunning,
        };

        let proto_wl_state = proto::WorkloadState {
            workload_name: "john".to_string(),
            agent_name: "strange".to_string(),
            execution_state: proto::ExecutionState::ExecRunning.into(),
        };

        assert_eq!(proto::WorkloadState::from(ankaios_wl_state), proto_wl_state);
    }

    #[test]
    fn utest_converts_to_ankaios_workload_state() {
        let ankaios_wl_state = WorkloadState {
            workload_name: "john".to_string(),
            agent_name: "strange".to_string(),
            execution_state: ExecutionState::ExecRunning,
        };

        let proto_wl_state = proto::WorkloadState {
            workload_name: "john".to_string(),
            agent_name: "strange".to_string(),
            execution_state: proto::ExecutionState::ExecRunning.into(),
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
        assert_eq!(ExecutionState::ExecStoppingFailed, ExecutionState::from(9));
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
            ExecutionState::ExecStoppingFailed.to_string(),
            String::from("StoppingFailed")
        );
        assert_eq!(
            ExecutionState::ExecUnknown.to_string(),
            String::from("Unknown")
        );
    }

    // [utest->swdd~common-workload-state-transitions~1]
    #[test]
    fn utest_execution_state_transition_hysteresis() {
        assert_eq!(
            ExecutionState::ExecStopping.transition(ExecutionState::ExecRunning),
            ExecutionState::ExecStopping
        );
        assert_eq!(
            ExecutionState::ExecStopping.transition(ExecutionState::ExecSucceeded),
            ExecutionState::ExecStopping
        );
        assert_eq!(
            ExecutionState::ExecStopping.transition(ExecutionState::ExecFailed),
            ExecutionState::ExecStopping
        );
        assert_eq!(
            ExecutionState::ExecStopping.transition(ExecutionState::ExecStarting),
            ExecutionState::ExecStarting
        );
        assert_eq!(
            ExecutionState::ExecStarting.transition(ExecutionState::ExecRunning),
            ExecutionState::ExecRunning
        );
        assert_eq!(
            ExecutionState::ExecStoppingFailed.transition(ExecutionState::ExecRunning),
            ExecutionState::ExecRunning
        );
    }
}

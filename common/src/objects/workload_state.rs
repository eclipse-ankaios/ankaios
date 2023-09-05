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

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum ExecutionState {
    ExecPending = 0,
    ExecRunning = 1,
    ExecSucceeded = 2,
    ExecFailed = 3,
    #[default]
    ExecUnknown = 4,
    ExecRemoved = 5,
}

impl From<i32> for ExecutionState {
    fn from(x: i32) -> Self {
        match x {
            x if x == ExecutionState::ExecPending as i32 => ExecutionState::ExecPending,
            x if x == ExecutionState::ExecRunning as i32 => ExecutionState::ExecRunning,
            x if x == ExecutionState::ExecSucceeded as i32 => ExecutionState::ExecSucceeded,
            x if x == ExecutionState::ExecFailed as i32 => ExecutionState::ExecFailed,
            x if x == ExecutionState::ExecRemoved as i32 => ExecutionState::ExecRemoved,
            _ => ExecutionState::ExecUnknown,
        }
    }
}

impl std::str::FromStr for ExecutionState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "created" => Ok(ExecutionState::ExecPending),
            "pending" => Ok(ExecutionState::ExecPending),
            "running" => Ok(ExecutionState::ExecRunning),
            "succeeded" => Ok(ExecutionState::ExecSucceeded),
            "failed" => Ok(ExecutionState::ExecFailed),
            "removed" => Ok(ExecutionState::ExecRemoved),
            _ => Ok(ExecutionState::ExecUnknown),
        }
    }
}

impl Display for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionState::ExecPending => write!(f, "Pending"),
            ExecutionState::ExecRunning => write!(f, "Running"),
            ExecutionState::ExecSucceeded => write!(f, "Succeeded"),
            ExecutionState::ExecFailed => write!(f, "Failed"),
            ExecutionState::ExecUnknown => write!(f, "Unknown"),
            ExecutionState::ExecRemoved => write!(f, "Removed"),
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
    use std::str::FromStr;

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

    #[test]
    fn utest_execution_state_from_int_mapping() {
        assert_eq!(ExecutionState::ExecPending, ExecutionState::from(0));
        assert_eq!(ExecutionState::ExecRunning, ExecutionState::from(1));
        assert_eq!(ExecutionState::ExecSucceeded, ExecutionState::from(2));
        assert_eq!(ExecutionState::ExecFailed, ExecutionState::from(3));
        assert_eq!(ExecutionState::ExecUnknown, ExecutionState::from(4));
        assert_eq!(ExecutionState::ExecRemoved, ExecutionState::from(5));
        assert_eq!(ExecutionState::ExecUnknown, ExecutionState::from(100));
    }

    #[test]
    fn utest_execution_state_from_string_basic_mapping() {
        assert_eq!(
            ExecutionState::ExecPending,
            ExecutionState::from_str("Pending").unwrap()
        );
        assert_eq!(
            ExecutionState::ExecRunning,
            ExecutionState::from_str("Running").unwrap()
        );
        assert_eq!(
            ExecutionState::ExecSucceeded,
            ExecutionState::from_str("Succeeded").unwrap()
        );
        assert_eq!(
            ExecutionState::ExecFailed,
            ExecutionState::from_str("Failed").unwrap()
        );
        assert_eq!(
            ExecutionState::ExecRemoved,
            ExecutionState::from_str("Removed").unwrap()
        );
        assert_eq!(
            ExecutionState::ExecUnknown,
            ExecutionState::from_str("Unsupported").unwrap()
        );
    }

    #[test]
    fn utest_execution_state_to_string_basic_mapping() {
        assert_eq!(
            ExecutionState::ExecPending.to_string(),
            String::from("Pending")
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
            ExecutionState::ExecRemoved.to_string(),
            String::from("Removed")
        );
        assert_eq!(
            ExecutionState::ExecUnknown.to_string(),
            String::from("Unknown")
        );
    }
}

// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use api::ank_base::{self, AgentMapInternal, StateInternal, WorkloadStatesMapInternal};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompleteState {
    #[serde(default)]
    pub desired_state: StateInternal,
    #[serde(default)]
    pub workload_states: WorkloadStatesMapInternal,
    #[serde(default)]
    pub agents: AgentMapInternal,
}

// pub type CompleteState = ank_base::CompleteStateInternal;

impl From<CompleteState> for ank_base::CompleteState {
    fn from(item: CompleteState) -> ank_base::CompleteState {
        ank_base::CompleteState {
            desired_state: Some(ank_base::State::from(item.desired_state)),
            workload_states: Some(item.workload_states.into()),
            agents: Some(item.agents.into()),
        }
    }
}

impl TryFrom<ank_base::CompleteState> for CompleteState {
    type Error = String;

    fn try_from(item: ank_base::CompleteState) -> Result<Self, Self::Error> {
        Ok(CompleteState {
            desired_state: item.desired_state.unwrap_or_default().try_into()?,
            workload_states: item.workload_states.unwrap_or_default().try_into()?,
            agents: item.agents.unwrap_or_default().try_into()?,
        })
    }
}

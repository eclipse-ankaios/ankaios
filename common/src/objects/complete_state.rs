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

use api::ank_proto;
use serde::{Deserialize, Serialize};

use super::{State, WorkloadState};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompleteState {
    #[serde(default)]
    pub startup_state: State,
    #[serde(default)]
    pub desired_state: State,
    #[serde(default)]
    pub workload_states: Vec<WorkloadState>,
}

impl From<CompleteState> for ank_proto::CompleteState {
    fn from(item: CompleteState) -> ank_proto::CompleteState {
        ank_proto::CompleteState {
            startup_state: Some(ank_proto::State::from(item.startup_state)),
            desired_state: Some(ank_proto::State::from(item.desired_state)),
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl TryFrom<ank_proto::CompleteState> for CompleteState {
    type Error = String;

    fn try_from(item: ank_proto::CompleteState) -> Result<Self, Self::Error> {
        Ok(CompleteState {
            startup_state: item.startup_state.unwrap_or_default().try_into()?,
            desired_state: item.desired_state.unwrap_or_default().try_into()?,
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        })
    }
}

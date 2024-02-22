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

use api::proto;
use serde::{Deserialize, Serialize};

use super::{State, WorkloadState};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct CompleteState {
    pub startup_state: State,
    pub current_state: State,
    pub workload_states: Vec<WorkloadState>,
}

impl From<CompleteState> for proto::CompleteState {
    fn from(item: CompleteState) -> proto::CompleteState {
        proto::CompleteState {
            startup_state: Some(proto::State::from(item.startup_state)),
            current_state: Some(proto::State::from(item.current_state)),
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl TryFrom<proto::CompleteState> for CompleteState {
    type Error = String;

    fn try_from(item: proto::CompleteState) -> Result<Self, Self::Error> {
        Ok(CompleteState {
            startup_state: item.startup_state.unwrap_or_default().try_into()?,
            current_state: item.current_state.unwrap_or_default().try_into()?,
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        })
    }
}

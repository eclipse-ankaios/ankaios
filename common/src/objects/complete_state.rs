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

use std::fmt::Display;

use api::proto;
use serde::{Deserialize, Serialize};

use super::{State, WorkloadState};

const CURRENT_API_VERSION: &str = "v0.1";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ApiVersion {
    pub version: String,
}

impl Default for ApiVersion {
    fn default() -> Self {
        Self {
            version: CURRENT_API_VERSION.to_string(),
        }
    }
}

impl From<ApiVersion> for proto::ApiVersion {
    fn from(item: ApiVersion) -> proto::ApiVersion {
        proto::ApiVersion {
            version: item.version,
        }
    }
}

impl From<proto::ApiVersion> for ApiVersion {
    fn from(item: proto::ApiVersion) -> Self {
        ApiVersion {
            version: item.version,
        }
    }
}

impl Display for ApiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.version)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompleteState {
    pub format_version: ApiVersion,
    #[serde(default)]
    pub startup_state: State,
    #[serde(default)]
    pub desired_state: State,
    #[serde(default)]
    pub workload_states: Vec<WorkloadState>,
}

impl CompleteState {
    pub fn is_compatible_format(format_version: &ApiVersion) -> bool {
        format_version.version == CURRENT_API_VERSION
    }
}

impl From<CompleteState> for proto::CompleteState {
    fn from(item: CompleteState) -> proto::CompleteState {
        proto::CompleteState {
            format_version: Some(proto::ApiVersion::from(item.format_version)),
            startup_state: Some(proto::State::from(item.startup_state)),
            desired_state: Some(proto::State::from(item.desired_state)),
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl TryFrom<proto::CompleteState> for CompleteState {
    type Error = String;

    fn try_from(item: proto::CompleteState) -> Result<Self, Self::Error> {
        Ok(CompleteState {
            format_version: item
                .format_version
                .unwrap_or_else(|| proto::ApiVersion {
                    version: "".to_string(),
                })
                .into(),
            startup_state: item.startup_state.unwrap_or_default().try_into()?,
            desired_state: item.desired_state.unwrap_or_default().try_into()?,
            workload_states: item.workload_states.into_iter().map(|x| x.into()).collect(),
        })
    }
}

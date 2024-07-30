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

use std::collections::HashMap;

use api::ank_base;
use common::{
    helpers::serialize_to_ordered_map,
    objects::{AddCondition, ControlInterfaceAccess, RestartPolicy, Tag, WorkloadStatesMap},
};
use serde::{Deserialize, Serialize, Serializer};

use crate::output_and_error;

pub fn serialize_option_to_ordered_map<S, T: Serialize>(
    value: &Option<HashMap<String, T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(value) = value {
        serialize_to_ordered_map(value, serializer)
    } else {
        serializer.serialize_none()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredCompleteState {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub desired_state: Option<FilteredState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub workload_states: Option<WorkloadStatesMap>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredState {
    // [impl->swdd~cli-returns-api-version-with-desired-state~1]
    pub api_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, serialize_with = "serialize_option_to_ordered_map")]
    pub workloads: Option<HashMap<String, FilteredWorkloadSpec>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredWorkloadSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,
    #[serde(serialize_with = "serialize_option_to_ordered_map")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<HashMap<String, AddCondition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<RestartPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_interface_access: Option<ControlInterfaceAccess>,
}

impl From<ank_base::CompleteState> for FilteredCompleteState {
    fn from(value: ank_base::CompleteState) -> Self {
        FilteredCompleteState {
            desired_state: value.desired_state.map(Into::into),
            workload_states: value.workload_states.map(Into::into),
        }
    }
}

impl From<ank_base::State> for FilteredState {
    fn from(value: ank_base::State) -> Self {
        FilteredState {
            api_version: value.api_version,
            workloads: value.workloads.map(|x| {
                x.workloads
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect()
            }),
        }
    }
}

fn map_vec<T, F>(vec: Vec<T>) -> Vec<F>
where
    F: From<T>,
{
    vec.into_iter().map(Into::into).collect()
}

impl From<ank_base::Workload> for FilteredWorkloadSpec {
    fn from(value: ank_base::Workload) -> Self {
        FilteredWorkloadSpec {
            agent: value.agent,
            tags: value.tags.map(|x| map_vec(x.tags)),
            dependencies: value.dependencies.map(|x| {
                x.dependencies
                    .into_iter()
                    .map(|(k, v)| (k, AddCondition::try_from(v).unwrap_or_else(|error| {
                        output_and_error!("Could not convert AddCondition.\nError: '{error}'. Check the Ankaios component compatibility.")
                    })))
                    .collect()
            }),
            restart_policy: value.restart_policy.map(|x| {
                RestartPolicy::try_from(x).unwrap_or_else(|error| {
                    output_and_error!("Could not convert RestartPolicy.\nError: '{error}'. Check the Ankaios component compatibility.")
                })
            }),
            runtime: value.runtime,
            runtime_config: value.runtime_config,
            control_interface_access: value
                .control_interface_access
                .map(|x| x.try_into().unwrap_or_else(|error| {
                    output_and_error!("Could not convert the ControlInterfaceAccess.\nError: '{error}'. Check the Ankaios component compatibility.")
                })),
        }
    }
}

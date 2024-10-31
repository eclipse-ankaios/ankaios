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
    objects::{
        AddCondition, ConfigItem, ControlInterfaceAccess, RestartPolicy, Tag, WorkloadStatesMap,
    },
};
use serde::{Deserialize, Serialize, Serializer};

use crate::{output_and_error, output_warn};

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

#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredCompleteState {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub desired_state: Option<FilteredState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub workload_states: Option<WorkloadStatesMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, flatten)]
    pub agents: Option<FilteredAgentMap>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredState {
    // [impl->swdd~cli-returns-api-version-with-desired-state~1]
    pub api_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, serialize_with = "serialize_option_to_ordered_map")]
    pub workloads: Option<HashMap<String, FilteredWorkloadSpec>>,
    #[serde(serialize_with = "serialize_option_to_ordered_map")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<HashMap<String, ConfigItem>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredAgentMap {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, serialize_with = "serialize_option_to_ordered_map")]
    pub agents: Option<HashMap<String, FilteredAgentAttributes>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilteredCpuUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_usage: Option<u32>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilteredFreeMemory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_memory: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilteredAgentAttributes {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub cpu_usage: Option<FilteredCpuUsage>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub free_memory: Option<FilteredFreeMemory>,
}

impl FilteredAgentAttributes {
    pub fn get_cpu_usage_as_string(&mut self) -> String {
        if let Some(cpu_usage) = &self.cpu_usage {
            if let Some(cpu_usage_value) = cpu_usage.cpu_usage {
                format!("{}%", cpu_usage_value)
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        }
    }

    pub fn get_free_memory_as_string(&mut self) -> String {
        if let Some(free_memory) = &self.free_memory {
            if let Some(free_memory_value) = free_memory.free_memory {
                format!("{}B", free_memory_value)
            } else {
                "".to_string()
            }
        } else {
            "".to_string()
        }
    }
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
    #[serde(serialize_with = "serialize_option_to_ordered_map")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<HashMap<String, String>>,
}

impl From<ank_base::CompleteState> for FilteredCompleteState {
    fn from(value: ank_base::CompleteState) -> Self {
        FilteredCompleteState {
            desired_state: value.desired_state.map(Into::into),
            workload_states: value.workload_states.map(Into::into),
            agents: value.agents.map(Into::into),
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
            configs: value.configs.map(|x| {
                x.configs
                    .into_iter()
                    .filter_map(|(key, value)| -> Option<(String, ConfigItem)> {
                        match value.try_into() {
                            Ok(value) => Some((key, value)),
                            Err(err) => {
                                output_warn!("Config item could not be converted: {}", err);
                                None
                            }
                        }
                    })
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
            configs: value.configs.map(|x| x.configs)
        }
    }
}

impl From<ank_base::AgentMap> for FilteredAgentMap {
    fn from(value: ank_base::AgentMap) -> Self {
        FilteredAgentMap {
            agents: Some(
                value
                    .agents
                    .into_iter()
                    .map(|(agent_name, agent_attributes)| (agent_name, agent_attributes.into()))
                    .collect(),
            ),
        }
    }
}

impl From<ank_base::AgentAttributes> for FilteredAgentAttributes {
    fn from(value: ank_base::AgentAttributes) -> Self {
        FilteredAgentAttributes {
            cpu_usage: value.cpu_usage.map(Into::into),
            free_memory: value.free_memory.map(Into::into),
        }
    }
}

impl From<ank_base::CpuUsage> for FilteredCpuUsage {
    fn from(value: ank_base::CpuUsage) -> Self {
        FilteredCpuUsage {
            cpu_usage: Some(value.cpu_usage),
        }
    }
}

impl From<ank_base::FreeMemory> for FilteredFreeMemory {
    fn from(value: ank_base::FreeMemory) -> Self {
        FilteredFreeMemory {
            free_memory: Some(value.free_memory),
        }
    }
}

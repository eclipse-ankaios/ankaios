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

use api::ank_base;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::Entry, HashMap};

use crate::commands;

type AgentName = String;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct CpuUsage {
    pub cpu_usage: u32,
}

impl CpuUsage {
    pub fn new(cpu_usage: f32) -> Self {
        Self {
            cpu_usage: cpu_usage.round() as u32,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct FreeMemory {
    pub free_memory: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentAttributes {
    pub cpu_usage: Option<CpuUsage>,
    pub free_memory: Option<FreeMemory>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentMap(HashMap<AgentName, AgentAttributes>);

// [impl->swdd~agent-map-manages-agent-names-with-agent-attributes~2]
impl AgentMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn entry(&mut self, key: String) -> Entry<'_, String, AgentAttributes> {
        self.0.entry(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }

    pub fn update_resource_availability(&mut self, agent_load_status: commands::AgentLoadStatus) {
        self.0.entry(agent_load_status.agent_name).and_modify(|e| {
            e.cpu_usage = Some(agent_load_status.cpu_usage);
            e.free_memory = Some(agent_load_status.free_memory);
        });
    }
}

impl From<CpuUsage> for ank_base::CpuUsage {
    fn from(item: CpuUsage) -> ank_base::CpuUsage {
        ank_base::CpuUsage {
            cpu_usage: item.cpu_usage,
        }
    }
}

impl From<ank_base::CpuUsage> for CpuUsage {
    fn from(item: ank_base::CpuUsage) -> Self {
        CpuUsage {
            cpu_usage: item.cpu_usage,
        }
    }
}

impl From<FreeMemory> for ank_base::FreeMemory {
    fn from(item: FreeMemory) -> ank_base::FreeMemory {
        ank_base::FreeMemory {
            free_memory: item.free_memory,
        }
    }
}

impl From<ank_base::FreeMemory> for FreeMemory {
    fn from(item: ank_base::FreeMemory) -> Self {
        FreeMemory {
            free_memory: item.free_memory,
        }
    }
}

impl From<AgentAttributes> for ank_base::AgentAttributes {
    fn from(item: AgentAttributes) -> ank_base::AgentAttributes {
        ank_base::AgentAttributes {
            cpu_usage: Some(ank_base::CpuUsage {
                cpu_usage: item.cpu_usage.unwrap_or_default().cpu_usage,
            }),
            free_memory: Some(ank_base::FreeMemory {
                free_memory: item.free_memory.unwrap_or_default().free_memory,
            }),
        }
    }
}

impl From<ank_base::AgentAttributes> for AgentAttributes {
    fn from(item: ank_base::AgentAttributes) -> Self {
        AgentAttributes {
            cpu_usage: Some(CpuUsage {
                cpu_usage: item.cpu_usage.unwrap_or_default().cpu_usage,
            }),
            free_memory: Some(FreeMemory {
                free_memory: item.free_memory.unwrap_or_default().free_memory,
            }),
        }
    }
}

impl From<AgentMap> for Option<ank_base::AgentMap> {
    fn from(item: AgentMap) -> Option<ank_base::AgentMap> {
        if item.0.is_empty() {
            return None;
        }

        Some(ank_base::AgentMap {
            agents: item
                .0
                .into_iter()
                .map(|(agent_name, agent_attributes)| (agent_name, agent_attributes.into()))
                .collect(),
        })
    }
}

impl From<ank_base::AgentMap> for AgentMap {
    fn from(item: ank_base::AgentMap) -> Self {
        AgentMap(
            item.agents
                .into_iter()
                .map(|(agent_name, agent_attributes)| (agent_name, agent_attributes.into()))
                .collect(),
        )
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
pub fn generate_test_agent_map(agent_name: impl Into<String>) -> AgentMap {
    let mut agent_map = AgentMap::new();
    agent_map
        .entry(agent_name.into())
        .or_insert(AgentAttributes {
            cpu_usage: Some(CpuUsage { cpu_usage: 42 }),
            free_memory: Some(FreeMemory { free_memory: 42 }),
        });
    agent_map
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_agent_map_from_specs(workloads: &[crate::objects::WorkloadSpec]) -> AgentMap {
    workloads
        .iter()
        .fold(AgentMap::new(), |mut agent_map, spec| {
            let agent_name = spec.instance_name.agent_name();
            agent_map
                .entry(agent_name.to_owned())
                .or_insert(AgentAttributes {
                    cpu_usage: Some(CpuUsage { cpu_usage: 42 }),
                    free_memory: Some(FreeMemory { free_memory: 42 }),
                });
            agent_map
        })
}

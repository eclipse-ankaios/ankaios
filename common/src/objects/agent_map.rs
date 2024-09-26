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
pub struct AgentResources {
    pub cpu_usage: u32,
    pub free_memory: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentAttributes {
    pub agent_resources: AgentResources,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentMap(HashMap<AgentName, AgentAttributes>);

// [impl->swdd~agent-map-manages-agent-names-with-agent-attributes~1]
impl AgentMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn entry(&mut self, key: String) -> Entry<'_, String, AgentAttributes> {
        self.0.entry(key)
    }

    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }

    pub fn update_resource_availability(&mut self, agent_resource: commands::AgentResourceCommand) {
        self.0.entry(agent_resource.agent_name).and_modify(|e| {
            e.agent_resources = agent_resource.agent_resources;
        });
    }
}

impl From<AgentAttributes> for ank_base::AgentAttributes {
    fn from(item: AgentAttributes) -> ank_base::AgentAttributes {
        ank_base::AgentAttributes {
            agent_resources: Some(ank_base::AgentResources {
                cpu_usage: item.agent_resources.cpu_usage,
                free_memory: item.agent_resources.free_memory,
            }),
        }
    }
}

impl From<ank_base::AgentAttributes> for AgentAttributes {
    fn from(item: ank_base::AgentAttributes) -> Self {
        AgentAttributes {
            agent_resources: AgentResources {
                cpu_usage: item.agent_resources.clone().unwrap().cpu_usage,
                free_memory: item.agent_resources.unwrap().free_memory,
            },
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
                .map(|(agent_name, agent_attributes)| {
                    (
                        agent_name,
                        ank_base::AgentAttributes {
                            agent_resources: Some(ank_base::AgentResources {
                                cpu_usage: agent_attributes.agent_resources.cpu_usage,
                                free_memory: agent_attributes.agent_resources.free_memory,
                            }),
                        },
                    )
                })
                .collect(),
        })
    }
}

impl From<ank_base::AgentMap> for AgentMap {
    fn from(item: ank_base::AgentMap) -> Self {
        AgentMap(
            item.agents
                .into_keys()
                .map(|agent_name| {
                    (
                        agent_name,
                        AgentAttributes {
                            ..Default::default()
                        },
                    )
                })
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
    agent_map.entry(agent_name.into()).or_default();
    agent_map
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_agent_map_from_specs(workloads: &[crate::objects::WorkloadSpec]) -> AgentMap {
    workloads
        .iter()
        .fold(AgentMap::new(), |mut agent_map, spec| {
            let agent_name = spec.instance_name.agent_name();
            agent_map.entry(agent_name.to_owned()).or_default();
            agent_map
        })
}

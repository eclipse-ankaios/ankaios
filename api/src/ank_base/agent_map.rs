// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use crate::ank_base::{
    AgentAttributesInternal, AgentMapInternal, CpuUsageInternal, FreeMemoryInternal,
};
use std::collections::{HashMap, hash_map::Entry};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentLoadStatus {
    pub agent_name: String,
    pub cpu_usage: CpuUsageInternal,
    pub free_memory: FreeMemoryInternal,
}

impl CpuUsageInternal {
    pub fn new(cpu_usage: f32) -> Self {
        Self {
            cpu_usage: cpu_usage.round() as u32,
        }
    }
}

// [impl->swdd~agent-map-manages-agent-names-with-agent-attributes~2]
impl AgentMapInternal {
    pub fn new() -> AgentMapInternal {
        AgentMapInternal {
            agents: HashMap::new(),
        }
    }

    pub fn entry(&mut self, key: String) -> Entry<'_, String, AgentAttributesInternal> {
        self.agents.entry(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.agents.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) {
        self.agents.remove(key);
    }

    pub fn update_resource_availability(&mut self, agent_load_status: AgentLoadStatus) {
        self.agents
            .entry(agent_load_status.agent_name)
            .and_modify(|e| {
                e.cpu_usage = Some(agent_load_status.cpu_usage);
                e.free_memory = Some(agent_load_status.free_memory);
            });
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
pub fn generate_test_agent_map(agent_name: impl Into<String>) -> AgentMapInternal {
    let mut agent_map = AgentMapInternal::new();
    agent_map
        .entry(agent_name.into())
        .or_insert(AgentAttributesInternal {
            cpu_usage: Some(CpuUsageInternal { cpu_usage: 42 }),
            free_memory: Some(FreeMemoryInternal { free_memory: 42 }),
        });
    agent_map
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_agent_map_from_specs(
    workloads: &[crate::ank_base::WorkloadInternal],
) -> AgentMapInternal {
    workloads
        .iter()
        .fold(AgentMapInternal::new(), |mut agent_map, spec| {
            let agent_name = &spec.agent;
            agent_map
                .entry(agent_name.to_owned())
                .or_insert(AgentAttributesInternal {
                    cpu_usage: Some(CpuUsageInternal { cpu_usage: 42 }),
                    free_memory: Some(FreeMemoryInternal { free_memory: 42 }),
                });
            agent_map
        })
}

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
    AgentAttributes, AgentAttributesInternal, AgentMapInternal, AgentStatus, CpuUsageInternal,
};
use std::collections::{HashMap, hash_map::Entry};

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
}

impl AgentAttributes {
    pub fn get_cpu_usage_as_string(&mut self) -> String {
        if let Some(AgentStatus {
            cpu_usage: Some(cpu_usage),
            ..
        }) = &self.status
        {
            format!("{}%", cpu_usage.cpu_usage)
        } else {
            "".to_string()
        }
    }

    pub fn get_free_memory_as_string(&mut self) -> String {
        if let Some(AgentStatus {
            free_memory: Some(free_memory),
            ..
        }) = &self.status
        {
            format!("{}B", free_memory.free_memory)
        } else {
            "".to_string()
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

#[cfg(any(feature = "test_utils", test))]
use crate::ank_base::{AgentStatusInternal, FreeMemoryInternal};

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_agent_map(agent_name: impl Into<String>) -> AgentMapInternal {
    let mut agent_map = AgentMapInternal::new();
    agent_map
        .entry(agent_name.into())
        .or_insert(AgentAttributesInternal {
            status: Some(AgentStatusInternal {
                cpu_usage: Some(CpuUsageInternal { cpu_usage: 42 }),
                free_memory: Some(FreeMemoryInternal { free_memory: 42 }),
            }),
            ..Default::default()
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
            use crate::ank_base::AgentStatusInternal;

            let agent_name = &spec.agent;
            agent_map
                .entry(agent_name.to_owned())
                .or_insert(AgentAttributesInternal {
                    status: Some(AgentStatusInternal {
                        cpu_usage: Some(CpuUsageInternal { cpu_usage: 42 }),
                        free_memory: Some(FreeMemoryInternal { free_memory: 42 }),
                    }),
                    ..Default::default()
                });
            agent_map
        })
}

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

type AgentName = String;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentAttributes {} // used for future extension

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentMap(HashMap<AgentName, AgentAttributes>);

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
}

impl From<AgentMap> for Option<ank_base::AgentMap> {
    fn from(item: AgentMap) -> Option<ank_base::AgentMap> {
        if item.0.is_empty() {
            return None;
        }

        Some(ank_base::AgentMap {
            agents: item
                .0
                .into_keys()
                .map(|agent_name| (agent_name, ank_base::AgentAttributes {}))
                .collect(),
        })
    }
}

impl From<ank_base::AgentMap> for AgentMap {
    fn from(item: ank_base::AgentMap) -> Self {
        AgentMap(
            item.agents
                .into_keys()
                .map(|agent_name| (agent_name, AgentAttributes {}))
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
        .entry(AgentName::from(agent_name.into()))
        .or_default();
    agent_map
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_agent_map_from_specs(workloads: &[crate::objects::WorkloadSpec]) -> AgentMap {
    workloads
        .iter()
        .fold(AgentMap::new(), |mut agent_map, spec| {
            let agent_name = spec.instance_name.agent_name();
            agent_map.entry(AgentName::from(agent_name)).or_default();
            agent_map
        })
}

#[cfg(test)]
mod tests {
    use super::AgentMap;

    const AGENT_A: &str = "agent_A";
    const AGENT_B: &str = "agent_B";

    #[test]
    fn utest_add_agent() {
        let mut agent_map = AgentMap::new();
        agent_map.entry(AGENT_A.to_string()).or_default();
        agent_map.entry(AGENT_B.to_string()).or_default();
        assert!(agent_map.0.contains_key(AGENT_A));
        assert!(agent_map.0.contains_key(AGENT_B));
    }

    #[test]
    fn utest_remove_agent() {
        let mut agent_map = AgentMap::new();
        agent_map.0.entry(AGENT_A.to_string()).or_default();
        agent_map.0.entry(AGENT_B.to_string()).or_default();

        agent_map.remove(AGENT_A);
        assert!(!agent_map.0.contains_key(AGENT_A));
    }
}

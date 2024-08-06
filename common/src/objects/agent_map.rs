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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::AgentName;
use api::ank_base;

pub type AgentAttributes = HashMap<String, String>;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentMap(HashMap<AgentName, AgentAttributes>);

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
                        agent_name.get().to_owned(),
                        ank_base::AgentAttributes { agent_attributes },
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
                .into_iter()
                .map(|(agent_name, agent_attributes)| {
                    (
                        AgentName::from(agent_name),
                        agent_attributes.agent_attributes,
                    )
                })
                .collect(),
        )
    }
}

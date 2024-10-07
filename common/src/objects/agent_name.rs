// Copyright (c) 2023 Elektrobit Automotive GmbH
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

use crate::objects::workload_instance_name::INSTANCE_NAME_SEPARATOR;

// [impl->swdd~common-object-representation~1]

#[derive(Debug, Eq, PartialEq)]
pub struct AgentName(String);

impl AgentName {
    pub fn get(&self) -> &str {
        &self.0
    }

    pub fn get_filter_regex(&self) -> String {
        format!("[{}]{}$", INSTANCE_NAME_SEPARATOR, self.0)
    }

    pub fn get_filter_suffix(&self) -> String {
        format!("{}{}", INSTANCE_NAME_SEPARATOR, self.0)
    }
}

impl From<String> for AgentName {
    fn from(value: String) -> Self {
        AgentName(value)
    }
}

impl From<&str> for AgentName {
    fn from(value: &str) -> Self {
        AgentName::from(value.to_string())
    }
}

impl Display for AgentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

// [utest->swdd~common-object-representation~1]
#[cfg(test)]
mod tests {
    use super::AgentName;

    const AGENT_NAME: &str = "agent";

    #[test]
    fn utest_agent_name_get_filter_regex() {
        assert_eq!(
            format!("[.]{AGENT_NAME}$"),
            AgentName::from(AGENT_NAME).get_filter_regex()
        );
    }

    #[test]
    fn utest_agent_name_get_filter_suffix() {
        assert_eq!(
            format!(".{AGENT_NAME}"),
            AgentName::from(AGENT_NAME).get_filter_suffix()
        );
    }
}

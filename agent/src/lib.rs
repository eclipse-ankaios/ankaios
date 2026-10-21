// Copyright (c) 2026 Elektrobit Automotive GmbH
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

mod agent_builder;
mod agent_config;
mod agent_manager;
mod cli;
mod control_interface;
pub mod runtime_connectors;
mod runtime_registry;
#[cfg(test)]
pub mod test_helper;
mod workload_operation;

mod generic_polling_state_checker;
mod resource_monitor;
mod runtime_manager;
mod subscription_store;
mod workload;
mod workload_files;
mod workload_log_facade;
mod workload_scheduler;
mod workload_state;

mod io_utils;

pub use agent_builder::AgentBuilder;
pub use agent_config::AgentConfig;

use ankaios_api::ALLOWED_CHAR_SET;
use common::std_extensions::IllegalStateResult;
use regex::Regex;

// [impl->swdd~agent-naming-convention~1]
pub fn validate_agent_name(agent_name: &str) -> Result<(), String> {
    const EXPECTED_AGENT_NAME_FORMAT: &str = "It shall contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols '-' and '_'.";
    if agent_name.is_empty() {
        return Err(format!(
            "Empty agent name is not allowed. {EXPECTED_AGENT_NAME_FORMAT}"
        ));
    }

    let re = Regex::new(&format!(r"^{ALLOWED_CHAR_SET}+$")).unwrap_or_illegal_state();

    if re.is_match(agent_name) {
        Ok(())
    } else {
        Err(format!(
            "Agent name '{agent_name}' is invalid. {EXPECTED_AGENT_NAME_FORMAT}",
        ))
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::validate_agent_name;

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_ok() {
        let name = "test_AgEnt-name1_56";
        assert!(validate_agent_name(name).is_ok());
    }

    // [utest->swdd~agent-naming-convention~1]
    #[test]
    fn utest_validate_agent_name_fail() {
        let invalid_agent_names = ["a.b", "a_b_%#", "a b"];
        for name in invalid_agent_names {
            let result = validate_agent_name(name);
            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .contains(&format!("Agent name '{name}' is invalid.",))
            );
        }

        let result = validate_agent_name("");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Empty agent name is not allowed.")
        );
    }
}

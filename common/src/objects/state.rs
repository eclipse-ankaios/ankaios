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

use regex::Regex;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::helpers::serialize_to_ordered_map;
use crate::objects::StoredWorkloadSpec;

use api::ank_base;

const CURRENT_API_VERSION: &str = "v0.1";
const MAX_CHARACTERS_WORKLOAD_NAME: usize = 63;

// [impl->swdd~common-object-representation~1]
// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub api_version: String,
    #[serde(default, serialize_with = "serialize_to_ordered_map")]
    pub workloads: HashMap<String, StoredWorkloadSpec>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            api_version: CURRENT_API_VERSION.into(),
            workloads: Default::default(),
        }
    }
}

impl From<State> for ank_base::State {
    fn from(item: State) -> Self {
        ank_base::State {
            api_version: item.api_version,
            workloads: Some(ank_base::WorkloadMap {
                workloads: item
                    .workloads
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect(),
            }),
        }
    }
}

impl TryFrom<ank_base::State> for State {
    type Error = String;

    fn try_from(item: ank_base::State) -> Result<Self, Self::Error> {
        Ok(State {
            api_version: item.api_version,
            workloads: item
                .workloads
                .ok_or("Missing workloads map")?
                .workloads
                .into_iter()
                .map(|(k, v)| Ok((k.to_owned(), v.try_into()?)))
                .collect::<Result<HashMap<String, StoredWorkloadSpec>, String>>()?,
        })
    }
}

impl State {
    pub fn verify_format(provided_state: &State) -> Result<(), String> {
        if provided_state.api_version != CURRENT_API_VERSION {
            return Err(format!(
                "Unsupported API version. Received '{}', expected '{}'",
                provided_state.api_version,
                State::default().api_version
            ));
        }

        let re = Regex::new(r"^[a-zA-Z0-9_-]+[a-zA-Z0-9_-]*$").unwrap();

        for (workload_name, workload_spec) in &provided_state.workloads {
            if !re.is_match(workload_name.as_str()) {
                return Err(format!(
                    "Unsupported workload name. Received '{}', expected to have characters in ^[a-zA-Z0-9_-]+[a-zA-Z0-9_-]*$",
                    workload_name
                ));
            }
            if workload_name.len() > MAX_CHARACTERS_WORKLOAD_NAME {
                return Err(format!(
                    "Workload name length {} exceeds the maximum limit of {} characters",
                    workload_name.len(),
                    MAX_CHARACTERS_WORKLOAD_NAME
                ));
            }
            if !re.is_match(workload_spec.agent.as_str()) {
                return Err(format!(
                    "Unsupported agent name. Received '{}', expected to have characters in ^[a-zA-Z0-9_-]+[a-zA-Z0-9_-]*$",
                    workload_spec.agent
                ));
            }
        }

        Ok(())
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {

    use super::{CURRENT_API_VERSION, MAX_CHARACTERS_WORKLOAD_NAME};
    use api::ank_base;
    use std::collections::HashMap;

    use crate::{
        objects::{State, StoredWorkloadSpec},
        test_utils::{generate_test_proto_state, generate_test_state},
    };

    #[test]
    fn utest_converts_to_proto_state() {
        let ankaios_state = generate_test_state();
        let proto_state = generate_test_proto_state();

        assert_eq!(ank_base::State::from(ankaios_state), proto_state);
    }

    #[test]
    fn utest_converts_to_ankaios_state() {
        let ankaios_state = generate_test_state();
        let proto_state = generate_test_proto_state();

        assert_eq!(State::try_from(proto_state), Ok(ankaios_state));
    }

    #[test]
    fn utest_serialize_state_into_ordered_output() {
        // input: random sorted state
        let ankaios_state = generate_test_state();

        // serialize to sorted output
        let sorted_state_string = serde_yaml::to_string(&ankaios_state).unwrap();

        let index_workload1 = sorted_state_string.find("workload_name_1").unwrap();
        let index_workload2 = sorted_state_string.find("workload_name_2").unwrap();
        assert!(
            index_workload1 < index_workload2,
            "expected sorted workloads."
        );
    }

    #[test]
    fn utest_state_accepts_compatible_state() {
        let state_compatible_version = State {
            ..Default::default()
        };
        assert_eq!(State::verify_format(&state_compatible_version), Ok(()));
    }

    #[test]
    fn utest_state_rejects_incompatible_state_on_api_version() {
        let api_version = "incompatible_version".to_string();
        let state_incompatible_version = State {
            api_version: api_version.clone(),
            ..Default::default()
        };
        assert_eq!(
            State::verify_format(&state_incompatible_version),
            Err(format!(
                "Unsupported API version. Received '{}', expected '{}'",
                api_version, CURRENT_API_VERSION
            ))
        );
    }

    #[test]
    fn utest_state_rejects_incompatible_state_on_workload_name() {
        let workload_name = "nginx.test".to_string();
        let state_incompatible_version = State {
            api_version: "v0.1".to_string(),
            workloads: HashMap::from([(workload_name.clone(), StoredWorkloadSpec::default())]),
        };
        assert_eq!(State::verify_format(&state_incompatible_version), Err(format!("Unsupported workload name. Received '{}', expected to have characters in [a-zA-Z0-9_-]", "nginx.test")));
    }

    #[test]
    fn utest_state_rejects_incompatible_state_on_inordinately_long_workload_name() {
        let workload_name = "workload_name_is_too_long_for_ankaios_to_accept_it_and_I_don_t_know_what_else_to_write".to_string();
        let state_incompatible_version = State {
            api_version: "v0.1".to_string(),
            workloads: HashMap::from([(workload_name.clone(), StoredWorkloadSpec::default())]),
        };
        assert_eq!(
            State::verify_format(&state_incompatible_version),
            Err(format!(
                "Workload name length {} exceeds the maximum limit of {} characters",
                workload_name.len(),
                MAX_CHARACTERS_WORKLOAD_NAME
            ))
        );
    }

    #[test]
    fn utest_state_rejects_incompatible_state_on_agent_name() {
        let agent_name = "agent_A.test".to_string();
        let state_incompatible_version = State {
            api_version: "v0.1".to_string(),
            workloads: HashMap::from([(
                "sample".to_string(),
                StoredWorkloadSpec {
                    agent: agent_name.clone(),
                    ..Default::default()
                },
            )]),
        };
        assert_eq!(
            State::verify_format(&state_incompatible_version),
            Err(format!(
                "Unsupported agent name. Received '{}', expected to have characters in [a-zA-Z0-9_-]",
                agent_name
            ))
        );
    }

    #[test]
    fn utest_state_rejects_state_without_api_version() {
        let state_proto_no_version = ank_base::State {
            api_version: "".into(),
            workloads: Some(ank_base::WorkloadMap {
                workloads: HashMap::new(),
            }),
        };
        let state_ankaios_no_version = State::try_from(state_proto_no_version).unwrap();

        assert_eq!(state_ankaios_no_version.api_version, "".to_string());

        let file_without_api_version = "";
        let deserialization_result = serde_yaml::from_str::<State>(file_without_api_version)
            .unwrap_err()
            .to_string();
        assert_eq!(deserialization_result, "missing field `apiVersion`");
    }
}

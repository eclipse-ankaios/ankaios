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

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::helpers::serialize_to_ordered_map;
use crate::objects::StoredWorkloadSpec;

use api::ank_base;

const CURRENT_API_VERSION: &str = "v0.1";

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
    pub fn is_compatible_format(api_version: &String) -> bool {
        api_version == CURRENT_API_VERSION
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

    use std::collections::HashMap;

    use api::ank_base;

    use crate::{
        objects::State,
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
        assert!(State::is_compatible_format(
            &state_compatible_version.api_version
        ));
    }

    #[test]
    fn utest_state_rejects_incompatible_state() {
        let state_incompatible_version = State {
            api_version: "incompatible_version".to_string(),
            ..Default::default()
        };
        assert!(!State::is_compatible_format(
            &state_incompatible_version.api_version
        ));
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

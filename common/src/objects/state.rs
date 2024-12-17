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

use regex::Regex;
use std::collections::HashMap;

use crate::helpers::serialize_to_ordered_map;
use crate::objects::ConfigItem;
use crate::objects::{StoredWorkloadSpec, STR_RE_CONFIG_REFERENCES};

use api::ank_base;

pub const CURRENT_API_VERSION: &str = "v0.1";

// [impl->swdd~common-object-representation~1]
// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub api_version: String,
    #[serde(default, serialize_with = "serialize_to_ordered_map")]
    pub workloads: HashMap<String, StoredWorkloadSpec>,
    #[serde(default)]
    pub configs: HashMap<String, ConfigItem>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            api_version: CURRENT_API_VERSION.into(),
            workloads: Default::default(),
            configs: Default::default(),
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
            configs: Some(ank_base::ConfigMap {
                configs: item
                    .configs
                    .into_iter()
                    .map(|(key, config_item)| (key, config_item.into()))
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
                .unwrap_or_default()
                .workloads
                .into_iter()
                .map(|(k, v)| Ok((k.to_owned(), v.try_into()?)))
                .collect::<Result<HashMap<String, StoredWorkloadSpec>, String>>()?,
            configs: item
                .configs
                .unwrap_or_default()
                .configs
                .into_iter()
                .map(|(k, v)| Ok((k, v.try_into()?)))
                .collect::<Result<_, Self::Error>>()?,
        })
    }
}

impl State {
    pub fn verify_api_version(provided_state: &State) -> Result<(), String> {
        if provided_state.api_version != CURRENT_API_VERSION {
            Err(format!(
                "Unsupported API version. Received '{}', expected '{}'",
                provided_state.api_version,
                State::default().api_version
            ))
        } else {
            Ok(())
        }
    }

    // [impl->swdd~common-config-item-key-naming-convention~1]
    pub fn verify_configs_format(provided_state: &State) -> Result<(), String> {
        let re_config_items = Regex::new(STR_RE_CONFIG_REFERENCES).unwrap();
        for config_key in provided_state.configs.keys() {
            if !re_config_items.is_match(config_key.as_str()) {
                return Err(format!(
                    "Unsupported config item key. Received '{}', expected to have characters in {}",
                    config_key, STR_RE_CONFIG_REFERENCES
                ));
            }
        }

        for workload in provided_state.workloads.values() {
            // [impl->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
            StoredWorkloadSpec::verify_config_reference_format(&workload.configs)?;
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
    use api::ank_base;
    use std::collections::HashMap;

    use crate::{
        objects::{generate_test_configs, generate_test_stored_workload_spec, ConfigItem, State},
        test_utils::{generate_test_proto_state, generate_test_state},
    };

    const WORKLOAD_NAME_1: &str = "workload_1";
    const AGENT_A: &str = "agent_A";
    const RUNTIME: &str = "runtime";
    const INVALID_CONFIG_KEY: &str = "invalid%key";

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
        let state_compatible_version = State::default();
        assert_eq!(State::verify_api_version(&state_compatible_version), Ok(()));
    }

    #[test]
    fn utest_state_rejects_incompatible_state_on_api_version() {
        let api_version = "incompatible_version".to_string();
        let state_incompatible_version = State {
            api_version: api_version.clone(),
            ..Default::default()
        };
        assert_eq!(
            State::verify_api_version(&state_incompatible_version),
            Err(format!(
                "Unsupported API version. Received '{}', expected '{}'",
                api_version,
                super::CURRENT_API_VERSION
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
            configs: Some(ank_base::ConfigMap {
                configs: HashMap::new(),
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

    // [utest->swdd~common-config-item-key-naming-convention~1]
    #[test]
    fn utest_verify_configs_format_compatible_config_item_keys_and_config_references() {
        let workload = generate_test_stored_workload_spec(AGENT_A, RUNTIME);
        let state = State {
            api_version: super::CURRENT_API_VERSION.into(),
            workloads: HashMap::from([(WORKLOAD_NAME_1.to_string(), workload)]),
            configs: generate_test_configs(),
        };

        assert_eq!(State::verify_configs_format(&state), Ok(()));
    }

    // [utest->swdd~common-config-item-key-naming-convention~1]
    #[test]
    fn utest_verify_configs_format_incompatible_config_item_key() {
        let state = State {
            api_version: super::CURRENT_API_VERSION.into(),
            configs: HashMap::from([(
                INVALID_CONFIG_KEY.to_owned(),
                ConfigItem::String("value".to_string()),
            )]),
            ..Default::default()
        };

        assert_eq!(
            State::verify_configs_format(&state),
            Err(format!(
                "Unsupported config item key. Received '{}', expected to have characters in {}",
                INVALID_CONFIG_KEY,
                super::STR_RE_CONFIG_REFERENCES
            ))
        );
    }

    // [utest->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
    #[test]
    fn utest_verify_configs_format_incompatible_workload_config_alias() {
        let mut workload = generate_test_stored_workload_spec(AGENT_A, RUNTIME);
        workload
            .configs
            .insert(INVALID_CONFIG_KEY.to_owned(), "config_1".to_string());

        let state = State {
            api_version: super::CURRENT_API_VERSION.into(),
            workloads: HashMap::from([(WORKLOAD_NAME_1.to_string(), workload)]),
            ..Default::default()
        };

        assert_eq!(
            State::verify_configs_format(&state),
            Err(format!(
                "Unsupported config alias. Received '{}', expected to have characters in {}",
                INVALID_CONFIG_KEY,
                super::STR_RE_CONFIG_REFERENCES
            ))
        );
    }
}

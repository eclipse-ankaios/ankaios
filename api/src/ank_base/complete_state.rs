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

use crate::ank_base::{STR_RE_CONFIG_REFERENCES, StateSpec, WorkloadSpec};
use crate::{CURRENT_API_VERSION, PREVIOUS_API_VERSION};
use regex::Regex;

impl Default for StateSpec {
    fn default() -> Self {
        Self {
            api_version: CURRENT_API_VERSION.into(),
            workloads: Default::default(),
            configs: Default::default(),
        }
    }
}

impl StateSpec {
    pub fn verify_api_version(provided_state: &StateSpec) -> Result<(), String> {
        match provided_state.api_version.as_str() {
            CURRENT_API_VERSION => Ok(()),
            PREVIOUS_API_VERSION => {
                log::warn!(
                    "The provided state uses an old API version '{PREVIOUS_API_VERSION}'. \
                     Please consider updating to the latest version '{CURRENT_API_VERSION}'."
                );
                Ok(())
            }
            version => Err(format!(
                "Unsupported API version. Received '{version}', expected '{CURRENT_API_VERSION}'"
            )),
        }
    }

    // [impl->swdd~common-config-item-key-naming-convention~1]
    pub fn verify_configs_format(provided_state: &StateSpec) -> Result<(), String> {
        let re_config_items = Regex::new(STR_RE_CONFIG_REFERENCES).unwrap();
        for config_key in provided_state.configs.configs.keys() {
            if !re_config_items.is_match(config_key.as_str()) {
                return Err(format!(
                    "Unsupported config item key. Received '{config_key}', expected to have characters in {STR_RE_CONFIG_REFERENCES}"
                ));
            }
        }

        for workload in provided_state.workloads.workloads.values() {
            // [impl->swdd~common-config-aliases-and-config-reference-keys-naming-convention~1]
            WorkloadSpec::verify_config_reference_format(&workload.configs.configs)?;
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
    use crate::ank_base::{
        ConfigMap, ConfigMapSpec, State, StateSpec, WorkloadMap, WorkloadMapSpec, WorkloadSpec,
    };
    use crate::test_utils::{
        generate_test_config_item, generate_test_configs, generate_test_state,
        generate_test_workload,
    };
    use std::collections::HashMap;

    const WORKLOAD_NAME_1: &str = "workload_1";
    const INVALID_CONFIG_KEY: &str = "invalid%key";

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
        let state_compatible_version = StateSpec::default();
        assert_eq!(
            StateSpec::verify_api_version(&state_compatible_version),
            Ok(())
        );
    }

    #[test]
    fn utest_state_rejects_incompatible_state_on_api_version() {
        let api_version = "incompatible_version".to_string();
        let state_incompatible_version = StateSpec {
            api_version: api_version.clone(),
            ..Default::default()
        };
        assert_eq!(
            StateSpec::verify_api_version(&state_incompatible_version),
            Err(format!(
                "Unsupported API version. Received '{}', expected '{}'",
                api_version,
                super::CURRENT_API_VERSION
            ))
        );
    }

    #[test]
    fn utest_state_rejects_state_without_api_version() {
        let state_proto_no_version = State {
            api_version: "".into(),
            workloads: Some(WorkloadMap {
                workloads: HashMap::new(),
            }),
            configs: Some(ConfigMap {
                configs: HashMap::new(),
            }),
        };
        let state_ankaios_no_version = StateSpec::try_from(state_proto_no_version).unwrap();

        assert_eq!(state_ankaios_no_version.api_version, "".to_string());

        let file_without_api_version = "";
        let deserialization_result = serde_yaml::from_str::<StateSpec>(file_without_api_version)
            .unwrap_err()
            .to_string();
        assert_eq!(deserialization_result, "missing field `apiVersion`");
    }

    // [utest->swdd~common-config-item-key-naming-convention~1]
    #[test]
    fn utest_verify_configs_format_compatible_config_item_keys_and_config_references() {
        let workload = generate_test_workload();
        let state = StateSpec {
            api_version: super::CURRENT_API_VERSION.into(),
            workloads: WorkloadMapSpec {
                workloads: HashMap::from([(WORKLOAD_NAME_1.to_string(), workload)]),
            },
            configs: generate_test_configs(),
        };

        assert_eq!(StateSpec::verify_configs_format(&state), Ok(()));
    }

    // [utest->swdd~common-config-item-key-naming-convention~1]
    #[test]
    fn utest_verify_configs_format_incompatible_config_item_key() {
        let state = StateSpec {
            api_version: super::CURRENT_API_VERSION.into(),
            configs: ConfigMapSpec {
                configs: HashMap::from([(
                    INVALID_CONFIG_KEY.to_owned(),
                    generate_test_config_item("value".to_owned()),
                )]),
            },
            ..Default::default()
        };

        assert_eq!(
            StateSpec::verify_configs_format(&state),
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
        let mut workload: WorkloadSpec = generate_test_workload();
        workload
            .configs
            .configs
            .insert(INVALID_CONFIG_KEY.to_owned(), "config_1".to_string());

        let state = StateSpec {
            api_version: super::CURRENT_API_VERSION.into(),
            workloads: WorkloadMapSpec {
                workloads: HashMap::from([(WORKLOAD_NAME_1.to_string(), workload)]),
            },
            ..Default::default()
        };

        assert_eq!(
            StateSpec::verify_configs_format(&state),
            Err(format!(
                "Unsupported config alias. Received '{}', expected to have characters in {}",
                INVALID_CONFIG_KEY,
                super::STR_RE_CONFIG_REFERENCES
            ))
        );
    }
}

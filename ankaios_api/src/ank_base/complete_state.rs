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

use std::collections::HashMap;

use crate::ank_base::{
    CompleteStateResponse, CompleteStateSpec, ConfigItemEnumSpec, ConfigItemSpec, StateSpec,
};
use crate::{
    ALLOWED_CHAR_SET, CONSTRAINT_FIELD_DESCRIPTION, CURRENT_API_VERSION, MAX_FIELD_LENGTH,
    PREVIOUS_API_VERSION,
};
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
    pub fn validate_pre_rendering(&self) -> Result<(), String> {
        // Before rendering we can only validate static fields
        Self::validate_api_version(self)?;
        Self::validate_configs_format(self)?;
        Ok(())
    }

    // [impl->swdd~api-version-checks~1]
    fn validate_api_version(&self) -> Result<(), String> {
        match self.api_version.as_str() {
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

    // [impl->swdd~api-config-item-key-naming-convention~1]
    fn validate_config_map(
        regex: &Regex,
        config_map: &HashMap<String, ConfigItemSpec>,
    ) -> Result<(), String> {
        for (key, value) in config_map {
            if key.len() > MAX_FIELD_LENGTH || !regex.is_match(key) {
                return Err(format!(
                    "Unsupported config item key '{key}'. {CONSTRAINT_FIELD_DESCRIPTION}"
                ));
            }

            match &value.config_item_enum {
                ConfigItemEnumSpec::Array(arr) => {
                    for item in &arr.values {
                        if let ConfigItemEnumSpec::Object(nested) = &item.config_item_enum {
                            Self::validate_config_map(regex, &nested.fields)?;
                        }
                    }
                }
                ConfigItemEnumSpec::Object(nested) => {
                    Self::validate_config_map(regex, &nested.fields)?
                }
                ConfigItemEnumSpec::String(_) => {}
            }
        }
        Ok(())
    }

    // [impl->swdd~common-config-item-key-naming-convention~1]
    fn validate_configs_format(&self) -> Result<(), String> {
        let re_config_items = Regex::new(&format!(r"^{ALLOWED_CHAR_SET}+$"))
            .map_err(|_| "Internal error. Invalid regular expression.")?;
        Self::validate_config_map(&re_config_items, &self.configs.configs)?;

        // [impl->swdd~api-config-aliases-and-config-reference-keys-naming-convention~1]
        self.workloads
            .workloads
            .values()
            .try_for_each(|workload_spec| workload_spec.validate_config_reference_format())?;

        Ok(())
    }
}

impl From<CompleteStateSpec> for CompleteStateResponse {
    fn from(item: CompleteStateSpec) -> Self {
        Self {
            complete_state: Some(item.into()),
            ..Default::default()
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
#[cfg(test)]
mod tests {
    use super::{CONSTRAINT_FIELD_DESCRIPTION, PREVIOUS_API_VERSION};
    use crate::ank_base::{
        ConfigMap, ConfigMapSpec, State, StateSpec, WorkloadMap, WorkloadMapSpec, WorkloadSpec,
    };
    use crate::test_utils::{
        fixtures, generate_test_config_item, generate_test_config_map, generate_test_state,
        generate_test_workload,
    };
    use std::collections::HashMap;

    const INVALID_CONFIG_KEY: &str = "invalid%key";

    // [utest->swdd~api-object-serialization~1]
    #[test]
    fn utest_serialize_state_into_ordered_output() {
        // input: random sorted state
        let ankaios_state = generate_test_state();

        // serialize to sorted output
        let sorted_state_string = serde_yaml::to_string(&ankaios_state).unwrap();

        let index_workload1 = sorted_state_string
            .find(fixtures::WORKLOAD_NAMES[0])
            .unwrap();
        let index_workload2 = sorted_state_string
            .find(fixtures::WORKLOAD_NAMES[1])
            .unwrap();
        assert!(
            index_workload1 < index_workload2,
            "expected sorted workloads."
        );
    }

    // [utest->swdd~api-version-checks~1]
    #[test]
    fn utest_state_accepts_compatible_state() {
        let mut state_compatible_version = StateSpec::default();
        assert_eq!(
            StateSpec::validate_api_version(&state_compatible_version),
            Ok(())
        );

        state_compatible_version.api_version = PREVIOUS_API_VERSION.to_string();
        assert_eq!(
            StateSpec::validate_api_version(&state_compatible_version),
            Ok(())
        );
    }

    // [utest->swdd~api-version-checks~1]
    #[test]
    fn utest_state_rejects_incompatible_state_on_api_version() {
        let api_version = "incompatible_version".to_string();
        let state_incompatible_version = StateSpec {
            api_version: api_version.clone(),
            ..Default::default()
        };
        assert_eq!(
            StateSpec::validate_api_version(&state_incompatible_version),
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

    // [utest->swdd~api-config-item-key-naming-convention~1]
    #[test]
    fn utest_validate_configs_format_compatible_config_item_keys_and_config_references() {
        let workload = generate_test_workload();
        let state = StateSpec {
            api_version: super::CURRENT_API_VERSION.into(),
            workloads: WorkloadMapSpec {
                workloads: HashMap::from([(fixtures::WORKLOAD_NAMES[0].to_string(), workload)]),
            },
            configs: generate_test_config_map(),
        };

        assert_eq!(StateSpec::validate_configs_format(&state), Ok(()));
    }

    // [utest->swdd~api-config-item-key-naming-convention~1]
    #[test]
    fn utest_validate_configs_format_incompatible_config_item_key() {
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
            StateSpec::validate_configs_format(&state),
            Err(format!(
                "Unsupported config item key '{INVALID_CONFIG_KEY}'. {CONSTRAINT_FIELD_DESCRIPTION}",
            ))
        );
    }

    // [utest->swdd~api-config-aliases-and-config-reference-keys-naming-convention~1]
    #[test]
    fn utest_validate_configs_format_incompatible_workload_config_alias() {
        let mut workload: WorkloadSpec = generate_test_workload();
        workload
            .configs
            .configs
            .insert(INVALID_CONFIG_KEY.to_owned(), "config_1".to_string());

        let state = StateSpec {
            api_version: super::CURRENT_API_VERSION.into(),
            workloads: WorkloadMapSpec {
                workloads: HashMap::from([(fixtures::WORKLOAD_NAMES[0].to_string(), workload)]),
            },
            ..Default::default()
        };

        assert_eq!(
            StateSpec::validate_configs_format(&state),
            Err(format!(
                "Unsupported config alias '{INVALID_CONFIG_KEY}'. {CONSTRAINT_FIELD_DESCRIPTION}",
            ))
        );
    }
}

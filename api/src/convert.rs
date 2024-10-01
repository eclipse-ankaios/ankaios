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

use std::collections::HashMap;

use crate::ank_base::{config_item, ConfigArray, ConfigItem, ConfigObject};

impl TryFrom<serde_yaml::Value> for ConfigItem {
    type Error = String;

    fn try_from(value: serde_yaml::Value) -> Result<Self, Self::Error> {
        match value {
            serde_yaml::Value::Null => Ok(Self { config_item: None }),
            serde_yaml::Value::Bool(_) => Err("Bool not supported".into()),
            serde_yaml::Value::Number(_) => Err("Number not supported".into()),
            serde_yaml::Value::String(string) => Ok(Self {
                config_item: Some(config_item::ConfigItem::String(string)),
            }),
            serde_yaml::Value::Sequence(array) => Ok(Self {
                config_item: Some(config_item::ConfigItem::Array(ConfigArray {
                    values: array
                        .into_iter()
                        .map(TryInto::try_into)
                        .collect::<Result<Vec<ConfigItem>, Self::Error>>()?,
                })),
            }),
            serde_yaml::Value::Mapping(object) => Ok(Self {
                config_item: Some(config_item::ConfigItem::Object(ConfigObject {
                    fields: object
                        .into_iter()
                        .map(|(key, value)| {
                            if let serde_yaml::Value::String(key) = key {
                                Ok((key, value.try_into()?))
                            } else {
                                Err("Key is not a string".into())
                            }
                        })
                        .collect::<Result<HashMap<String, ConfigItem>, Self::Error>>()?,
                })),
            }),

            serde_yaml::Value::Tagged(_) => Err("Tagged not supported".into()),
        }
    }
}

impl From<ConfigItem> for serde_yaml::Value {
    fn from(value: ConfigItem) -> Self {
        match value.config_item {
            None => serde_yaml::Value::Null,
            Some(config_item::ConfigItem::String(string)) => serde_yaml::Value::String(string),
            Some(config_item::ConfigItem::Array(ConfigArray { values })) => {
                serde_yaml::Value::Sequence(values.into_iter().map(Into::into).collect())
            }
            Some(config_item::ConfigItem::Object(ConfigObject { fields })) => {
                serde_yaml::Value::Mapping(
                    fields
                        .into_iter()
                        .map(|(key, value)| {
                            let key = serde_yaml::Value::String(key);
                            let value = serde_yaml::Value::from(value);
                            (key, value)
                        })
                        .collect(),
                )
            }
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
    use serde_yaml::Value;

    use crate::ank_base::{config_item, ConfigArray, ConfigItem, ConfigObject};

    const YAML_CONFIG_EXAMPLE: &str = r#"
- string_value
- key_1: object_value_1
  key_2:
    key_2_1: object_value_2_1
    key_2_2: object_value_2_2
  key_3:
  - array_value_1
  - array_value_2
  - array_value_3
"#;

    fn config_example() -> ConfigItem {
        array([
            string("string_value"),
            object([
                ("key_1", string("object_value_1")),
                (
                    "key_2",
                    object([
                        ("key_2_1", string("object_value_2_1")),
                        ("key_2_2", string("object_value_2_2")),
                    ]),
                ),
                (
                    "key_3",
                    array([
                        string("array_value_1"),
                        string("array_value_2"),
                        string("array_value_3"),
                    ]),
                ),
            ]),
        ])
    }

    fn string(str: &str) -> ConfigItem {
        ConfigItem {
            config_item: Some(config_item::ConfigItem::String(str.to_string())),
        }
    }

    fn array<const N: usize>(array: [ConfigItem; N]) -> ConfigItem {
        ConfigItem {
            config_item: Some(config_item::ConfigItem::Array(ConfigArray {
                values: array.to_vec(),
            })),
        }
    }

    fn object<const N: usize>(object: [(&str, ConfigItem); N]) -> ConfigItem {
        ConfigItem {
            config_item: Some(config_item::ConfigItem::Object(ConfigObject {
                fields: object
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value))
                    .collect(),
            })),
        }
    }

    #[test]
    fn utest_convert_string_from_yaml() {
        let parsed_config: ConfigItem = serde_yaml::from_str("string").unwrap();
        let expected_config = string("string");
        assert_eq!(parsed_config, expected_config);
    }

    #[test]
    fn utest_convert_string_to_yaml() {
        let serialized_config = serde_yaml::to_string(&string("string")).unwrap();
        let expected_yaml = "string\n";
        assert_eq!(serialized_config, expected_yaml);
    }

    #[test]
    fn utest_convert_none_from_yaml() {
        let parsed_config: ConfigItem = serde_yaml::from_str("null").unwrap();
        let expected_config = ConfigItem { config_item: None };
        assert_eq!(parsed_config, expected_config);
    }

    #[test]
    fn utest_convert_none_to_yaml() {
        let serialized_config = serde_yaml::to_string(&ConfigItem { config_item: None }).unwrap();
        let expected_yaml = "null\n";
        assert_eq!(serialized_config, expected_yaml);
    }

    #[test]
    fn utest_convert_object_from_yaml() {
        let parsed_config: ConfigItem = serde_yaml::from_str(YAML_CONFIG_EXAMPLE).unwrap();
        let expected_config = config_example();
        assert_eq!(parsed_config, expected_config);
    }

    #[test]
    fn utest_convert_object_to_yaml() {
        let serialized_config = serde_yaml::to_value(config_example()).unwrap();
        let expected_yaml: Value = serde_yaml::from_str(YAML_CONFIG_EXAMPLE).unwrap();
        assert_eq!(serialized_config, expected_yaml);
    }

    #[test]
    fn utest_convert_from_number_fails() {
        let parse_config_error = serde_yaml::from_str::<ConfigItem>("52").unwrap_err();
        assert_eq!(parse_config_error.to_string(), "Number not supported");
    }
    #[test]
    fn utest_convert_from_bool_fails() {
        let parse_config_error = serde_yaml::from_str::<ConfigItem>("true").unwrap_err();
        assert_eq!(parse_config_error.to_string(), "Bool not supported");
    }

    #[test]
    fn utest_convert_with_non_string_key_fails() {
        let parse_config_error = serde_yaml::from_str::<ConfigItem>("1: 2").unwrap_err();
        assert_eq!(parse_config_error.to_string(), "Key is not a string");
    }

    #[test]
    fn utest_convert_with_tags_fails() {
        let parse_config_error = serde_yaml::from_str::<ConfigItem>("!tag").unwrap_err();
        assert_eq!(parse_config_error.to_string(), "Tagged not supported");
    }

    #[test]
    fn utest_convert_with_not_convertible_object_value_fails() {
        let parsed_config = serde_yaml::from_str::<ConfigItem>("key: 32");
        assert!(parsed_config.is_err());
    }

    #[test]
    fn utest_convert_with_not_convertible_array_value_fails() {
        let parsed_config = serde_yaml::from_str::<ConfigItem>("- 32");
        assert!(parsed_config.is_err());
    }
}

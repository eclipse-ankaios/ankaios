// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use crate::ank_base::{
    ConfigArrayInternal, ConfigItemEnumInternal, ConfigItemInternal, ConfigMapInternal,
    ConfigMappingsInternal, ConfigObjectInternal,
};
use std::collections::HashMap;

impl<const N: usize> From<[(String, String); N]> for ConfigMappingsInternal {
    fn from(value: [(String, String); N]) -> Self {
        ConfigMappingsInternal {
            configs: value
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>(),
        }
    }
}

impl From<HashMap<String, String>> for ConfigMappingsInternal {
    fn from(value: HashMap<String, String>) -> Self {
        ConfigMappingsInternal { configs: value }
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
pub fn generate_test_configs() -> ConfigMapInternal {
    ConfigMapInternal {
        configs: HashMap::from([
            (
                "config_1".to_owned(),
                ConfigItemInternal {
                    config_item_enum: ConfigItemEnumInternal::Object(
                        ConfigObjectInternal {
                            fields: HashMap::from([
                                (
                                    "values".to_owned(),
                                    ConfigItemInternal {
                                        config_item_enum: ConfigItemEnumInternal::Object(
                                            ConfigObjectInternal {
                                                fields: HashMap::from([
                                                    (
                                                        "value_1".to_owned(),
                                                        ConfigItemInternal {
                                                            config_item_enum: ConfigItemEnumInternal::String("value123".to_owned()),
                                                        }
                                                    ),
                                                    (
                                                        "value_2".to_owned(),
                                                        ConfigItemInternal {
                                                            config_item_enum: ConfigItemEnumInternal::Array(
                                                                ConfigArrayInternal {
                                                                    values: vec![
                                                                        ConfigItemInternal {
                                                                            config_item_enum: ConfigItemEnumInternal::String("list_value_1".to_owned()),
                                                                        },
                                                                        ConfigItemInternal {
                                                                            config_item_enum: ConfigItemEnumInternal::String("list_value_2".to_owned()),
                                                                        },
                                                                    ],
                                                                }
                                                            ),
                                                        }
                                                    )
                                                ])
                                            }
                                        ),
                                    }
                                ),
                                (
                                    "agent_name".to_owned(),
                                    ConfigItemInternal {
                                        config_item_enum: ConfigItemEnumInternal::String("agent_A".to_owned()),
                                    }
                                ),
                                (
                                    "config_file".to_owned(),
                                    ConfigItemInternal {
                                        config_item_enum: ConfigItemEnumInternal::String("text data".to_owned()),
                                    }
                                ),
                                (
                                    "binary_file".to_owned(),
                                    ConfigItemInternal {
                                        config_item_enum: ConfigItemEnumInternal::String("base64_data".to_owned()),
                                    }
                                )
                            ])
                        }
                    ),
                }
            ),
            (
                "config_2".to_owned(),
                ConfigItemInternal {
                    config_item_enum: ConfigItemEnumInternal::String("value_3".to_owned()),
                }
            )
        ])
    }
}

#[cfg(test)]
mod tests {
    use crate::ank_base::ConfigItemInternal;

    macro_rules! sample_config {
        ($expression:ident) => {{
            $expression::array([
                $expression::string("string_value"),
                $expression::object([
                    ("key_1", $expression::string("object_value_1")),
                    (
                        "key_2",
                        $expression::object([
                            ("key_2_1", $expression::string("object_value_2_1")),
                            ("key_2_2", $expression::string("object_value_2_2")),
                        ]),
                    ),
                    (
                        "key_3",
                        $expression::array([
                            $expression::string("array_value_1"),
                            $expression::string("array_value_2"),
                            $expression::string("array_value_3"),
                        ]),
                    ),
                ]),
            ])
        }};
    }

    mod proto {
        use crate::ank_base;

        pub fn none() -> ank_base::ConfigItem {
            ank_base::ConfigItem { config_item_enum: None }
        }

        pub fn string(string: &str) -> ank_base::ConfigItem {
            ank_base::ConfigItem {
                config_item_enum: Some(ank_base::config_item::ConfigItemEnum::String(
                    string.to_string(),
                )),
            }
        }

        pub fn array<const N: usize>(values: [ank_base::ConfigItem; N]) -> ank_base::ConfigItem {
            ank_base::ConfigItem {
                config_item_enum: Some(ank_base::config_item::ConfigItemEnum::Array(
                    ank_base::ConfigArray {
                        values: values.to_vec(),
                    },
                )),
            }
        }

        pub fn object<const N: usize>(
            fields: [(&str, ank_base::ConfigItem); N],
        ) -> ank_base::ConfigItem {
            ank_base::ConfigItem {
                config_item_enum: Some(ank_base::config_item::ConfigItemEnum::Object(
                    ank_base::ConfigObject {
                        fields: fields
                            .into_iter()
                            .map(|(key, value)| (key.to_string(), value))
                            .collect(),
                    },
                )),
            }
        }
    }

    mod internal {
        use crate::ank_base::{ConfigItemEnumInternal, ConfigArrayInternal, ConfigObjectInternal, ConfigItemInternal};

        pub fn string(string: &str) -> ConfigItemInternal {
            ConfigItemInternal {
                config_item_enum: ConfigItemEnumInternal::String(string.to_string()),
            }
        }

        pub fn array<const N: usize>(values: [ConfigItemInternal; N]) -> ConfigItemInternal {
            ConfigItemInternal {
                config_item_enum: ConfigItemEnumInternal::Array(
                    ConfigArrayInternal {
                        values: values.to_vec(),
                    },
                ),
            }
        }

        pub fn object<const N: usize>(fields: [(&str, ConfigItemInternal); N]) -> ConfigItemInternal {
            ConfigItemInternal {
                config_item_enum: ConfigItemEnumInternal::Object(
                    ConfigObjectInternal {
                        fields: fields
                            .into_iter()
                            .map(|(key, value)| (key.to_string(), value))
                            .collect(),
                    }
                ),
            }
        }
    }
}

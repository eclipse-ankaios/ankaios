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

use crate::helpers::serialize_to_ordered_map;
use api::ank_base::{self, config_item};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum ConfigItem {
    String(String),
    ConfigArray(Vec<ConfigItem>),
    ConfigObject(#[serde(serialize_with = "serialize_to_ordered_map")] HashMap<String, ConfigItem>),
}

impl From<ConfigItem> for ank_base::ConfigItem {
    fn from(value: ConfigItem) -> Self {
        Self {
            config_item: Some(match value {
                ConfigItem::String(string) => config_item::ConfigItem::String(string),
                ConfigItem::ConfigArray(array) => {
                    config_item::ConfigItem::Array(ank_base::ConfigArray {
                        values: array.into_iter().map(Into::into).collect(),
                    })
                }
                ConfigItem::ConfigObject(object) => {
                    config_item::ConfigItem::Object(ank_base::ConfigObject {
                        fields: object
                            .into_iter()
                            .map(|(key, value)| (key, value.into()))
                            .collect(),
                    })
                }
            }),
        }
    }
}

impl TryFrom<ank_base::ConfigItem> for ConfigItem {
    type Error = String;
    fn try_from(value: ank_base::ConfigItem) -> Result<Self, Self::Error> {
        let Some(value) = value.config_item else {
            return Err("Value of ConfigItem is None".into());
        };
        Ok(match value {
            config_item::ConfigItem::String(string) => Self::String(string),
            config_item::ConfigItem::Array(ank_base::ConfigArray { values }) => Self::ConfigArray(
                values
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<ConfigItem>, Self::Error>>()?,
            ),
            config_item::ConfigItem::Object(ank_base::ConfigObject { fields }) => {
                Self::ConfigObject(
                    fields
                        .into_iter()
                        .map(|(key, value)| Ok((key, value.try_into()?)))
                        .collect::<Result<HashMap<String, ConfigItem>, Self::Error>>()?,
                )
            }
        })
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
    use api::ank_base;

    use crate::objects::ConfigItem;

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
        use api::ank_base;

        pub fn none() -> ank_base::ConfigItem {
            ank_base::ConfigItem { config_item: None }
        }

        pub fn string(string: &str) -> ank_base::ConfigItem {
            ank_base::ConfigItem {
                config_item: Some(ank_base::config_item::ConfigItem::String(
                    string.to_string(),
                )),
            }
        }

        pub fn array<const N: usize>(values: [ank_base::ConfigItem; N]) -> ank_base::ConfigItem {
            ank_base::ConfigItem {
                config_item: Some(ank_base::config_item::ConfigItem::Array(
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
                config_item: Some(ank_base::config_item::ConfigItem::Object(
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
        use crate::objects::ConfigItem;

        pub fn string(string: &str) -> ConfigItem {
            ConfigItem::String(string.to_string())
        }

        pub fn array<const N: usize>(values: [ConfigItem; N]) -> ConfigItem {
            ConfigItem::ConfigArray(values.to_vec())
        }

        pub fn object<const N: usize>(fields: [(&str, ConfigItem); N]) -> ConfigItem {
            ConfigItem::ConfigObject(
                fields
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value))
                    .collect(),
            )
        }
    }

    #[test]
    fn convert_from_proto_to_internal() {
        let proto_config = sample_config!(proto);
        let expected_config = Ok(sample_config!(internal));

        let converted_config = ConfigItem::try_from(proto_config);

        assert_eq!(converted_config, expected_config);
    }

    #[test]
    fn convert_from_internal_to_proto() {
        let internal_config = sample_config!(internal);
        let expected_config = sample_config!(proto);

        let converted_config = ank_base::ConfigItem::from(internal_config);

        assert_eq!(converted_config, expected_config);
    }

    #[test]
    fn convert_from_none_to_internal_fails() {
        let proto_config = proto::none();
        let converted_config = ConfigItem::try_from(proto_config);
        assert_eq!(converted_config, Err("Value of ConfigItem is None".into()));
    }

    #[test]
    fn convert_from_array_containing_none_to_internal_fails() {
        let proto_config = proto::array([proto::none()]);
        let converted_config = ConfigItem::try_from(proto_config);
        assert_eq!(converted_config, Err("Value of ConfigItem is None".into()));
    }

    #[test]
    fn convert_from_object_containing_none_to_internal_fails() {
        let proto_config = proto::object([("key", proto::none())]);
        let converted_config = ConfigItem::try_from(proto_config);
        assert_eq!(converted_config, Err("Value of ConfigItem is None".into()));
    }
}

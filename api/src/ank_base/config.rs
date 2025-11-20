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

use crate::ank_base::ConfigMappingsSpec;
use std::collections::HashMap;

impl<const N: usize> From<[(String, String); N]> for ConfigMappingsSpec {
    fn from(value: [(String, String); N]) -> Self {
        ConfigMappingsSpec {
            configs: value
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>(),
        }
    }
}

impl From<HashMap<String, String>> for ConfigMappingsSpec {
    fn from(value: HashMap<String, String>) -> Self {
        ConfigMappingsSpec { configs: value }
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
use crate::ank_base::{ConfigArraySpec, ConfigItemEnumSpec, ConfigItemSpec, ConfigObjectSpec};

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_config_item<T>(item: T) -> ConfigItemSpec
where
    T: Into<ConfigItemSpec>,
{
    item.into()
}

#[cfg(any(feature = "test_utils", test))]
impl From<String> for ConfigItemSpec {
    fn from(s: String) -> Self {
        ConfigItemSpec {
            config_item_enum: ConfigItemEnumSpec::String(s),
        }
    }
}

#[cfg(any(feature = "test_utils", test))]
impl From<Vec<ConfigItemSpec>> for ConfigItemSpec {
    fn from(values: Vec<ConfigItemSpec>) -> Self {
        ConfigItemSpec {
            config_item_enum: ConfigItemEnumSpec::Array(ConfigArraySpec { values }),
        }
    }
}

#[cfg(any(feature = "test_utils", test))]
impl From<HashMap<String, ConfigItemSpec>> for ConfigItemSpec {
    fn from(fields: HashMap<String, ConfigItemSpec>) -> Self {
        ConfigItemSpec {
            config_item_enum: ConfigItemEnumSpec::Object(ConfigObjectSpec { fields }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ank_base::ConfigMappingsSpec;
    use std::collections::HashMap;

    #[test]
    fn utest_config_mappings_spec_from_array() {
        let mappings: ConfigMappingsSpec = [
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
        ]
        .into();

        assert_eq!(mappings.configs.get("key1"), Some(&"value1".to_string()));
        assert_eq!(mappings.configs.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn utest_config_mappings_spec_from_hashmap() {
        let mut map = HashMap::new();
        map.insert("key1".to_string(), "value1".to_string());
        map.insert("key2".to_string(), "value2".to_string());

        let mappings: ConfigMappingsSpec = map.into();

        assert_eq!(mappings.configs.get("key1"), Some(&"value1".to_string()));
        assert_eq!(mappings.configs.get("key2"), Some(&"value2".to_string()));
    }
}

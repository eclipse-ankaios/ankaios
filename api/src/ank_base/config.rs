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

use crate::ank_base::ConfigMappingsInternal;
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
use crate::ank_base::{
    ConfigArrayInternal, ConfigItemEnumInternal, ConfigItemInternal, ConfigObjectInternal,
};

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_config_item<T>(item: T) -> ConfigItemInternal
where
    T: Into<ConfigItemInternal>,
{
    item.into()
}

#[cfg(any(feature = "test_utils", test))]
impl From<String> for ConfigItemInternal {
    fn from(s: String) -> Self {
        ConfigItemInternal {
            config_item_enum: ConfigItemEnumInternal::String(s),
        }
    }
}

#[cfg(any(feature = "test_utils", test))]
impl From<Vec<ConfigItemInternal>> for ConfigItemInternal {
    fn from(values: Vec<ConfigItemInternal>) -> Self {
        ConfigItemInternal {
            config_item_enum: ConfigItemEnumInternal::Array(ConfigArrayInternal { values }),
        }
    }
}

#[cfg(any(feature = "test_utils", test))]
impl From<HashMap<String, ConfigItemInternal>> for ConfigItemInternal {
    fn from(fields: HashMap<String, ConfigItemInternal>) -> Self {
        ConfigItemInternal {
            config_item_enum: ConfigItemEnumInternal::Object(ConfigObjectInternal { fields }),
        }
    }
}

#[cfg(test)]
mod tests {}

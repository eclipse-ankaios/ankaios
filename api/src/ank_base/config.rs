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

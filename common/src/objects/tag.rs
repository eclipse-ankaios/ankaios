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

use api::ank_base;

#[derive(Debug, Clone, Serialize, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Tag {
    pub key: String,
    pub value: String,
}

impl From<ank_base::Tag> for Tag {
    fn from(item: ank_base::Tag) -> Self {
        Tag {
            key: item.key,
            value: item.value,
        }
    }
}

impl From<Tag> for ank_base::Tag {
    fn from(item: Tag) -> Self {
        ank_base::Tag {
            key: item.key,
            value: item.value,
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

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
#[cfg(test)]
mod tests {
    use crate::objects::*;
    use api::ank_base;

    #[test]
    fn utest_converts_to_ankaios_tag() {
        let proto_tag = ank_base::Tag {
            key: String::from("key1"),
            value: String::from("value1"),
        };

        assert_eq!(
            Tag::from(proto_tag),
            Tag {
                key: String::from("key1"),
                value: String::from("value1"),
            }
        )
    }

    #[test]
    fn utest_converts_to_proto_tag() {
        assert_eq!(
            ank_base::Tag::from(Tag {
                key: String::from("key"),
                value: String::from("value"),
            }),
            ank_base::Tag {
                key: String::from("key"),
                value: String::from("value"),
            }
        );
    }
}

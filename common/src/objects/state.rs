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

use std::collections::HashMap;

use crate::helpers::serialize_to_ordered_map;
use crate::objects::Cronjob;
use crate::objects::WorkloadSpec;
use api::proto;

use super::external_state::ExternalState;

// [impl->swdd~common-object-representation~1]#[accessible_by_field_name]
// [impl->swdd~common-object-serialization~1]
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", from = "ExternalState")]
pub struct State {
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub workloads: HashMap<String, WorkloadSpec>,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub configs: HashMap<String, String>,
    #[serde(serialize_with = "serialize_to_ordered_map")]
    pub cron_jobs: HashMap<String, Cronjob>,
}

impl From<State> for proto::State {
    fn from(item: State) -> Self {
        proto::State {
            workloads: item
                .workloads
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            configs: item.configs,
            cronjobs: item
                .cron_jobs
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
        }
    }
}

impl TryFrom<proto::State> for State {
    type Error = String;

    fn try_from(item: proto::State) -> Result<Self, Self::Error> {
        Ok(State {
            workloads: item
                .workloads
                .into_iter()
                .map(|(k, v)| Ok((k.to_owned(), (k, v).try_into()?)))
                .collect::<Result<HashMap<String, WorkloadSpec>, String>>()?,
            configs: item.configs,
            cron_jobs: item
                .cronjobs
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
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

// [utest->swdd~common-conversions-between-ankaios-and-proto~1]
// [utest->swdd~common-object-representation~1]
// [utest->swdd~common-object-serialization~1]
#[cfg(test)]
mod tests {

    use api::proto;

    use crate::{
        objects::State,
        test_utils::{generate_test_proto_state, generate_test_state},
    };

    #[test]
    fn utest_converts_to_proto_state() {
        let ankaios_state = generate_test_state();
        let proto_state = generate_test_proto_state();

        assert_eq!(proto::State::from(ankaios_state), proto_state);
    }

    #[test]
    fn utest_converts_to_ankaios_state() {
        let ankaios_state = generate_test_state();
        let proto_state = generate_test_proto_state();

        assert_eq!(State::try_from(proto_state), Ok(ankaios_state));
    }
}

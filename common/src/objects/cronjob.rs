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

use api::proto;

#[derive(Debug, Clone, Serialize, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Interval {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
}

impl Interval {
    pub fn is_empty(&self) -> bool {
        if self.hours == 0 && self.minutes == 0 && self.seconds == 0 {
            return true;
        }

        false
    }
}

impl From<proto::Interval> for Interval {
    fn from(item: proto::Interval) -> Self {
        Interval {
            hours: item.hours,
            minutes: item.minutes,
            seconds: item.seconds,
        }
    }
}

impl From<Interval> for proto::Interval {
    fn from(item: Interval) -> Self {
        proto::Interval {
            hours: item.hours,
            minutes: item.minutes,
            seconds: item.seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Cronjob {
    pub workload: String,
    pub interval: Interval,
}

impl From<proto::Cronjob> for Cronjob {
    fn from(item: proto::Cronjob) -> Self {
        Cronjob {
            workload: item.workload,
            interval: item.interval.unwrap_or_default().into(),
        }
    }
}

impl From<Cronjob> for proto::Cronjob {
    fn from(item: Cronjob) -> Self {
        proto::Cronjob {
            workload: item.workload,
            interval: if item.interval.is_empty() {
                None
            } else {
                Some(item.interval.into())
            },
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
    use api::proto;

    use crate::objects::*;
    use crate::test_utils::*;

    #[test]
    fn utest_converts_to_ankaios_cronjob() {
        let input = generate_test_proto_cronjob();

        let converted = generate_test_cronjob();

        assert_eq!(Cronjob::from(input), converted);
    }

    #[test]
    fn utest_converts_to_proto_cronjob() {
        let input = generate_test_cronjob();

        let converted = generate_test_proto_cronjob();

        assert_eq!(proto::Cronjob::from(input), converted);
    }

    #[test]
    fn utest_converts_to_ankaios_cronjob_without_interval() {
        let input = proto::Cronjob {
            workload: String::from("some job"),
            interval: None,
        };

        let converted = generate_test_cronjob_empty_interval();

        assert_eq!(Cronjob::from(input), converted);
    }

    #[test]
    fn utest_converts_to_proto_cronjob_without_interval() {
        let input = generate_test_cronjob_empty_interval();

        let converted = proto::Cronjob {
            workload: String::from("some job"),
            interval: None,
        };

        assert_eq!(proto::Cronjob::from(input), converted);
    }

    #[test]
    fn utest_converts_to_ankaios_interval() {
        assert_eq!(
            Interval::from(proto::Interval {
                hours: 4,
                minutes: 3,
                seconds: 42
            }),
            Interval {
                hours: 4,
                minutes: 3,
                seconds: 42
            }
        )
    }

    #[test]
    fn utest_converts_to_proto_interval() {
        assert_eq!(
            proto::Interval::from(Interval {
                hours: 4,
                minutes: 3,
                seconds: 42
            }),
            proto::Interval {
                hours: 4,
                minutes: 3,
                seconds: 42
            }
        )
    }
}

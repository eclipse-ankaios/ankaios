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

use api::control_api;
use common::commands;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ToAnkaios {
    Request(commands::Request),
}

// [impl->swdd~agent-converts-control-interface-message-to-ankaios-object~1]
impl TryFrom<control_api::ToAnkaios> for ToAnkaios {
    type Error = String;

    fn try_from(item: control_api::ToAnkaios) -> Result<Self, Self::Error> {
        use control_api::to_ankaios::ToAnkaiosEnum;
        let to_ankaios = item
            .to_ankaios_enum
            .ok_or("ToAnkaios is None.".to_string())?;

        Ok(match to_ankaios {
            ToAnkaiosEnum::Request(content) => ToAnkaios::Request(content.try_into()?),
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
    use super::{control_api, ToAnkaios};
    use api::ank_base;
    use common::commands::{CompleteStateRequest, Request, RequestContent};

    const FIELD_1: &str = "field_1";
    const FIELD_2: &str = "field_2";
    const REQUEST_ID: &str = "id";

    // [utest->swdd~agent-converts-control-interface-message-to-ankaios-object~1]
    #[test]
    fn utest_convert_control_interface_proto_to_ankaios_object() {
        let proto_request = control_api::ToAnkaios {
            to_ankaios_enum: Some(control_api::to_ankaios::ToAnkaiosEnum::Request(
                ank_base::Request {
                    request_id: REQUEST_ID.into(),
                    request_content: Some(ank_base::request::RequestContent::CompleteStateRequest(
                        ank_base::CompleteStateRequest {
                            field_mask: vec![FIELD_1.into(), FIELD_2.into()],
                        },
                    )),
                },
            )),
        };

        let expected = ToAnkaios::Request(Request {
            request_id: REQUEST_ID.into(),
            request_content: RequestContent::CompleteStateRequest(CompleteStateRequest {
                field_mask: vec![FIELD_1.into(), FIELD_2.into()],
            }),
        });

        assert_eq!(ToAnkaios::try_from(proto_request).unwrap(), expected);
    }
}

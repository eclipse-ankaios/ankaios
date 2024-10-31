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

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlInterfaceAccess {
    #[serde(default)]
    pub allow_rules: Vec<AccessRightsRule>,
    #[serde(default)]
    pub deny_rules: Vec<AccessRightsRule>,
}

impl TryFrom<api::ank_base::ControlInterfaceAccess> for ControlInterfaceAccess {
    type Error = String;
    fn try_from(value: api::ank_base::ControlInterfaceAccess) -> Result<Self, Self::Error> {
        Ok(Self {
            allow_rules: convert_rule_vec(value.allow_rules)?,
            deny_rules: convert_rule_vec(value.deny_rules)?,
        })
    }
}

impl From<ControlInterfaceAccess> for Option<api::ank_base::ControlInterfaceAccess> {
    fn from(value: ControlInterfaceAccess) -> Self {
        if value.allow_rules.is_empty() && value.deny_rules.is_empty() {
            None
        } else {
            Some(api::ank_base::ControlInterfaceAccess {
                allow_rules: value.allow_rules.into_iter().map(|x| x.into()).collect(),
                deny_rules: value.deny_rules.into_iter().map(|x| x.into()).collect(),
            })
        }
    }
}

fn convert_rule_vec(
    value: Vec<api::ank_base::AccessRightsRule>,
) -> Result<Vec<AccessRightsRule>, String> {
    value
        .into_iter()
        .map(AccessRightsRule::try_from)
        .collect::<Result<Vec<AccessRightsRule>, String>>()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum AccessRightsRule {
    StateRule(StateRule),
}

impl TryFrom<api::ank_base::AccessRightsRule> for AccessRightsRule {
    type Error = String;

    fn try_from(value: api::ank_base::AccessRightsRule) -> Result<Self, Self::Error> {
        match value
            .access_rights_rule_enum
            .ok_or_else(|| "Access right rule empty".to_string())?
        {
            api::ank_base::access_rights_rule::AccessRightsRuleEnum::StateRule(state_rule) => {
                Ok(Self::StateRule(state_rule.try_into()?))
            }
        }
    }
}

impl From<AccessRightsRule> for api::ank_base::AccessRightsRule {
    fn from(value: AccessRightsRule) -> Self {
        Self {
            access_rights_rule_enum: match value {
                AccessRightsRule::StateRule(state) => Some(
                    api::ank_base::access_rights_rule::AccessRightsRuleEnum::StateRule(
                        state.into(),
                    ),
                ),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StateRule {
    pub operation: ReadWriteEnum,
    pub filter_mask: Vec<String>,
}

impl TryFrom<api::ank_base::StateRule> for StateRule {
    type Error = String;
    fn try_from(value: api::ank_base::StateRule) -> Result<Self, Self::Error> {
        Ok(Self {
            operation: value.operation.try_into()?,
            filter_mask: value.filter_masks,
        })
    }
}

impl From<StateRule> for api::ank_base::StateRule {
    fn from(value: StateRule) -> Self {
        Self {
            operation: value.operation.into(),
            filter_masks: value.filter_mask,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ReadWriteEnum {
    Nothing,
    Read,
    Write,
    ReadWrite,
}

impl TryFrom<i32> for ReadWriteEnum {
    type Error = String;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            x if x == api::ank_base::ReadWriteEnum::RwNothing as i32 => Ok(Self::Nothing),
            x if x == api::ank_base::ReadWriteEnum::RwRead as i32 => Ok(Self::Read),
            x if x == api::ank_base::ReadWriteEnum::RwWrite as i32 => Ok(Self::Write),
            x if x == api::ank_base::ReadWriteEnum::RwReadWrite as i32 => Ok(Self::ReadWrite),
            _ => Err(format!(
                "Received an unknown value '{value}' as ReadWriteEnum."
            )),
        }
    }
}

impl From<ReadWriteEnum> for i32 {
    fn from(value: ReadWriteEnum) -> Self {
        match value {
            ReadWriteEnum::Nothing => api::ank_base::ReadWriteEnum::RwNothing as i32,
            ReadWriteEnum::Read => api::ank_base::ReadWriteEnum::RwRead as i32,
            ReadWriteEnum::Write => api::ank_base::ReadWriteEnum::RwWrite as i32,
            ReadWriteEnum::ReadWrite => api::ank_base::ReadWriteEnum::RwReadWrite as i32,
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

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_control_interface_access() -> ControlInterfaceAccess {
    ControlInterfaceAccess {
        allow_rules: vec![AccessRightsRule::StateRule(StateRule {
            operation: ReadWriteEnum::ReadWrite,
            filter_mask: vec!["desiredState".to_string()],
        })],
        deny_rules: vec![AccessRightsRule::StateRule(StateRule {
            operation: ReadWriteEnum::Write,
            filter_mask: vec!["desiredState.workload.watchDog".to_string()],
        })],
    }
}

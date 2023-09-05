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

use crate::helpers::try_into_vec;

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct AccessRights {
    pub allow: Vec<AccessRightsRule>,
    pub deny: Vec<AccessRightsRule>,
}

impl AccessRights {
    pub fn is_empty(&self) -> bool {
        if self.allow.is_empty() && self.deny.is_empty() {
            return true;
        }
        false
    }
}

impl From<AccessRights> for proto::AccessRights {
    fn from(item: AccessRights) -> Self {
        proto::AccessRights {
            allow: item.allow.into_iter().map(|x| x.into()).collect(),
            deny: item.deny.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl TryFrom<proto::AccessRights> for AccessRights {
    type Error = String;
    fn try_from(item: proto::AccessRights) -> Result<Self, Self::Error> {
        Ok(AccessRights {
            allow: try_into_vec(item.allow)?,
            deny: try_into_vec(item.deny)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessRightsRule {
    pub operation: PatchOperation,
    #[serde(rename = "UpdateMask")]
    pub update_mask: Vec<String>,
    pub value: Vec<String>,
}

impl From<AccessRightsRule> for proto::AccessRightsRule {
    fn from(item: AccessRightsRule) -> Self {
        proto::AccessRightsRule {
            operation: item.operation as i32,
            update_mask: item.update_mask,
            value: item.value,
        }
    }
}

impl TryFrom<proto::AccessRightsRule> for AccessRightsRule {
    type Error = String;

    fn try_from(item: proto::AccessRightsRule) -> Result<Self, Self::Error> {
        Ok(AccessRightsRule {
            operation: item.operation.try_into()?,
            update_mask: item.update_mask,
            value: item.value,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PatchOperation {
    Replace = 0,
    Add,
    Remove,
}

impl TryFrom<i32> for PatchOperation {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            x if x == PatchOperation::Replace as i32 => Ok(PatchOperation::Replace),
            x if x == PatchOperation::Add as i32 => Ok(PatchOperation::Add),
            x if x == PatchOperation::Remove as i32 => Ok(PatchOperation::Remove),
            _ => Err(format!(
                "Received an unknown value '{value}' as PatchOperation."
            )),
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

    #[test]
    fn utest_access_rule_rights_is_empty() {
        let empty_rule = AccessRights {
            allow: vec![],
            deny: vec![],
        };

        assert!(empty_rule.is_empty());

        let access_right_rules = vec![AccessRightsRule {
            operation: PatchOperation::Add,
            update_mask: vec![String::from("udpate_mask A")],
            value: vec![String::from("value 1")],
        }];

        assert!(!AccessRights {
            allow: vec![],
            deny: access_right_rules.clone(),
        }
        .is_empty());

        assert!(!AccessRights {
            allow: access_right_rules,
            deny: vec![],
        }
        .is_empty());
    }

    #[test]
    fn utest_converts_to_proto_access_rights_with_empty_allow_or_deny() {
        let access_right_rules = vec![
            AccessRightsRule {
                operation: PatchOperation::Add,
                update_mask: vec![String::from("udpate_mask A"), String::from("udpate_mask B")],
                value: vec![String::from("value 1"), String::from("value 2")],
            },
            AccessRightsRule {
                operation: PatchOperation::Remove,
                update_mask: vec![String::from("udpate_mask D"), String::from("udpate_mask E")],
                value: vec![String::from("value 2")],
            },
        ];

        let converted_access_right_rules = vec![
            proto::AccessRightsRule {
                operation: proto::PatchOperation::Add.into(),
                update_mask: vec![String::from("udpate_mask A"), String::from("udpate_mask B")],
                value: vec![String::from("value 1"), String::from("value 2")],
            },
            proto::AccessRightsRule {
                operation: proto::PatchOperation::Remove.into(),
                update_mask: vec![String::from("udpate_mask D"), String::from("udpate_mask E")],
                value: vec![String::from("value 2")],
            },
        ];

        assert_eq!(
            proto::AccessRights::from(AccessRights {
                allow: vec![],
                deny: access_right_rules.clone(),
            }),
            proto::AccessRights {
                allow: vec![],
                deny: converted_access_right_rules.clone()
            }
        );

        assert_eq!(
            proto::AccessRights::from(AccessRights {
                deny: vec![],
                allow: access_right_rules,
            }),
            proto::AccessRights {
                deny: vec![],
                allow: converted_access_right_rules
            }
        );
    }

    #[test]
    fn utest_converts_to_ankaios_access_rights() {
        let proto = proto::AccessRights {
            allow: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask allow 1-1".into(), "mask allow 1-2".into()],
                    value: vec!["value allow 1".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Add.into(),
                    update_mask: vec!["mask allow 2".into()],
                    value: vec!["value allow 2-1".into(), "value allow 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Remove.into(),
                    update_mask: vec!["mask allow 3".into()],
                    value: vec!["value allow 13".into()],
                },
            ],
            deny: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask deny 1-1".into(), "mask deny 1-2".into()],
                    value: vec!["value deny 1".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Add.into(),
                    update_mask: vec!["mask deny 2".into()],
                    value: vec!["value deny 2-1".into(), "value deny 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Remove.into(),
                    update_mask: vec!["mask deny 3".into()],
                    value: vec!["value deny 13".into()],
                },
            ],
        };

        let expected = AccessRights {
            allow: vec![
                AccessRightsRule {
                    operation: PatchOperation::Replace,
                    update_mask: vec!["mask allow 1-1".into(), "mask allow 1-2".into()],
                    value: vec!["value allow 1".into()],
                },
                AccessRightsRule {
                    operation: PatchOperation::Add,
                    update_mask: vec!["mask allow 2".into()],
                    value: vec!["value allow 2-1".into(), "value allow 2-2".into()],
                },
                AccessRightsRule {
                    operation: PatchOperation::Remove,
                    update_mask: vec!["mask allow 3".into()],
                    value: vec!["value allow 13".into()],
                },
            ],
            deny: vec![
                AccessRightsRule {
                    operation: PatchOperation::Replace,
                    update_mask: vec!["mask deny 1-1".into(), "mask deny 1-2".into()],
                    value: vec!["value deny 1".into()],
                },
                AccessRightsRule {
                    operation: PatchOperation::Add,
                    update_mask: vec!["mask deny 2".into()],
                    value: vec!["value deny 2-1".into(), "value deny 2-2".into()],
                },
                AccessRightsRule {
                    operation: PatchOperation::Remove,
                    update_mask: vec!["mask deny 3".into()],
                    value: vec!["value deny 13".into()],
                },
            ],
        };

        assert_eq!(AccessRights::try_from(proto), Ok(expected));
    }

    #[test]
    fn utest_converts_to_ankaios_access_rights_fails_at_allow() {
        let proto = proto::AccessRights {
            allow: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask allow 1-1".into(), "mask allow 1-2".into()],
                    value: vec!["value allow 1".into()],
                },
                proto::AccessRightsRule {
                    operation: -1,
                    update_mask: vec!["mask allow 2".into()],
                    value: vec!["value allow 2-1".into(), "value allow 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Remove.into(),
                    update_mask: vec!["mask allow 3".into()],
                    value: vec!["value allow 13".into()],
                },
            ],
            deny: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask deny 1-1".into(), "mask deny 1-2".into()],
                    value: vec!["value deny 1".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Add.into(),
                    update_mask: vec!["mask deny 2".into()],
                    value: vec!["value deny 2-1".into(), "value deny 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Remove.into(),
                    update_mask: vec!["mask deny 3".into()],
                    value: vec!["value deny 13".into()],
                },
            ],
        };

        assert!(AccessRights::try_from(proto).is_err());
    }

    #[test]
    fn utest_converts_to_ankaios_access_rights_fails_at_deny() {
        let proto = proto::AccessRights {
            allow: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask allow 1-1".into(), "mask allow 1-2".into()],
                    value: vec!["value allow 1".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Add.into(),
                    update_mask: vec!["mask allow 2".into()],
                    value: vec!["value allow 2-1".into(), "value allow 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Remove.into(),
                    update_mask: vec!["mask allow 3".into()],
                    value: vec!["value allow 13".into()],
                },
            ],
            deny: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask deny 1-1".into(), "mask deny 1-2".into()],
                    value: vec!["value deny 1".into()],
                },
                proto::AccessRightsRule {
                    operation: -1,
                    update_mask: vec!["mask deny 2".into()],
                    value: vec!["value deny 2-1".into(), "value deny 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Remove.into(),
                    update_mask: vec!["mask deny 3".into()],
                    value: vec!["value deny 13".into()],
                },
            ],
        };

        assert!(AccessRights::try_from(proto).is_err());
    }

    #[test]
    fn utest_converts_to_ankaios_access_rights_fails_at_both() {
        let proto = proto::AccessRights {
            allow: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask allow 1-1".into(), "mask allow 1-2".into()],
                    value: vec!["value allow 1".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Add.into(),
                    update_mask: vec!["mask allow 2".into()],
                    value: vec!["value allow 2-1".into(), "value allow 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: -1,
                    update_mask: vec!["mask allow 3".into()],
                    value: vec!["value allow 13".into()],
                },
            ],
            deny: vec![
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Replace.into(),
                    update_mask: vec!["mask deny 1-1".into(), "mask deny 1-2".into()],
                    value: vec!["value deny 1".into()],
                },
                proto::AccessRightsRule {
                    operation: -1,
                    update_mask: vec!["mask deny 2".into()],
                    value: vec!["value deny 2-1".into(), "value deny 2-2".into()],
                },
                proto::AccessRightsRule {
                    operation: proto::PatchOperation::Remove.into(),
                    update_mask: vec!["mask deny 3".into()],
                    value: vec!["value deny 13".into()],
                },
            ],
        };

        assert!(AccessRights::try_from(proto).is_err());
    }

    #[test]
    fn utest_converts_to_ankaios_access_rights_rule() {
        let access_rightes_rule_replace = proto::AccessRightsRule {
            operation: proto::PatchOperation::Replace.into(),
            update_mask: vec![String::from("maskA")],
            value: vec![String::from("valueA")],
        };

        let access_rightes_rule_add = proto::AccessRightsRule {
            operation: proto::PatchOperation::Add.into(),
            update_mask: vec![String::from("maskA"), String::from("maskB")],
            value: vec![String::from("valueA"), String::from("valueB")],
        };

        let access_rightes_rule_remove = proto::AccessRightsRule {
            operation: proto::PatchOperation::Remove.into(),
            update_mask: vec![String::from("maskA"), String::from("maskB")],
            value: vec![String::from("valueA"), String::from("valueB")],
        };

        assert_eq!(
            AccessRightsRule::try_from(access_rightes_rule_replace),
            Ok(AccessRightsRule {
                operation: PatchOperation::Replace,
                update_mask: vec![String::from("maskA")],
                value: vec![String::from("valueA")],
            })
        );

        assert_eq!(
            AccessRightsRule::try_from(access_rightes_rule_add),
            Ok(AccessRightsRule {
                operation: PatchOperation::Add,
                update_mask: vec![String::from("maskA"), String::from("maskB")],
                value: vec![String::from("valueA"), String::from("valueB")],
            })
        );

        assert_eq!(
            AccessRightsRule::try_from(access_rightes_rule_remove),
            Ok(AccessRightsRule {
                operation: PatchOperation::Remove,
                update_mask: vec![String::from("maskA"), String::from("maskB")],
                value: vec![String::from("valueA"), String::from("valueB")],
            })
        );
    }

    #[test]
    fn utest_converts_to_ankaios_access_rights_rule_fails_on_unknown_expected_state() {
        let access_rightes_rule = proto::AccessRightsRule {
            operation: 4,
            update_mask: vec![String::from("maskA"), String::from("maskB")],
            value: vec![String::from("valueA"), String::from("valueB")],
        };

        assert!(AccessRightsRule::try_from(access_rightes_rule).is_err());
    }

    #[test]
    fn utest_converts_to_proto_access_rights_rule() {
        let add = AccessRightsRule {
            operation: PatchOperation::Add,
            update_mask: vec![
                String::from("udpate_mask A"),
                String::from("udpate_mask B"),
                String::from("udpate_mask C"),
            ],
            value: vec![
                String::from("value 1"),
                String::from("value 2"),
                String::from("value 3"),
            ],
        };

        let mut remove = add.clone();
        remove.operation = PatchOperation::Remove;

        let mut replace = add.clone();
        replace.operation = PatchOperation::Replace;

        let expected_add = proto::AccessRightsRule {
            operation: proto::PatchOperation::Add.into(),
            update_mask: vec![
                String::from("udpate_mask A"),
                String::from("udpate_mask B"),
                String::from("udpate_mask C"),
            ],
            value: vec![
                String::from("value 1"),
                String::from("value 2"),
                String::from("value 3"),
            ],
        };

        let mut expected_remove = expected_add.clone();
        expected_remove.operation = proto::PatchOperation::Remove.into();

        let mut expected_replace = expected_add.clone();
        expected_replace.operation = proto::PatchOperation::Replace.into();

        assert_eq!(proto::AccessRightsRule::from(add), expected_add);
        assert_eq!(proto::AccessRightsRule::from(remove), expected_remove);
        assert_eq!(proto::AccessRightsRule::from(replace), expected_replace);
    }

    #[test]
    fn utest_patch_operation_from_int() {
        assert_eq!(
            PatchOperation::try_from(0).unwrap(),
            PatchOperation::Replace
        );
        assert_eq!(PatchOperation::try_from(1).unwrap(), PatchOperation::Add);
        assert_eq!(PatchOperation::try_from(2).unwrap(), PatchOperation::Remove);
        assert_eq!(
            PatchOperation::try_from(100).unwrap_err(),
            Err::<PatchOperation, String>(
                "Received an unknown value '100' as PatchOperation.".to_string()
            )
            .unwrap_err()
        );
    }
}

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

pub mod path;
pub mod path_pattern;
pub mod rule;

use common::{
    commands::Request,
    objects::{AccessRightsRule, ControlInterfaceAccess, ReadWriteEnum},
};
use path_pattern::{AllowPathPattern, DenyPathPattern, PathPattern};
#[cfg(not(test))]
use rule::Rule;

#[cfg(test)]
use test::MockRule as Rule;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Authorizer {
    allow_write_state_rule: Vec<Rule<AllowPathPattern>>,
    deny_write_state_rule: Vec<Rule<DenyPathPattern>>,
    allow_read_state_rule: Vec<Rule<AllowPathPattern>>,
    deny_read_state_rule: Vec<Rule<DenyPathPattern>>,
    allow_read_write_state_rule: Vec<Rule<AllowPathPattern>>,
    deny_read_write_state_rule: Vec<Rule<DenyPathPattern>>,
}

impl Authorizer {
    #[cfg(test)]
    pub fn test_value(name: &str) -> Self {
        Self {
            allow_write_state_rule: vec![Rule::create(vec![name.into()])],
            ..Default::default()
        }
    }

    pub fn authorize(&self, request: &Request) -> bool {
        match &request.request_content {
            common::commands::RequestContent::CompleteStateRequest(r) => {
                r.field_mask.iter().all(|path_string| {
                    let path = path_string.as_str().into();
                    let allow_reason = if let (true, reason) =
                        self.allow_read_state_rule.matches(&path)
                    {
                        reason
                    } else if let (true, reason) = self.allow_read_write_state_rule.matches(&path) {
                        reason
                    } else {
                        log::debug!(
                            "Denying field mask '{}' of request '{}' as no rule matches",
                            path_string,
                            request.request_id
                        );
                        return false;
                    };

                    let deny_reason = if let (true, reason) =
                        self.deny_read_state_rule.matches(&path)
                    {
                        reason
                    } else if let (true, reason) = self.deny_read_write_state_rule.matches(&path) {
                        reason
                    } else {
                        log::debug!(
                            "Allow field mask '{}' of request '{}' as '{}' is allowed",
                            path_string,
                            request.request_id,
                            allow_reason
                        );
                        return true;
                    };

                    log::debug!(
                        "Deny field mask '{}' of request '{}',also allowed by '{}', as denied by '{}'",
                        path_string,
                        request.request_id,
                        allow_reason,
                        deny_reason
                    );
                    false
                })
            }
            common::commands::RequestContent::UpdateStateRequest(r) => {
                r.update_mask.iter().all(|path_string| {
                    let path = path_string.as_str().into();
                    let allow_reason = if let (true, reason) =
                        self.allow_write_state_rule.matches(&path)
                    {
                        reason
                    } else if let (true, reason) = self.allow_read_write_state_rule.matches(&path) {
                        reason
                    } else {
                        log::debug!(
                            "Deny update mask '{}' of request '{}' as no rule matches",
                            path_string,
                            request.request_id
                        );
                        return false;
                    };

                    let deny_reason = if let (true, reason) =
                        self.deny_write_state_rule.matches(&path)
                    {
                        reason
                    } else if let (true, reason) = self.deny_read_write_state_rule.matches(&path) {
                        reason
                    } else {
                        log::debug!(
                            "Allow update mask '{}' of request '{}' as '{}' is allowed",
                            path_string,
                            request.request_id,
                            allow_reason
                        );
                        return true;
                    };

                    log::debug!(
                        "Deny update mask '{}' of request '{}', also allowed by '{}', as denied by '{}'",
                        path_string,
                        request.request_id,
                        allow_reason,
                        deny_reason
                    );
                    false
                })
            }
        }
    }
}

impl From<&ControlInterfaceAccess> for Authorizer {
    fn from(value: &ControlInterfaceAccess) -> Self {
        struct ReadWriteFiltered<T: PathPattern> {
            read: Vec<Rule<T>>,
            write: Vec<Rule<T>>,
            read_write: Vec<Rule<T>>,
        }

        fn split_to_read_write_rules<T>(rule_list: &[AccessRightsRule]) -> ReadWriteFiltered<T>
        where
            T: PathPattern,
            T: for<'a> From<&'a str>,
        {
            let mut res = ReadWriteFiltered {
                read: Vec::new(),
                write: Vec::new(),
                read_write: Vec::new(),
            };

            for access_rights in rule_list {
                let AccessRightsRule::StateRule(state_rule) = access_rights;
                let rule = Rule::create(
                    state_rule
                        .filter_mask
                        .iter()
                        .map(|x| (**x).into())
                        .collect(),
                );
                match state_rule.operation {
                    ReadWriteEnum::Read => res.read.push(rule),
                    ReadWriteEnum::Write => res.write.push(rule),
                    ReadWriteEnum::ReadWrite => res.read_write.push(rule),
                    ReadWriteEnum::Nothing => {}
                };
            }

            res
        }

        let allow_rules = split_to_read_write_rules(&value.allow_rules);
        let deny_rules = split_to_read_write_rules(&value.deny_rules);

        Self {
            allow_write_state_rule: allow_rules.write,
            deny_write_state_rule: deny_rules.write,
            allow_read_state_rule: allow_rules.read,
            deny_read_state_rule: deny_rules.read,
            allow_read_write_state_rule: allow_rules.read_write,
            deny_read_write_state_rule: deny_rules.read_write,
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

#[cfg(test)]
mod test {
    use std::marker::PhantomData;

    use common::{
        commands::{CompleteStateRequest, Request, UpdateStateRequest},
        objects::{AccessRightsRule, ControlInterfaceAccess, StateRule},
    };

    use super::super::authorizer::path_pattern::{AllowPathPattern, DenyPathPattern};

    use super::{path::Path, path_pattern::PathPattern, Authorizer};

    const MATCHING_PATH: &str = "matching.path";

    enum RuleType {
        AllowWrite,
        DenyWrite,
        AllowRead,
        DenyRead,
        AllowReadWrite,
        DenyReadWrite,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct MockRule<T> {
        patterns: Option<Vec<T>>,
        phantom: PhantomData<T>,
    }

    impl<T> MockRule<T> {
        pub fn create(patterns: Vec<T>) -> Self {
            Self {
                patterns: Some(patterns),
                phantom: PhantomData,
            }
        }

        fn default() -> Self {
            Self {
                patterns: None,
                phantom: PhantomData,
            }
        }
    }

    impl<T> PathPattern for MockRule<T> {
        fn matches(&self, path: &Path) -> (bool, String) {
            if path.to_string() == MATCHING_PATH {
                (true, "".into())
            } else {
                (false, "".into())
            }
        }
    }

    fn create_authorizer(matching_rules: &[RuleType]) -> Authorizer {
        let mut res: Authorizer = Default::default();

        for rule_to_change in matching_rules {
            match rule_to_change {
                RuleType::AllowWrite => res.allow_write_state_rule.push(MockRule::default()),
                RuleType::DenyWrite => res.deny_write_state_rule.push(MockRule::default()),
                RuleType::AllowRead => res.allow_read_state_rule.push(MockRule::default()),
                RuleType::DenyRead => res.deny_read_state_rule.push(MockRule::default()),
                RuleType::AllowReadWrite => {
                    res.allow_read_write_state_rule.push(MockRule::default())
                }
                RuleType::DenyReadWrite => res.deny_read_write_state_rule.push(MockRule::default()),
            }
        }

        res
    }

    #[test]
    fn utest_read_requests() {
        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::CompleteStateRequest(
                CompleteStateRequest {
                    field_mask: vec![MATCHING_PATH.into()],
                },
            ),
        };

        let authorizer = create_authorizer(&[]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowRead]);
        assert!(authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowReadWrite]);
        assert!(authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowWrite]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowRead, RuleType::DenyRead]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowRead, RuleType::DenyReadWrite]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowRead, RuleType::DenyWrite]);
        assert!(authorizer.authorize(&request));
    }

    #[test]
    fn utest_write_requests() {
        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    update_mask: vec![MATCHING_PATH.into()],
                    state: Default::default(),
                },
            )),
        };

        let authorizer = create_authorizer(&[]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowWrite]);
        assert!(authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowReadWrite]);
        assert!(authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowRead]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowWrite, RuleType::DenyWrite]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowWrite, RuleType::DenyReadWrite]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::AllowWrite, RuleType::DenyRead]);
        assert!(authorizer.authorize(&request));
    }

    #[test]
    fn utest_authorizer_from_control_interface_access() {
        let access_rights = ControlInterfaceAccess {
            allow_rules: vec![
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::Nothing,
                    filter_mask: vec!["allow.nothing".into()],
                }),
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::Read,
                    filter_mask: vec!["allow.read".into()],
                }),
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::Write,
                    filter_mask: vec!["allow.write".into()],
                }),
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::ReadWrite,
                    filter_mask: vec!["allow.read.write".into()],
                }),
            ],
            deny_rules: vec![
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::Nothing,
                    filter_mask: vec!["deny.nothing".into()],
                }),
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::Read,
                    filter_mask: vec!["deny.read".into()],
                }),
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::Write,
                    filter_mask: vec!["deny.write".into()],
                }),
                AccessRightsRule::StateRule(StateRule {
                    operation: common::objects::ReadWriteEnum::ReadWrite,
                    filter_mask: vec!["deny.read.write".into()],
                }),
            ],
        };

        let authorizer = Authorizer::from(&access_rights);

        assert_eq!(
            authorizer.allow_read_state_rule,
            vec![MockRule {
                patterns: Some(vec![AllowPathPattern::from("allow.read")]),
                phantom: PhantomData
            }]
        );
        assert_eq!(
            authorizer.allow_write_state_rule,
            vec![MockRule {
                patterns: Some(vec![AllowPathPattern::from("allow.write")]),
                phantom: PhantomData
            }]
        );
        assert_eq!(
            authorizer.allow_read_write_state_rule,
            vec![MockRule {
                patterns: Some(vec![AllowPathPattern::from("allow.read.write")]),
                phantom: PhantomData
            }]
        );

        assert_eq!(
            authorizer.deny_read_state_rule,
            vec![MockRule {
                patterns: Some(vec![DenyPathPattern::from("deny.read")]),
                phantom: PhantomData
            }]
        );
        assert_eq!(
            authorizer.deny_write_state_rule,
            vec![MockRule {
                patterns: Some(vec![DenyPathPattern::from("deny.write")]),
                phantom: PhantomData
            }]
        );
        assert_eq!(
            authorizer.deny_read_write_state_rule,
            vec![MockRule {
                patterns: Some(vec![DenyPathPattern::from("deny.read.write")]),
                phantom: PhantomData
            }]
        );
    }
}

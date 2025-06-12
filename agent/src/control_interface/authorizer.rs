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

mod path;
mod path_pattern;
mod rules;

use std::{sync::Arc, vec};

use common::{
    commands::Request,
    objects::{AccessRightsRule, ControlInterfaceAccess, ReadWriteEnum},
    std_extensions::UnreachableOption,
};
use path_pattern::{AllowPathPattern, DenyPathPattern, PathPatternMatcher};
use rules::{LogRule, StateRule};

#[cfg(test)]
use mockall::mock;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Authorizer {
    state_allow_write: Vec<Arc<StateRule<AllowPathPattern>>>,
    state_allow_read: Vec<Arc<StateRule<AllowPathPattern>>>,
    state_deny_write: Vec<Arc<StateRule<DenyPathPattern>>>,
    state_deny_read: Vec<Arc<StateRule<DenyPathPattern>>>,
    log_allow: Vec<LogRule>,
    log_deny: Vec<LogRule>,
}

#[cfg(test)]
mock! {
    #[derive(Debug)]
    pub Authorizer {
        pub fn authorize(&self, request: &Request) -> bool;
    }

    impl PartialEq for Authorizer {
        fn eq(&self, other: &Self) -> bool;
    }

    impl From<&ControlInterfaceAccess> for Authorizer {
        fn from(value: &ControlInterfaceAccess) -> Self;
    }
}

impl Authorizer {
    // [impl->swdd~agent-authorizing-request-operations~1]
    // [impl->swdd~agent-authorizing-condition-element-filter-mask-allowed~1]
    pub fn authorize(&self, request: &Request) -> bool {
        match &request.request_content {
            common::commands::RequestContent::CompleteStateRequest(r) => {
                let field_mask = if r.field_mask.is_empty() {
                    // [impl->swdd~agent-authorizing-request-without-filter-mask~2]
                    &vec!["".into()]
                } else {
                    &r.field_mask
                };
                // [impl->swdd~agent-authorizing-all-elements-of-filter-mask-allowed~1]
                field_mask.iter().all(|path_string| {
                    let path = path_string.as_str().into();

                    // [impl->swdd~agent-authorizing-matching-allow-rules~1]
                    let allow_reason = if let (true, reason) = self.state_allow_read.matches(&path) {
                        reason
                    } else {
                        log::info!(
                            "Denying field mask '{}' of request '{}' as no rule matches",
                            path_string,
                            request.request_id
                        );
                        return false;
                    };

                    // [impl->swdd~agent-authorizing-matching-deny-rules~1]
                    let deny_reason = if let (true, reason) = self.state_deny_read.matches(&path) {
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

                    log::info!(
                        "Deny field mask '{}' of request '{}', also allowed by '{}', as denied by '{}'",
                        path_string,
                        request.request_id,
                        allow_reason,
                        deny_reason
                    );
                    false
                })
            }
            common::commands::RequestContent::UpdateStateRequest(r) => {
                let update_mask: &Vec<_> = if r.update_mask.is_empty() {
                    // [impl->swdd~agent-authorizing-request-without-filter-mask~2]
                    &vec!["".into()]
                } else {
                    &r.update_mask
                };
                // [impl->swdd~agent-authorizing-all-elements-of-filter-mask-allowed~1]
                update_mask.iter().all(|path_string| {
                    let path = path_string.as_str().into();

                    // [impl->swdd~agent-authorizing-matching-allow-rules~1]
                    let allow_reason = if let (true, reason) = self.state_allow_write.matches(&path) {
                        reason
                    } else {
                        log::info!(
                            "Deny update mask '{}' of request '{}' as no rule matches",
                            path_string,
                            request.request_id
                        );
                        return false;
                    };

                    // [impl->swdd~agent-authorizing-matching-deny-rules~1]
                    let deny_reason = if let (true, reason) = self.state_deny_write.matches(&path) {
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

                    log::info!(
                        "Deny update mask '{}' of request '{}', also allowed by '{}', as denied by '{}'",
                        path_string,
                        request.request_id,
                        allow_reason,
                        deny_reason
                    );
                    false
                })
            }
            common::commands::RequestContent::LogsRequest(logs_request) => {
                if logs_request.workload_names.is_empty() {
                    log::info!(
                        "Deny logs request '{}' as no workload names are provided",
                        request.request_id
                    );
                    return false;
                }

                let allow_reason = logs_request
                    .workload_names
                    .iter()
                    .find(|instance_name| {
                        self.log_allow
                            .iter()
                            .any(|allow_rule| allow_rule.matches(instance_name.workload_name()))
                    })
                    .map(|instance_name| {
                        format!("allowed by rule for workload '{}'", instance_name)
                    });

                if allow_reason.is_none() {
                    log::info!(
                        "Deny log request '{}' as no rule matches any workload name",
                        request.request_id
                    );
                    return false;
                }

                let deny_reason = logs_request
                    .workload_names
                    .iter()
                    .find(|instance_name| {
                        self.log_deny
                            .iter()
                            .any(|deny_rule| deny_rule.matches(instance_name.workload_name()))
                    })
                    .map(|instance_name| {
                        format!("denied by rule for workload '{}'", instance_name)
                    });

                if deny_reason.is_none() {
                    log::debug!(
                        "Allow log request '{}' as '{}' is allowed",
                        request.request_id,
                        allow_reason.unwrap_or_unreachable()
                    );
                    return true;
                }

                log::info!(
                    "Deny log request '{}' as '{}' is allowed, but also denied by '{}'",
                    request.request_id,
                    allow_reason.unwrap_or_unreachable(),
                    deny_reason.unwrap_or_unreachable()
                );
                false
            }
            common::commands::RequestContent::LogsCancelRequest => true,
        }
    }
}

impl From<&ControlInterfaceAccess> for Authorizer {
    fn from(value: &ControlInterfaceAccess) -> Self {
        let mut state_allow_write = vec![];
        let mut state_allow_read = vec![];
        let mut state_deny_write = vec![];
        let mut state_deny_read = vec![];
        let mut log_allow = vec![];
        let mut log_deny = vec![];

        // Parse allow rules
        for access_rule in &value.allow_rules {
            match access_rule {
                AccessRightsRule::StateRule(state_rule) => {
                    let rule = Arc::new(StateRule::create(
                        state_rule
                            .filter_mask
                            .iter()
                            .map(|x| (**x).into())
                            .collect(),
                    ));
                    match state_rule.operation {
                        ReadWriteEnum::Read => state_allow_read.push(rule),
                        ReadWriteEnum::Write => state_allow_write.push(rule),
                        ReadWriteEnum::ReadWrite => {
                            state_allow_read.push(rule.clone());
                            state_allow_write.push(rule);
                        }
                        ReadWriteEnum::Nothing => {}
                    }
                }
                AccessRightsRule::LogRule(log_rule) => {
                    log_allow.push(log_rule.workload_names.clone().into());
                }
            }
        }

        // Parse deny rules
        for access_rule in &value.deny_rules {
            match access_rule {
                AccessRightsRule::StateRule(state_rule) => {
                    let rule = Arc::new(StateRule::create(
                        state_rule
                            .filter_mask
                            .iter()
                            .map(|x| (**x).into())
                            .collect(),
                    ));
                    match state_rule.operation {
                        ReadWriteEnum::Read => state_deny_read.push(rule),
                        ReadWriteEnum::Write => state_deny_write.push(rule),
                        ReadWriteEnum::ReadWrite => {
                            state_deny_read.push(rule.clone());
                            state_deny_write.push(rule);
                        }
                        ReadWriteEnum::Nothing => {}
                    }
                }
                AccessRightsRule::LogRule(log_rule) => {
                    log_deny.push(log_rule.workload_names.clone().into());
                }
            }
        }

        Self {
            state_allow_write,
            state_allow_read,
            state_deny_write,
            state_deny_read,
            log_allow,
            log_deny,
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
    use common::{
        commands::{CompleteStateRequest, LogsRequest, Request, UpdateStateRequest},
        objects::{self, AccessRightsRule, ControlInterfaceAccess, WorkloadInstanceName},
    };
    use std::sync::Arc;

    use super::{
        path_pattern::{AllowPathPattern, DenyPathPattern},
        Authorizer, LogRule, StateRule,
    };

    const MATCHING_PATH: &str = "matching.path";
    const MATCHING_PATH_2: &str = "matching.path.2";
    const NON_MATCHING_PATH: &str = "non.matching.path";
    const WORKLOAD_NAME: &str = "workload_name";
    const NON_EXISTING_WORKLOAD_NAME: &str = "non_existing_workload_name";

    type FieldMasks = Vec<String>;
    enum RuleType {
        StateAllowWrite(FieldMasks),
        StateDenyWrite(FieldMasks),
        StateAllowRead(FieldMasks),
        StateDenyRead(FieldMasks),
        StateAllowReadWrite(FieldMasks),
        StateDenyReadWrite(FieldMasks),
        LogAllow(Vec<String>),
        LogDeny(Vec<String>),
    }

    fn populate_authorizer(mut authorizer: Authorizer, rules: &[RuleType]) -> Authorizer {
        for rule in rules {
            match rule {
                RuleType::StateAllowWrite(masks) => {
                    let rule = Arc::new(StateRule::<AllowPathPattern>::create(
                        masks.iter().map(|x| x.as_str().into()).collect(),
                    ));
                    authorizer.state_allow_write.push(rule.clone())
                }
                RuleType::StateDenyWrite(masks) => {
                    let rule = Arc::new(StateRule::<DenyPathPattern>::create(
                        masks.iter().map(|x| x.as_str().into()).collect(),
                    ));
                    authorizer.state_deny_write.push(rule.clone())
                }
                RuleType::StateAllowRead(masks) => {
                    let rule = Arc::new(StateRule::<AllowPathPattern>::create(
                        masks.iter().map(|x| x.as_str().into()).collect(),
                    ));
                    authorizer.state_allow_read.push(rule.clone())
                }
                RuleType::StateDenyRead(masks) => {
                    let rule = Arc::new(StateRule::<DenyPathPattern>::create(
                        masks.iter().map(|x| x.as_str().into()).collect(),
                    ));
                    authorizer.state_deny_read.push(rule.clone())
                }
                RuleType::StateAllowReadWrite(masks) => {
                    let rule = Arc::new(StateRule::<AllowPathPattern>::create(
                        masks.iter().map(|x| x.as_str().into()).collect(),
                    ));
                    authorizer.state_allow_read.push(rule.clone());
                    authorizer.state_allow_write.push(rule.clone());
                }
                RuleType::StateDenyReadWrite(masks) => {
                    let rule = Arc::new(StateRule::<DenyPathPattern>::create(
                        masks.iter().map(|x| x.as_str().into()).collect(),
                    ));
                    authorizer.state_deny_read.push(rule.clone());
                    authorizer.state_deny_write.push(rule.clone());
                }
                RuleType::LogAllow(names) => {
                    authorizer.log_allow.push(LogRule::from(
                        names
                            .iter()
                            .map(|name| name.as_str().into())
                            .collect::<Vec<_>>(),
                    ));
                }
                RuleType::LogDeny(names) => {
                    authorizer.log_deny.push(LogRule::from(
                        names
                            .iter()
                            .map(|name| name.as_str().into())
                            .collect::<Vec<_>>(),
                    ));
                }
            }
        }

        authorizer
    }

    fn create_authorizer(rules: &[RuleType]) -> Authorizer {
        populate_authorizer(Authorizer::default(), rules)
    }

    // [utest->swdd~agent-authorizing-request-without-filter-mask~2]
    #[test]
    fn utest_request_without_filter_mask() {
        let mut authorizer = Authorizer::default();
        let complete_state_request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::CompleteStateRequest(
                CompleteStateRequest { field_mask: vec![] },
            ),
        };
        let update_state_request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    state: Default::default(),
                    update_mask: vec![],
                },
            )),
        };

        assert!(!authorizer.authorize(&complete_state_request));
        assert!(!authorizer.authorize(&update_state_request));

        authorizer = populate_authorizer(
            authorizer,
            &[RuleType::StateAllowReadWrite(vec!["*".into()])],
        );
        assert!(authorizer.authorize(&complete_state_request));
        assert!(authorizer.authorize(&update_state_request));

        authorizer = populate_authorizer(
            authorizer,
            &[RuleType::StateDenyReadWrite(vec!["*".into()])],
        );
        assert!(!authorizer.authorize(&complete_state_request));
        assert!(!authorizer.authorize(&update_state_request));
    }

    // [utest->swdd~agent-authorizing-request-operations~1]
    // [utest->swdd~agent-authorizing-condition-element-filter-mask-allowed~1]
    #[test]
    fn utest_read_requests_operations() {
        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::CompleteStateRequest(
                CompleteStateRequest {
                    field_mask: vec![MATCHING_PATH.into()],
                },
            ),
        };

        let authorizer = Authorizer::default();
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::StateAllowRead(vec![MATCHING_PATH.into()])]);
        assert!(authorizer.authorize(&request));
        let authorizer =
            create_authorizer(&[RuleType::StateAllowReadWrite(vec![MATCHING_PATH.into()])]);
        assert!(authorizer.authorize(&request));
        let authorizer =
            create_authorizer(&[RuleType::StateAllowWrite(vec![MATCHING_PATH.into()])]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::StateAllowRead(vec![MATCHING_PATH.into()]),
            RuleType::StateDenyRead(vec![MATCHING_PATH.into()]),
        ]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::StateAllowRead(vec![MATCHING_PATH.into()]),
            RuleType::StateDenyReadWrite(vec![MATCHING_PATH.into()]),
        ]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::StateAllowRead(vec![MATCHING_PATH.into()]),
            RuleType::StateDenyWrite(vec![MATCHING_PATH.into()]),
        ]);
        assert!(authorizer.authorize(&request));
    }

    // [utest->swdd~agent-authorizing-request-operations~1]
    // [utest->swdd~agent-authorizing-condition-element-filter-mask-allowed~1]
    #[test]
    fn utest_write_requests_operations() {
        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    update_mask: vec![MATCHING_PATH.into()],
                    state: Default::default(),
                },
            )),
        };

        let authorizer = Authorizer::default();
        assert!(!authorizer.authorize(&request));
        let authorizer =
            create_authorizer(&[RuleType::StateAllowWrite(vec![MATCHING_PATH.into()])]);
        assert!(authorizer.authorize(&request));
        let authorizer =
            create_authorizer(&[RuleType::StateAllowReadWrite(vec![MATCHING_PATH.into()])]);
        assert!(authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::StateAllowRead(vec![MATCHING_PATH.into()])]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::StateAllowWrite(vec![MATCHING_PATH.into()]),
            RuleType::StateDenyWrite(vec![MATCHING_PATH.into()]),
        ]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::StateAllowWrite(vec![MATCHING_PATH.into()]),
            RuleType::StateDenyReadWrite(vec![MATCHING_PATH.into()]),
        ]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::StateAllowWrite(vec![MATCHING_PATH.into()]),
            RuleType::StateDenyRead(vec![MATCHING_PATH.into()]),
        ]);
        assert!(authorizer.authorize(&request));
    }

    // [utest->swdd~agent-authorizing-all-elements-of-filter-mask-allowed~1]
    #[test]
    fn utest_matches_all_filter_entries() {
        let authorizer =
            create_authorizer(&[RuleType::StateAllowReadWrite(vec![MATCHING_PATH.into()])]);

        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::CompleteStateRequest(
                CompleteStateRequest {
                    field_mask: vec![MATCHING_PATH.into(), MATCHING_PATH_2.into()],
                },
            ),
        };
        assert!(authorizer.authorize(&request));

        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    update_mask: vec![MATCHING_PATH.into(), MATCHING_PATH_2.into()],
                    state: Default::default(),
                },
            )),
        };
        assert!(authorizer.authorize(&request));
    }

    // [utest->swdd~agent-authorizing-all-elements-of-filter-mask-allowed~1]
    #[test]
    fn utest_matches_not_all_filter_entries() {
        let authorizer =
            create_authorizer(&[RuleType::StateAllowReadWrite(vec![MATCHING_PATH.into()])]);

        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::CompleteStateRequest(
                CompleteStateRequest {
                    field_mask: vec![MATCHING_PATH.into(), NON_MATCHING_PATH.into()],
                },
            ),
        };
        assert!(!authorizer.authorize(&request));

        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::UpdateStateRequest(Box::new(
                UpdateStateRequest {
                    update_mask: vec![MATCHING_PATH.into(), NON_MATCHING_PATH.into()],
                    state: Default::default(),
                },
            )),
        };
        assert!(!authorizer.authorize(&request));
    }

    #[test]
    fn utest_log_request_empty() {
        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::LogsRequest(LogsRequest {
                workload_names: vec![],
                follow: false,
                tail: -1,
                since: None,
                until: None,
            }),
        };

        let authorizer = Authorizer::default();
        assert!(!authorizer.authorize(&request));
    }

    #[test]
    fn utest_log_requests_operations() {
        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::LogsRequest(LogsRequest {
                workload_names: vec![WorkloadInstanceName::new("", WORKLOAD_NAME, "")],
                follow: false,
                tail: -1,
                since: None,
                until: None,
            }),
        };

        let authorizer = Authorizer::default();
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::LogAllow(vec![WORKLOAD_NAME.into()])]);
        assert!(authorizer.authorize(&request));
        let authorizer = create_authorizer(&[RuleType::LogDeny(vec![WORKLOAD_NAME.into()])]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::LogAllow(vec![WORKLOAD_NAME.into()]),
            RuleType::LogDeny(vec![WORKLOAD_NAME.into()]),
        ]);
        assert!(!authorizer.authorize(&request));
        let authorizer = create_authorizer(&[
            RuleType::LogAllow(vec![WORKLOAD_NAME.into()]),
            RuleType::LogDeny(vec![NON_EXISTING_WORKLOAD_NAME.into()]),
        ]);
        assert!(authorizer.authorize(&request));
    }

    #[test]
    fn utest_log_cancel_request() {
        let request = Request {
            request_id: "".into(),
            request_content: common::commands::RequestContent::LogsCancelRequest,
        };

        let authorizer = Authorizer::default();
        assert!(authorizer.authorize(&request));
    }

    #[test]
    fn utest_authorizer_from_control_interface_access() {
        let control_interface_access = ControlInterfaceAccess {
            allow_rules: vec![
                AccessRightsRule::StateRule(objects::StateRule {
                    operation: common::objects::ReadWriteEnum::Read,
                    filter_mask: vec!["state.allow.read.mask".into()],
                }),
                AccessRightsRule::StateRule(objects::StateRule {
                    operation: common::objects::ReadWriteEnum::Write,
                    filter_mask: vec!["state.allow.write.mask".into()],
                }),
                AccessRightsRule::StateRule(objects::StateRule {
                    operation: common::objects::ReadWriteEnum::ReadWrite,
                    filter_mask: vec!["state.allow.read.write.mask".into()],
                }),
                AccessRightsRule::LogRule(objects::LogRule {
                    workload_names: vec!["allowed_workload".into()],
                }),
            ],
            deny_rules: vec![
                AccessRightsRule::StateRule(objects::StateRule {
                    operation: common::objects::ReadWriteEnum::Read,
                    filter_mask: vec!["state.deny.read.mask".into()],
                }),
                AccessRightsRule::StateRule(objects::StateRule {
                    operation: common::objects::ReadWriteEnum::Write,
                    filter_mask: vec!["state.deny.write.mask".into()],
                }),
                AccessRightsRule::StateRule(objects::StateRule {
                    operation: common::objects::ReadWriteEnum::ReadWrite,
                    filter_mask: vec!["state.deny.read.write.mask".into()],
                }),
                AccessRightsRule::LogRule(objects::LogRule {
                    workload_names: vec!["denied_workload".into()],
                }),
            ],
        };
        let authorizer = Authorizer::from(&control_interface_access);

        assert_eq!(
            authorizer.state_allow_read,
            vec![
                Arc::new(StateRule::create(vec!["state.allow.read.mask".into()])),
                Arc::new(StateRule::create(
                    vec!["state.allow.read.write.mask".into()]
                ))
            ]
        );
        assert_eq!(
            authorizer.state_allow_write,
            vec![
                Arc::new(StateRule::create(vec!["state.allow.write.mask".into()])),
                Arc::new(StateRule::create(
                    vec!["state.allow.read.write.mask".into()]
                ))
            ]
        );
        assert_eq!(
            authorizer.state_deny_read,
            vec![
                Arc::new(StateRule::create(vec!["state.deny.read.mask".into()])),
                Arc::new(StateRule::create(vec!["state.deny.read.write.mask".into()]))
            ]
        );
        assert_eq!(
            authorizer.state_deny_write,
            vec![
                Arc::new(StateRule::create(vec!["state.deny.write.mask".into()])),
                Arc::new(StateRule::create(vec!["state.deny.read.write.mask".into()]))
            ]
        );
        assert_eq!(
            authorizer.log_allow,
            vec![LogRule::from(vec!["allowed_workload".into()])]
        );
        assert_eq!(
            authorizer.log_deny,
            vec![LogRule::from(vec!["denied_workload".into()])]
        );

        // Check that the read_write rule is not duplicated in memory
        assert!(Arc::ptr_eq(
            &authorizer.state_allow_read[1],
            &authorizer.state_allow_write[1]
        ));
        assert!(Arc::ptr_eq(
            &authorizer.state_deny_read[1],
            &authorizer.state_deny_write[1]
        ));
    }
}

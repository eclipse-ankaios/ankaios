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

use common::{
    commands::Request,
    objects::{AccessRightsRule, ControlInterfaceAccess, ReadWriteEnum},
    PATH_SEPARATOR,
};

const WILDCARD_SYMBOL: &str = "*";

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
            allow_write_state_rule: vec![Rule {
                patterns: vec![AllowPathPattern {
                    sections: vec![PathPatterSection::String(name.into())],
                }],
            }],
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

            rule_list.iter().fold((), |(), access_rights| {
                let AccessRightsRule::StateRule(state_rule) = access_rights;
                let rule = Rule {
                    patterns: state_rule
                        .filter_mask
                        .iter()
                        .map(|x| (**x).into())
                        .collect(),
                };
                match state_rule.operation {
                    ReadWriteEnum::Read => res.read.push(rule),
                    ReadWriteEnum::Write => res.write.push(rule),
                    ReadWriteEnum::ReadWrite => res.read_write.push(rule),
                    ReadWriteEnum::Nothing => {}
                };
            });

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

#[derive(Clone, Debug, PartialEq)]
struct Rule<P: PathPattern> {
    patterns: Vec<P>,
}

impl<P: PathPattern> PathPattern for Rule<P> {
    fn matches(&self, path: &Path) -> (bool, PathPatternMatchReason) {
        self.patterns
            .iter()
            .find_map(|p| {
                if let (true, reason) = p.matches(path) {
                    Some((true, reason))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| (false, String::new()))
    }
}

#[derive(Clone, Debug)]
struct Path {
    sections: Vec<String>,
}

impl From<&str> for Path {
    fn from(value: &str) -> Self {
        Self {
            sections: if value.is_empty() {
                Vec::new()
            } else {
                value.split(PATH_SEPARATOR).map(Into::into).collect()
            },
        }
    }
}

impl ToString for Path {
    fn to_string(&self) -> String {
        self.sections.join(".")
    }
}

type PathPatternMatchReason = String;
trait PathPattern {
    fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason);
}

impl<T: PathPattern + std::fmt::Debug> PathPattern for Vec<T> {
    fn matches(&self, path: &Path) -> (bool, PathPatternMatchReason) {
        for r in self {
            if let (true, reason) = r.matches(path) {
                return (true, reason);
            }
        }
        (false, String::new())
    }
}

#[derive(Clone, Debug, PartialEq)]
struct AllowPathPattern {
    sections: Vec<PathPatterSection>,
}

impl From<&str> for AllowPathPattern {
    fn from(value: &str) -> Self {
        Self {
            sections: if value.is_empty() {
                Vec::new()
            } else {
                value.split(PATH_SEPARATOR).map(Into::into).collect()
            },
        }
    }
}

impl PathPattern for AllowPathPattern {
    fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason) {
        if self.sections.len() > other.sections.len() {
            return (false, String::new());
        }
        for (a, b) in self.sections.iter().zip(other.sections.iter()) {
            if !a.matches(b) {
                return (false, String::new());
            }
        }

        (
            true,
            self.sections
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("."),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DenyPathPattern {
    sections: Vec<PathPatterSection>,
}

impl From<&str> for DenyPathPattern {
    fn from(value: &str) -> Self {
        Self {
            sections: if value.is_empty() {
                Vec::new()
            } else {
                value.split(PATH_SEPARATOR).map(Into::into).collect()
            },
        }
    }
}

impl PathPattern for DenyPathPattern {
    fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason) {
        for (a, b) in self.sections.iter().zip(other.sections.iter()) {
            if !a.matches(b) {
                return (false, String::new());
            }
        }
        (
            true,
            self.sections
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("."),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PathPatterSection {
    Wildcard,
    String(String),
}

impl From<&str> for PathPatterSection {
    fn from(value: &str) -> Self {
        if value == WILDCARD_SYMBOL {
            Self::Wildcard
        } else {
            Self::String(value.into())
        }
    }
}

impl ToString for PathPatterSection {
    fn to_string(&self) -> String {
        match self {
            PathPatterSection::Wildcard => "*".into(),
            PathPatterSection::String(s) => s.clone(),
        }
    }
}

impl PathPatterSection {
    pub fn matches(&self, other: &String) -> bool {
        match self {
            PathPatterSection::Wildcard => true,
            PathPatterSection::String(self_string) => self_string == other,
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
mod tests {
    use crate::control_interface::authorizer::{AllowPathPattern, DenyPathPattern, PathPattern};

    #[test]
    fn utest_allow_path_pattern() {
        let p = AllowPathPattern::from("some.pre.fix");

        assert!(p.matches(&"some.pre.fix".into()).0);
        assert!(p.matches(&"some.pre.fix.test".into()).0);
        assert!(!p.matches(&"".into()).0);
        assert!(!p.matches(&"some.pre".into()).0);
        assert!(!p.matches(&"some.pre.fixtest".into()).0);
        assert!(!p.matches(&"some.pre.test".into()).0);
        assert!(!p.matches(&"some.pre.test.2".into()).0);
    }

    #[test]
    fn utest_allow_path_pattern_with_wildcard() {
        let p = AllowPathPattern::from("some.*.fix");

        assert!(p.matches(&"some.pre.fix".into()).0);
        assert!(p.matches(&"some.pre.fix.test".into()).0);
        assert!(!p.matches(&"some.pre".into()).0);
        assert!(!p.matches(&"some.pre.fixtest".into()).0);
        assert!(!p.matches(&"some.pre.test".into()).0);
        assert!(!p.matches(&"some.pre.test.2".into()).0);
        assert!(p.matches(&"some.pre2.fix".into()).0);
        assert!(p.matches(&"some.pre2.fix.test".into()).0);
        assert!(!p.matches(&"some.pre2".into()).0);
        assert!(!p.matches(&"some.pre2.fixtest".into()).0);
        assert!(!p.matches(&"some.pre2.test".into()).0);
        assert!(!p.matches(&"some.pre2.test.2".into()).0);
    }

    #[test]
    fn utest_empty_allow_path_pattern() {
        let p = AllowPathPattern::from("");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some.pre".into()).0);
    }

    #[test]
    fn utest_deny_path_pattern() {
        let p = DenyPathPattern::from("some.pre.fix");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some.pre".into()).0);
        assert!(p.matches(&"some.pre.fix".into()).0);
        assert!(p.matches(&"some.pre.fix.test".into()).0);
        assert!(!p.matches(&"some2.pre".into()).0);
        assert!(!p.matches(&"some2.pre.fix".into()).0);
        assert!(!p.matches(&"some.pre.fix2".into()).0);
        assert!(!p.matches(&"some.pre.fix2.test".into()).0);
    }

    #[test]
    fn utest_deny_path_pattern_with_wildcard() {
        let p = DenyPathPattern::from("some.*.fix");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some.pre".into()).0);
        assert!(p.matches(&"some.pre.fix".into()).0);
        assert!(p.matches(&"some.pre.fix.test".into()).0);
        assert!(!p.matches(&"some2.pre".into()).0);
        assert!(!p.matches(&"some2.pre.fix".into()).0);
        assert!(!p.matches(&"some.pre.fix2".into()).0);
        assert!(!p.matches(&"some.pre.fix2.test".into()).0);
        assert!(p.matches(&"some.pre2".into()).0);
        assert!(p.matches(&"some.pre2.fix".into()).0);
        assert!(p.matches(&"some.pre2.fix.test".into()).0);
        assert!(!p.matches(&"some2.pre2".into()).0);
        assert!(!p.matches(&"some2.pre2.fix".into()).0);
        assert!(!p.matches(&"some.pre2.fix2".into()).0);
        assert!(!p.matches(&"some.pre2.fix2.test".into()).0);
    }

    #[test]
    fn utest_empty_deny_path_pattern() {
        let p = DenyPathPattern::from("");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some.pre".into()).0);
    }
}

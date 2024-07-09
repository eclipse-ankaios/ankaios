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
                r.field_mask.iter().all(|path| {
                    let path = path.as_str().into();
                    (self.allow_read_state_rule.matches(&path)
                        || self.allow_read_write_state_rule.matches(&path))
                        && !(self.deny_read_state_rule.matches(&path)
                            || self.deny_read_write_state_rule.matches(&path))
                })
            }
            common::commands::RequestContent::UpdateStateRequest(r) => {
                r.update_mask.iter().all(|path| {
                    let path = path.as_str().into();
                    (self.allow_write_state_rule.matches(&path)
                        || self.allow_read_write_state_rule.matches(&path))
                        && !(self.deny_write_state_rule.matches(&path)
                            || self.deny_read_write_state_rule.matches(&path))
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
    fn matches(&self, path: &Path) -> bool {
        self.patterns.iter().any(|p| p.matches(path))
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

trait PathPattern {
    fn matches(&self, other: &Path) -> bool;
}

impl<T: PathPattern> PathPattern for Vec<T> {
    fn matches(&self, path: &Path) -> bool {
        self.iter().any(|r| r.matches(path))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct AllowPathPattern {
    sections: Vec<PathPatterSection>,
}

impl From<&str> for AllowPathPattern {
    fn from(value: &str) -> Self {
        Self {
            sections: value.split(PATH_SEPARATOR).map(Into::into).collect(),
        }
    }
}

impl PathPattern for AllowPathPattern {
    fn matches(&self, other: &Path) -> bool {
        if self.sections.len() > other.sections.len() {
            return false;
        }
        self.sections
            .iter()
            .zip(other.sections.iter())
            .all(|(a, b)| a.matches(b))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DenyPathPattern {
    sections: Vec<PathPatterSection>,
}

impl From<&str> for DenyPathPattern {
    fn from(value: &str) -> Self {
        Self {
            sections: value.split(PATH_SEPARATOR).map(Into::into).collect(),
        }
    }
}

impl PathPattern for DenyPathPattern {
    fn matches(&self, other: &Path) -> bool {
        !self
            .sections
            .iter()
            .zip(other.sections.iter())
            .any(|(a, b)| !a.matches(b))
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

        assert!(p.matches(&"some.pre.fix".into()));
        assert!(p.matches(&"some.pre.fix.test".into()));
        assert!(!p.matches(&"some.pre".into()));
        assert!(!p.matches(&"some.pre.fixtest".into()));
        assert!(!p.matches(&"some.pre.test".into()));
        assert!(!p.matches(&"some.pre.test.2".into()));
    }

    #[test]
    fn utest_allow_path_pattern_with_wildcard() {
        let p = AllowPathPattern::from("some.*.fix");
        println!("Patterh: {:?}", p);

        assert!(p.matches(&"some.pre.fix".into()));
        assert!(p.matches(&"some.pre.fix.test".into()));
        assert!(!p.matches(&"some.pre".into()));
        assert!(!p.matches(&"some.pre.fixtest".into()));
        assert!(!p.matches(&"some.pre.test".into()));
        assert!(!p.matches(&"some.pre.test.2".into()));
        assert!(p.matches(&"some.pre2.fix".into()));
        assert!(p.matches(&"some.pre2.fix.test".into()));
        assert!(!p.matches(&"some.pre2".into()));
        assert!(!p.matches(&"some.pre2.fixtest".into()));
        assert!(!p.matches(&"some.pre2.test".into()));
        assert!(!p.matches(&"some.pre2.test.2".into()));
    }

    #[test]
    fn utest_deny_path_pattern() {
        let p = DenyPathPattern::from("some.pre.fix");

        assert!(p.matches(&"".into()));
        assert!(p.matches(&"some.pre".into()));
        assert!(p.matches(&"some.pre.fix".into()));
        assert!(p.matches(&"some.pre.fix.test".into()));
        assert!(!p.matches(&"some2.pre".into()));
        assert!(!p.matches(&"some2.pre.fix".into()));
        assert!(!p.matches(&"some.pre.fix2".into()));
        assert!(!p.matches(&"some.pre.fix2.test".into()));
    }

    #[test]
    fn utest_deny_path_pattern_with_wildcard() {
        let p = DenyPathPattern::from("some.*.fix");

        assert!(p.matches(&"".into()));
        assert!(p.matches(&"some.pre".into()));
        assert!(p.matches(&"some.pre.fix".into()));
        assert!(p.matches(&"some.pre.fix.test".into()));
        assert!(!p.matches(&"some2.pre".into()));
        assert!(!p.matches(&"some2.pre.fix".into()));
        assert!(!p.matches(&"some.pre.fix2".into()));
        assert!(!p.matches(&"some.pre.fix2.test".into()));
        assert!(p.matches(&"some.pre2".into()));
        assert!(p.matches(&"some.pre2.fix".into()));
        assert!(p.matches(&"some.pre2.fix.test".into()));
        assert!(!p.matches(&"some2.pre2".into()));
        assert!(!p.matches(&"some2.pre2.fix".into()));
        assert!(!p.matches(&"some.pre2.fix2".into()));
        assert!(!p.matches(&"some.pre2.fix2.test".into()));
    }
}

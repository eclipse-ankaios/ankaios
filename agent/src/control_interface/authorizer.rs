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
    objects::{AccessRightsRule, ControlInterfaceAccess},
    PATH_SEPARATOR,
};

const WILDCARD_SYMBOL: &str = "*";

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Authorizer {
    allow_state_rule: Vec<Rule<AllowPathPattern>>,
    deny_state_rule: Vec<Rule<DenyPathPattern>>,
}

// #[cfg_attr(test, automock)]
impl Authorizer {
    #[cfg(test)]
    pub fn test_value(name: &str) -> Self {
        Self {
            allow_state_rule: vec![Rule {
                patterns: vec![AllowPathPattern {
                    sections: vec![PathPatterSection::String(name.into())],
                }],
            }],
            deny_state_rule: vec![],
        }
    }

    pub fn authorize(&self, request: &Request) -> bool {
        let mask = match &request.request_content {
            common::commands::RequestContent::CompleteStateRequest(r) => &r.field_mask,
            common::commands::RequestContent::UpdateStateRequest(r) => &r.update_mask,
        };
        mask.iter().all(|m| self.match_single_mask_path(m))
    }

    pub fn match_single_mask_path(&self, path: &str) -> bool {
        let path = Path::from(path);
        self.allow_state_rule.iter().any(|r| r.matches(&path))
            && !self.deny_state_rule.iter().any(|r| r.matches(&path))
    }
}

impl From<&ControlInterfaceAccess> for Authorizer {
    fn from(value: &ControlInterfaceAccess) -> Self {
        fn to_rule_list<T>(rule_list: &[AccessRightsRule]) -> Vec<Rule<T>>
        where
            T: for<'a> From<&'a str>,
            T: PathPattern,
        {
            rule_list
                .iter()
                .map(|access_rights| {
                    let AccessRightsRule::StateRule(state_rule) = access_rights;
                    let v: Vec<T> = state_rule
                        .filter_mask
                        .iter()
                        .map(|x| (**x).into())
                        .collect();
                    Rule { patterns: v }
                })
                .collect()
        }

        Self {
            allow_state_rule: to_rule_list(&value.allow_rules),
            deny_state_rule: to_rule_list(&value.deny_rules),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Rule<P: PathPattern> {
    patterns: Vec<P>,
}

impl<P: PathPattern> Rule<P> {
    pub fn matches(&self, path: &Path) -> bool {
        self.patterns.iter().any(|p| p.matches(path))
    }
}

// impl Rule {
//     pub fn applies(&self, filter_mask: String, target_value: String) -> bool {
//         self.filter_mask_applies(filter_mask) && self.targe_value_applies(target_value)
//     }

//     fn filter_mask_applies(&self, filter_mask: String) -> bool {
//         self.filter_masks.is_empty() || self.filter_masks.iter().any(|x| x.matches(&filter_mask))
//     }

//     fn targe_value_applies(&self, target_value: String) -> bool {
//         self.target_values.is_empty() || self.target_values.contains(&target_value)
//     }
// }

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

    // #[test]
    // fn utest_foobar() {
    //     let rule = Rule {
    //         filter_masks: vec![
    //             Path::from("workloads.nginx.agent"),
    //             Path::from("workloads.nginx.agent"),
    //         ],
    //         target_values: vec!["agent_A".into(), "agent_B".into()],
    //     };

    //     assert!(rule.applies(filter_mask, target_value))
    // }
}

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

use std::{fmt::Display, sync::Arc};

use super::path::Path;
use common::PATH_SEPARATOR;

const WILDCARD_SYMBOL: &str = "*";

pub type PathPatternMatchReason = String;

fn match_rule_with_path(rule: &impl PathPattern, other: &Path) -> (bool, PathPatternMatchReason) {
    // [impl->swdd~agent-authorizing-rules-without-segments-never-match~1]
    if rule.sections().is_empty() {
        return (false, "Empty filter masks in rules never match.".into());
    }

    for (a, b) in rule.sections().iter().zip(other.sections.iter()) {
        if !a.matches(b) {
            return (false, String::new());
        }
    }

    (
        true,
        rule.sections()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("."),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub enum PathPatternSection {
    Wildcard,
    String(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AllowPathPattern {
    sections: Vec<PathPatternSection>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DenyPathPattern {
    sections: Vec<PathPatternSection>,
}

pub trait PathPattern {
    fn sections(&self) -> &Vec<PathPatternSection>;
    fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason);
}

pub trait PathPatternMatcher {
    fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason);
}

// [impl->swdd~agent-authorizing-matching-allow-rules~1]
impl PathPattern for AllowPathPattern {
    fn sections(&self) -> &Vec<PathPatternSection> {
        &self.sections
    }

    fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason) {
        if self.sections.len() > other.sections.len()
            && self.sections.first() != Some(&PathPatternSection::Wildcard)
        {
            return (false, String::new());
        }

        match_rule_with_path(self, other)
    }
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

// [impl->swdd~agent-authorizing-matching-deny-rules~1]
impl PathPattern for DenyPathPattern {
    fn sections(&self) -> &Vec<PathPatternSection> {
        &self.sections
    }

    fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason) {
        match_rule_with_path(self, other)
    }
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

impl PathPatternSection {
    // [impl->swdd~agent-authorizing-matching-rules-elements~1]
    pub fn matches(&self, other: &String) -> bool {
        match self {
            PathPatternSection::Wildcard => true,
            PathPatternSection::String(self_string) => self_string == other,
        }
    }
}

impl From<&str> for PathPatternSection {
    fn from(value: &str) -> Self {
        if value == WILDCARD_SYMBOL {
            Self::Wildcard
        } else {
            Self::String(value.into())
        }
    }
}

impl Display for PathPatternSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathPatternSection::Wildcard => write!(f, "*"),
            PathPatternSection::String(s) => write!(f, "{}", s.clone()),
        }
    }
}

impl<T: PathPatternMatcher + std::fmt::Debug> PathPatternMatcher for Vec<Arc<T>> {
    fn matches(&self, path: &Path) -> (bool, PathPatternMatchReason) {
        for rule in self {
            if let (true, reason) = rule.matches(path) {
                return (true, reason);
            }
        }
        (false, String::new())
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
    use super::{AllowPathPattern, DenyPathPattern, PathPattern, PathPatternSection};

    impl From<Vec<PathPatternSection>> for AllowPathPattern {
        fn from(value: Vec<PathPatternSection>) -> Self {
            Self { sections: value }
        }
    }

    impl From<Vec<PathPatternSection>> for DenyPathPattern {
        fn from(value: Vec<PathPatternSection>) -> Self {
            Self { sections: value }
        }
    }

    // [utest->swdd~agent-authorizing-matching-allow-rules~1]
    #[test]
    fn utest_allow_path_pattern_sections() {
        let p = AllowPathPattern::from("some.pre.fix");
        let sections: Vec<PathPatternSection> = vec!["some".into(), "pre".into(), "fix".into()];

        assert_eq!(p.sections(), &sections);
        assert_eq!(p, AllowPathPattern::from(sections));
    }

    // [utest->swdd~agent-authorizing-matching-allow-rules~1]
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

    // [utest->swdd~agent-authorizing-matching-allow-rules~1]
    // [utest->swdd~agent-authorizing-matching-rules-elements~1]
    #[test]
    fn utest_allow_path_pattern_with_wildcard() {
        let p = AllowPathPattern::from("some.*.fix");

        assert!(p.matches(&"some.pre.fix".into()).0);
        assert!(p.matches(&"some.pre.fix.test".into()).0);
        assert!(!p.matches(&"some.pre".into()).0);
        assert!(!p.matches(&"some.pre.fixtest".into()).0);
        assert!(!p.matches(&"some.pre.test".into()).0);
        assert!(!p.matches(&"some.pre.test.2".into()).0);
        assert!(!p.matches(&"some.pre.bla.fix".into()).0);
        assert!(p.matches(&"some.pre2.fix".into()).0);
        assert!(p.matches(&"some.pre2.fix.test".into()).0);
        assert!(!p.matches(&"some.pre2".into()).0);
        assert!(!p.matches(&"some.pre2.fixtest".into()).0);
        assert!(!p.matches(&"some.pre2.test".into()).0);
        assert!(!p.matches(&"some.pre2.test.2".into()).0);
    }

    // [utest->swdd~agent-authorizing-matching-allow-rules~1]
    #[test]
    fn utest_allow_path_pattern_with_wildcard_only() {
        let p = AllowPathPattern::from("*");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some".into()).0);
        assert!(p.matches(&"some.pre.fix.test".into()).0);
    }

    // [utest->swdd~agent-authorizing-rules-without-segments-never-match~1]
    #[test]
    fn utest_empty_allow_path_pattern_does_not_match() {
        let p = AllowPathPattern::from("");

        assert!(!p.matches(&"".into()).0);
        assert!(!p.matches(&"some.pre".into()).0);
    }

    // [utest->swdd~agent-authorizing-matching-deny-rules~1]
    #[test]
    fn utest_deny_path_pattern_sections() {
        let p = DenyPathPattern::from("some.pre.fix");
        let sections: Vec<PathPatternSection> = vec!["some".into(), "pre".into(), "fix".into()];

        assert_eq!(p.sections(), &sections);
        assert_eq!(p, DenyPathPattern::from(sections));
    }

    // [utest->swdd~agent-authorizing-matching-deny-rules~1]
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

    // [utest->swdd~agent-authorizing-matching-deny-rules~1]
    // [utest->swdd~agent-authorizing-matching-rules-elements~1]
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
        assert!(!p.matches(&"some.pre.bla.fix".into()).0);
        assert!(p.matches(&"some.pre2".into()).0);
        assert!(p.matches(&"some.pre2.fix".into()).0);
        assert!(p.matches(&"some.pre2.fix.test".into()).0);
        assert!(!p.matches(&"some2.pre2".into()).0);
        assert!(!p.matches(&"some2.pre2.fix".into()).0);
        assert!(!p.matches(&"some.pre2.fix2".into()).0);
        assert!(!p.matches(&"some.pre2.fix2.test".into()).0);
    }

    // [utest->swdd~agent-authorizing-matching-deny-rules~1]
    #[test]
    fn utest_deny_path_pattern_with_wildcard_only() {
        let p = AllowPathPattern::from("*");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some".into()).0);
        assert!(p.matches(&"some.pre.fix.test".into()).0);
    }

    // [utest->swdd~agent-authorizing-rules-without-segments-never-match~1]
    #[test]
    fn utest_empty_deny_path_pattern_does_not_match() {
        let p = DenyPathPattern::from("");

        assert!(!p.matches(&"".into()).0);
        assert!(!p.matches(&"some.pre".into()).0);
    }

    #[test]
    fn utest_path_pattern_section() {
        let section = PathPatternSection::from("some");

        assert!(section.matches(&"some".into()));
        assert!(!section.matches(&"other".into()));

        let wildcard_section = PathPatternSection::from("*");

        assert!(wildcard_section.matches(&"any".into()));
        assert!(wildcard_section.matches(&"".into()));
    }
}

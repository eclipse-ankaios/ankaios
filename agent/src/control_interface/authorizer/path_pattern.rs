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

use std::fmt::Display;

use super::path::Path;
use common::PATH_SEPARATOR;

const WILDCARD_SYMBOL: &str = "*";

pub type PathPatternMatchReason = String;
pub trait PathPattern {
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
pub struct AllowPathPattern {
    sections: Vec<PathPatternSection>,
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
    // [impl->swdd~agent-authorizing-matching-allow-rules~1]
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
pub struct DenyPathPattern {
    sections: Vec<PathPatternSection>,
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
    // [impl->swdd~agent-authorizing-matching-deny-rules~1]
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
enum PathPatternSection {
    Wildcard,
    String(String),
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

impl PathPatternSection {
    // [impl->swdd~agent-authorizing-matching-rules-elements~1]
    pub fn matches(&self, other: &String) -> bool {
        match self {
            PathPatternSection::Wildcard => true,
            PathPatternSection::String(self_string) => self_string == other,
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
    #[derive(Debug)]
    struct MockPathPattern {
        path_returning_true: Path,
    }
    use super::super::path::Path;

    impl MockPathPattern {
        fn create(path: &str) -> Self {
            Self {
                path_returning_true: path.into(),
            }
        }
    }

    impl PathPattern for MockPathPattern {
        fn matches(&self, other: &Path) -> (bool, super::PathPatternMatchReason) {
            (
                other.sections == self.path_returning_true.sections,
                String::new(),
            )
        }
    }

    use crate::control_interface::authorizer::{AllowPathPattern, DenyPathPattern, PathPattern};

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
        assert!(p.matches(&"some.pre2.fix".into()).0);
        assert!(p.matches(&"some.pre2.fix.test".into()).0);
        assert!(!p.matches(&"some.pre2".into()).0);
        assert!(!p.matches(&"some.pre2.fixtest".into()).0);
        assert!(!p.matches(&"some.pre2.test".into()).0);
        assert!(!p.matches(&"some.pre2.test.2".into()).0);
    }

    // [utest->swdd~agent-authorizing-matching-allow-rules~1]
    #[test]
    fn utest_empty_allow_path_pattern() {
        let p = AllowPathPattern::from("");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some.pre".into()).0);
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
    fn utest_empty_deny_path_pattern() {
        let p = DenyPathPattern::from("");

        assert!(p.matches(&"".into()).0);
        assert!(p.matches(&"some.pre".into()).0);
    }

    #[test]
    fn utest_empty_vec_path_pattern() {
        let p = Vec::<MockPathPattern>::new();

        assert!(!p.matches(&"".into()).0);
    }

    #[test]
    fn utest_matches_one_in_vec_path_pattern() {
        let p = vec![
            MockPathPattern::create("some.path.1"),
            MockPathPattern::create("known.path"),
            MockPathPattern::create("some.path.2"),
        ];

        assert!(p.matches(&"known.path".into()).0);
    }

    #[test]
    fn utest_matches_none_in_vec_path_pattern() {
        let p = vec![
            MockPathPattern::create("some.path.1"),
            MockPathPattern::create("some.path.2"),
            MockPathPattern::create("some.path.3"),
        ];

        assert!(!p.matches(&"known.path".into()).0);
    }
}

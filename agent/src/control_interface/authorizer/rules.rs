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

// [impl->swdd~agent-authorizing-supported-rules~1]

use super::{
    path::Path,
    path_pattern::{PathPattern, PathPatternMatchReason, PathPatternMatcher},
};
use common::{objects::WILDCARD_SYMBOL, std_extensions::UnreachableOption};

#[derive(Clone, Debug, PartialEq)]
pub struct StateRule<P: PathPattern> {
    patterns: Vec<P>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogRule {
    patterns: Vec<String>,
}

impl<P: PathPattern> StateRule<P> {
    pub fn create(patterns: Vec<P>) -> Self {
        Self { patterns }
    }
}

impl<P: PathPattern> PathPatternMatcher for StateRule<P> {
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

impl LogRule {
    pub fn create(patterns: Vec<String>) -> Self {
        Self { patterns }
    }

    // [impl->swdd~agent-authorizing-log-rules-matches-request~1]
    pub fn matches(&self, workload_name: &str) -> bool {
        for pattern in &self.patterns {
            if pattern == workload_name || pattern == WILDCARD_SYMBOL {
                return true;
            } else if pattern.contains(WILDCARD_SYMBOL) {
                let wildcard_pos = pattern.find(WILDCARD_SYMBOL).unwrap_or_unreachable();
                let prefix = &pattern[..wildcard_pos];
                let suffix = &pattern[wildcard_pos + WILDCARD_SYMBOL.len()..];
                return workload_name.starts_with(prefix) && workload_name.ends_with(suffix);
            }
        }
        false
    }
}

impl From<Vec<String>> for LogRule {
    fn from(workload_names: Vec<String>) -> Self {
        Self::create(workload_names)
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

// [utest->swdd~agent-authorizing-supported-rules~1]
#[cfg(test)]
mod test {
    use super::{super::path::Path, LogRule, StateRule, WILDCARD_SYMBOL};
    use crate::control_interface::authorizer::path_pattern::{
        PathPattern, PathPatternMatchReason, PathPatternMatcher, PathPatternSection,
    };

    struct MockPathPattern {
        matching_path: Path,
        reason: String,
    }

    impl MockPathPattern {
        fn create(pattern: &str, reason: &str) -> Self {
            Self {
                matching_path: pattern.into(),
                reason: reason.into(),
            }
        }
    }

    impl PathPattern for MockPathPattern {
        fn matches(&self, other: &Path) -> (bool, PathPatternMatchReason) {
            if other.sections == self.matching_path.sections {
                (true, self.reason.clone())
            } else {
                (false, String::new())
            }
        }

        fn sections(&self) -> &Vec<PathPatternSection> {
            panic!("Not implemented for the mock");
        }
    }

    // [utest->swdd~agent-authorizing-log-rules-matches-request~1]
    #[test]
    fn utest_empty_state_rule() {
        let rule = StateRule::<MockPathPattern>::create(Vec::new());
        assert_eq!(rule.matches(&Path::from("some.path")), (false, "".into()));
    }

    // [utest->swdd~agent-authorizing-log-rules-matches-request~1]
    #[test]
    fn utest_matches_on_pattern_in_state_rule() {
        let rule = StateRule::create(vec![
            MockPathPattern::create("pattern.1", "reason1"),
            MockPathPattern::create("pattern.2", "reason2"),
            MockPathPattern::create("pattern.3", "reason3"),
        ]);
        assert_eq!(
            rule.matches(&Path::from("pattern.2")),
            (true, "reason2".into())
        );
    }

    // [utest->swdd~agent-authorizing-log-rules-matches-request~1]
    #[test]
    fn utest_matches_none_pattern_in_rule() {
        let rule = StateRule::create(vec![
            MockPathPattern::create("pattern.1", "reason1"),
            MockPathPattern::create("pattern.2", "reason2"),
            MockPathPattern::create("pattern.3", "reason3"),
        ]);
        assert_eq!(
            rule.matches(&Path::from("no.matching.pattern")),
            (false, "".into())
        );
    }

    // [utest->swdd~agent-authorizing-log-rules-matches-request~1]
    #[test]
    fn utest_log_rule_matches_no_wildcard() {
        let rule = LogRule::from(vec!["workload1".into(), "workload2".into()]);
        assert!(rule.matches("workload1"));
        assert!(rule.matches("workload2"));
        assert!(!rule.matches("workload3"));
    }

    // [utest->swdd~agent-authorizing-log-rules-matches-request~1]
    #[test]
    fn utest_log_rule_matches_wildcard() {
        let rule = LogRule::from(vec![WILDCARD_SYMBOL.into()]);
        assert!(rule.matches("any_workload"));
        assert!(rule.matches("name_matches"));

        let rule = LogRule::from(vec![format!("{}ef", WILDCARD_SYMBOL)]);
        assert!(rule.matches("abcdef"));
        assert!(rule.matches("ef"));
        assert!(!rule.matches("abcde"));

        let rule = LogRule::from(vec![format!("ab{}", WILDCARD_SYMBOL)]);
        assert!(rule.matches("abcdef"));
        assert!(rule.matches("ab"));
        assert!(!rule.matches("bcdef"));

        let rule = LogRule::from(vec![format!("ab{}ef", WILDCARD_SYMBOL)]);
        assert!(rule.matches("abcdef"));
        assert!(rule.matches("abef"));
        assert!(!rule.matches("abc"));
        assert!(!rule.matches("def"));
    }
}

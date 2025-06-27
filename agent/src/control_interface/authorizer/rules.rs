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

use super::{
    path::Path,
    path_pattern::{PathPattern, PathPatternMatchReason, PathPatternMatcher},
};

#[derive(Clone, Debug, PartialEq)]
pub struct StateRule<P: PathPattern> {
    patterns: Vec<P>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogRule {
    workload_names: Vec<String>,
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
    pub fn create(workload_names: Vec<String>) -> Self {
        Self { workload_names }
    }

    pub fn matches(&self, workload_name: &str) -> bool {
        self.workload_names.contains(&workload_name.to_string())
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

#[cfg(test)]
mod test {
    use super::{super::path::Path, LogRule, StateRule};
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

    #[test]
    fn utest_empty_state_rule() {
        let rule = StateRule::<MockPathPattern>::create(Vec::new());
        assert_eq!(rule.matches(&Path::from("some.path")), (false, "".into()));
    }

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

    #[test]
    fn utest_log_rule_matches() {
        let rule = LogRule::from(vec!["workload1".into(), "workload2".into()]);
        assert!(rule.matches("workload1"));
        assert!(rule.matches("workload2"));
        assert!(!rule.matches("workload3"));
    }
}

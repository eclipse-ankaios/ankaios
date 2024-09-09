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
    path_pattern::{PathPattern, PathPatternMatchReason},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Rule<P: PathPattern> {
    patterns: Vec<P>,
}

impl<P: PathPattern> Rule<P> {
    pub fn create(patterns: Vec<P>) -> Self {
        Self { patterns }
    }
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod test {
    use super::{super::path::Path, Rule};
    use crate::control_interface::authorizer::path_pattern::{PathPattern, PathPatternMatchReason};

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
    }

    #[test]
    fn utest_empty_rule() {
        let rule = Rule::<MockPathPattern>::create(Vec::new());
        assert_eq!(rule.matches(&Path::from("some.path")), (false, "".into()));
    }

    #[test]
    fn utest_matches_on_pattern_in_rule() {
        let rule = Rule::create(vec![
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
        let rule = Rule::create(vec![
            MockPathPattern::create("pattern.1", "reason1"),
            MockPathPattern::create("pattern.2", "reason2"),
            MockPathPattern::create("pattern.3", "reason3"),
        ]);
        assert_eq!(
            rule.matches(&Path::from("no.matching.pattern")),
            (false, "".into())
        );
    }
}

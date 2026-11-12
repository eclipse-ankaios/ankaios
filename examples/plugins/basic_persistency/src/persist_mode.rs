// Copyright (c) 2026 Red Hat LLC
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

//! Parsing of the `persist` workload tag.

use std::collections::HashMap;

/// Persistence mode requested via a workload's `persist` tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistMode {
    /// Persist the workload as soon as the server accepts it (in desired state),
    /// even if deployment is pending or failed.
    Always,
    /// Persist the workload only once it reaches the Running execution state.
    OnRunning,
}

impl PersistMode {
    pub fn from_tags(tags: &HashMap<String, String>) -> Option<Self> {
        let value = tags.get("persist")?;
        match value.to_uppercase().as_str() {
            "ALWAYS" => Some(PersistMode::Always),
            "ON_RUNNING" => Some(PersistMode::OnRunning),
            _ => {
                log::warn!(
                    "Invalid persist tag value '{value}'. Valid values: ALWAYS, ON_RUNNING"
                );
                None
            }
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
    use super::*;

    fn tags_with(value: &str) -> HashMap<String, String> {
        HashMap::from([("persist".to_owned(), value.to_owned())])
    }

    #[test]
    fn utest_from_tags_always_case_insensitive() {
        assert_eq!(PersistMode::from_tags(&tags_with("ALWAYS")), Some(PersistMode::Always));
        assert_eq!(PersistMode::from_tags(&tags_with("always")), Some(PersistMode::Always));
        assert_eq!(PersistMode::from_tags(&tags_with("Always")), Some(PersistMode::Always));
    }

    #[test]
    fn utest_from_tags_on_running_case_insensitive() {
        assert_eq!(PersistMode::from_tags(&tags_with("ON_RUNNING")), Some(PersistMode::OnRunning));
        assert_eq!(PersistMode::from_tags(&tags_with("on_running")), Some(PersistMode::OnRunning));
    }

    #[test]
    fn utest_from_tags_invalid_value() {
        assert_eq!(PersistMode::from_tags(&tags_with("SOMETIMES")), None);
    }

    #[test]
    fn utest_from_tags_missing_tag() {
        assert_eq!(PersistMode::from_tags(&HashMap::new()), None);
    }
}

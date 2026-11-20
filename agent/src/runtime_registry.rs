// Copyright (c) 2026 Elektrobit Automotive GmbH
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

use crate::runtime_connectors::OwnableRuntime;
use std::collections::HashMap;

/// Runtimes made available to an `AgentBuilder`, keyed by runtime name.
#[derive(Default)]
pub struct RuntimeRegistry(HashMap<String, Box<dyn OwnableRuntime>>);

impl RuntimeRegistry {
    pub fn register(&mut self, runtime: Box<dyn OwnableRuntime>) {
        self.0.insert(runtime.name(), runtime);
    }

    // [impl->swdd~agent-allows-enabled-runtimes~1]
    pub fn select(
        mut self,
        enabled_runtimes: &[String],
    ) -> HashMap<String, Box<dyn OwnableRuntime>> {
        let mut selected_runtimes = HashMap::new();

        for name in enabled_runtimes {
            match self.0.remove(name) {
                Some(runtime) => {
                    selected_runtimes.insert(name.clone(), runtime);
                }
                None => log::warn!("Configured runtime '{name}' is not supported."),
            }
        }

        log::debug!(
            "Runtimes to register: {:?}",
            selected_runtimes.keys().collect::<Vec<_>>()
        );
        selected_runtimes
    }
}

// [impl->swdd~agent-allows-enabled-runtimes~1]
impl From<RuntimeRegistry> for HashMap<String, Box<dyn OwnableRuntime>> {
    fn from(registry: RuntimeRegistry) -> Self {
        registry.0
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
    use super::{OwnableRuntime, RuntimeRegistry};
    use crate::runtime_connectors::test::MockRuntimeConnector;
    use std::collections::HashMap;

    fn registry_with_runtimes(names: &[&str]) -> RuntimeRegistry {
        RuntimeRegistry(
            names
                .iter()
                .map(|&name| {
                    (
                        name.to_string(),
                        Box::new(MockRuntimeConnector::default()) as Box<dyn OwnableRuntime>,
                    )
                })
                .collect(),
        )
    }

    // [utest->swdd~agent-allows-enabled-runtimes~1]
    #[test]
    fn utest_into_hashmap_contains_all_registered_runtimes() {
        let supported_runtimes = ["podman", "podman-kube", "containerd"];
        let registry = registry_with_runtimes(&supported_runtimes);

        let result: HashMap<String, Box<dyn OwnableRuntime>> = registry.into();

        assert_eq!(result.len(), supported_runtimes.len());
        for name in supported_runtimes {
            assert!(result.contains_key(name));
        }
    }

    #[test]
    fn utest_select_returns_empty_map_for_empty_configured_list() {
        let registry = registry_with_runtimes(&["podman", "podman-kube", "containerd"]);

        let result = registry.select(&[]);

        assert!(result.is_empty());
    }

    #[test]
    fn utest_select_selects_all_when_all_configured() {
        let supported_runtimes = ["podman", "podman-kube", "containerd"];
        let registry = registry_with_runtimes(&supported_runtimes);
        let configured: Vec<String> = supported_runtimes.iter().map(|s| s.to_string()).collect();

        let result = registry.select(&configured);

        assert_eq!(result.len(), supported_runtimes.len());
        for name in supported_runtimes {
            assert!(result.contains_key(name));
        }
    }

    #[test]
    fn utest_select_selects_single_configured_runtime() {
        let registry = registry_with_runtimes(&["podman", "podman-kube", "containerd"]);

        let result = registry.select(&["podman".to_string()]);

        assert_eq!(result.len(), 1);
        assert!(result.contains_key("podman"));
    }

    #[test]
    fn utest_select_returns_empty_map_for_unsupported_runtime() {
        let registry = registry_with_runtimes(&["podman", "podman-kube", "containerd"]);

        let result = registry.select(&["invalid_runtime".to_string()]);

        assert!(result.is_empty());
    }

    #[test]
    fn utest_select_ignores_unsupported_but_keeps_valid_ones() {
        let registry = registry_with_runtimes(&["podman", "podman-kube", "containerd"]);

        let result = registry.select(&[
            "podman".to_string(),
            "invalid_runtime".to_string(),
            "containerd".to_string(),
        ]);

        assert_eq!(result.len(), 2);
        assert!(result.contains_key("podman"));
        assert!(result.contains_key("containerd"));
    }
}

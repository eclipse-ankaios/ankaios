// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use common::objects::WorkloadSpec;

use super::nerdctl_cli::NerdctlRunConfig;

use super::containerd_runtime::CONTAINERD_RUNTIME_NAME;

#[derive(Debug, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerdRuntimeConfig {
    #[serde(default, alias = "generalOptions")]
    pub general_options: Vec<String>,
    #[serde(default, alias = "commandOptions")]
    pub command_options: Vec<String>,
    pub image: String,
    #[serde(default, alias = "commandArgs")]
    pub command_args: Vec<String>,
}

impl From<ContainerdRuntimeConfig> for NerdctlRunConfig {
    fn from(value: ContainerdRuntimeConfig) -> Self {
        NerdctlRunConfig {
            general_options: value.general_options,
            command_options: value.command_options,
            image: value.image,
            command_args: value.command_args,
        }
    }
}

impl TryFrom<&WorkloadSpec> for ContainerdRuntimeConfig {
    type Error = String;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        if CONTAINERD_RUNTIME_NAME != workload_spec.runtime {
            return Err(format!(
                "Received a spec for the wrong runtime: '{}'",
                workload_spec.runtime
            ));
        }
        match serde_yaml::from_str(workload_spec.runtime_config.as_str()) {
            Ok(workload_cfg) => Ok(workload_cfg),
            Err(e) => Err(e.to_string()),
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
    use common::objects::generate_test_workload_spec_with_param;

    use super::{ContainerdRuntimeConfig, NerdctlRunConfig};
    use crate::runtime_connectors::containerd::containerd_runtime::CONTAINERD_RUNTIME_NAME;

    const DIFFERENT_RUNTIME_NAME: &str = "different-runtime-name";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";

    #[test]
    fn utest_containerd_config_failure_missing_image() {
        let mut workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            CONTAINERD_RUNTIME_NAME.to_string(),
        );

        workload_spec.runtime_config = "something without an image".to_string();

        assert!(ContainerdRuntimeConfig::try_from(&workload_spec).is_err());
    }

    #[test]
    fn utest_containerd_config_failure_wrong_runtime() {
        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            DIFFERENT_RUNTIME_NAME.to_string(),
        );

        assert!(ContainerdRuntimeConfig::try_from(&workload_spec).is_err());
    }

    #[test]
    fn utest_containerd_config_success() {
        let mut workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            CONTAINERD_RUNTIME_NAME.to_string(),
        );

        let expected_containerd_config = ContainerdRuntimeConfig {
            general_options: vec!["--version".to_string()],
            command_options: vec!["--network=host".to_string()],
            image: "alpine:latest".to_string(),
            command_args: vec!["bash".to_string()],
        };

        workload_spec.runtime_config = "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n".to_string();

        assert_eq!(
            ContainerdRuntimeConfig::try_from(&workload_spec).unwrap(),
            expected_containerd_config
        );
    }

    #[test]
    fn utest_containerd_config_to_containerd_run_config() {
        let containerd_runtime_config = ContainerdRuntimeConfig {
            general_options: vec!["1".to_string(), "42".to_string()],
            command_options: vec!["--network=host".to_string(), "foo".to_string()],
            image: "alpine:latest".to_string(),
            command_args: vec!["bash".to_string(), "bar".to_string()],
        };

        let containerd_run_config = NerdctlRunConfig {
            general_options: vec!["1".to_string(), "42".to_string()],
            command_options: vec!["--network=host".to_string(), "foo".to_string()],
            image: "alpine:latest".to_string(),
            command_args: vec!["bash".to_string(), "bar".to_string()],
        };

        assert_eq!(
            NerdctlRunConfig::from(containerd_runtime_config),
            containerd_run_config
        );
    }
}

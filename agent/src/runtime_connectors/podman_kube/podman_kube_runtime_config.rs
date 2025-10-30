// Copyright (c) 2023 Elektrobit Automotive GmbH
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

use api::ank_base::WorkloadInternal;

use super::podman_kube_runtime::PODMAN_KUBE_RUNTIME_NAME;

// [impl->swdd~podman-kube-runtime-config~1]
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodmanKubeRuntimeConfig {
    #[serde(default, alias = "generalOptions")]
    pub general_options: Vec<String>,
    #[serde(default, alias = "playOptions")]
    pub play_options: Vec<String>,
    #[serde(default, alias = "downOptions")]
    pub down_options: Vec<String>,
    pub control_interface_target: Option<String>,
    pub manifest: String,
}

impl TryFrom<&WorkloadInternal> for PodmanKubeRuntimeConfig {
    type Error = String;
    fn try_from(workload_spec: &WorkloadInternal) -> Result<Self, Self::Error> {
        if PODMAN_KUBE_RUNTIME_NAME != workload_spec.runtime {
            return Err(format!(
                "Received a spec for the wrong runtime: '{}'",
                workload_spec.runtime
            ));
        }

        // [impl->swdd~podman-kube-rejects-workload-files~1]
        if !workload_spec.files.is_empty() {
            return Err(format!(
                "Workload files are not supported for runtime {PODMAN_KUBE_RUNTIME_NAME}. Use ConfigMaps instead."
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
    use api::test_utils::{
        generate_test_workload_files, generate_test_workload_with_files,
        generate_test_workload_with_param,
    };

    use super::{PODMAN_KUBE_RUNTIME_NAME, PodmanKubeRuntimeConfig};

    const DIFFERENT_RUNTIME_NAME: &str = "different-runtime-name";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const MANIFEST_CONTENT: &str = "kube, man";

    #[tokio::test]
    async fn utest_podman_kube_config_failure_missing_manifest() {
        let workload_spec = generate_test_workload_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
        );

        assert!(PodmanKubeRuntimeConfig::try_from(&workload_spec).is_err());
    }

    #[tokio::test]
    async fn utest_podman_kube_config_failure_wrong_runtime() {
        let workload_spec = generate_test_workload_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            DIFFERENT_RUNTIME_NAME.to_string(),
        );

        assert!(PodmanKubeRuntimeConfig::try_from(&workload_spec).is_err());
    }

    // [utest->swdd~podman-kube-rejects-workload-files~1]
    #[tokio::test]
    async fn utest_podman_kube_config_failure_unsupported_files_field() {
        let workload_spec_with_files = generate_test_workload_with_files(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            DIFFERENT_RUNTIME_NAME.to_string(),
            generate_test_workload_files(),
        );

        assert!(PodmanKubeRuntimeConfig::try_from(&workload_spec_with_files).is_err());
    }

    #[tokio::test]
    async fn utest_podman_kube_config_success() {
        let mut workload_spec = generate_test_workload_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
        );

        workload_spec.runtime_config = format!("manifest: {MANIFEST_CONTENT}");

        assert!(
            PodmanKubeRuntimeConfig::try_from(&workload_spec)
                .unwrap()
                .manifest
                == *MANIFEST_CONTENT
        );
    }
}

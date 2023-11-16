use common::objects::WorkloadSpec;

use super::podman_kube_runtime::PODMAN_KUBE_RUNTIME_NAME;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodmanKubeRuntimeConfig {
    #[serde(default, alias = "generalOptions")]
    pub general_options: Vec<String>,
    #[serde(default, alias = "playOptions")]
    pub play_options: Vec<String>,
    #[serde(default, alias = "downOptions")]
    pub down_options: Vec<String>,
    pub manifest: String,
}

impl TryFrom<&WorkloadSpec> for PodmanKubeRuntimeConfig {
    type Error = String;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        if PODMAN_KUBE_RUNTIME_NAME != workload_spec.runtime {
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
    use common::test_utils::generate_test_workload_spec_with_param;

    use super::{PodmanKubeRuntimeConfig, PODMAN_KUBE_RUNTIME_NAME};

    const DIFFERENT_RUNTIME_NAME: &str = "different-runtime-name";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const MANIFEST_CONTENT: &str = "kube, man";

    #[tokio::test]
    async fn utest_podman_kube_config_failure_missing_manifest() {
        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
        );

        assert!(PodmanKubeRuntimeConfig::try_from(&workload_spec).is_err());
    }

    #[tokio::test]
    async fn utest_podman_kube_config_failure_wrong_runtime() {
        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            DIFFERENT_RUNTIME_NAME.to_string(),
        );

        assert!(PodmanKubeRuntimeConfig::try_from(&workload_spec).is_err());
    }

    #[tokio::test]
    async fn utest_podman_kube_config_success() {
        let mut workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_KUBE_RUNTIME_NAME.to_string(),
        );

        workload_spec.runtime_config = format!("manifest: {}", MANIFEST_CONTENT);

        assert!(
            PodmanKubeRuntimeConfig::try_from(&workload_spec)
                .unwrap()
                .manifest
                == *MANIFEST_CONTENT
        );
    }
}

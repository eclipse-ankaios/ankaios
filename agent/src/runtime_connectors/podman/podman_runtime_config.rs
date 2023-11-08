use common::objects::WorkloadSpec;

use super::podman_runtime::PODMAN_RUNTIME_NAME;

#[derive(Debug, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodmanRuntimeConfig {
    #[serde(default, alias = "generalOptions")]
    pub general_options: Vec<String>,
    #[serde(default, alias = "commandOptions")]
    pub command_options: Vec<String>,
    pub image: String,
    #[serde(default, alias = "commandArgs")]
    pub command_args: Vec<String>,
}

#[derive(Debug)]
pub struct TryFromWorkloadSpecError(String);

impl TryFrom<&WorkloadSpec> for PodmanRuntimeConfig {
    type Error = TryFromWorkloadSpecError;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        if PODMAN_RUNTIME_NAME != workload_spec.runtime {
            return Err(TryFromWorkloadSpecError(format!(
                "Received a spec for the wrong runtime: '{}'",
                workload_spec.runtime
            )));
        }
        match serde_yaml::from_str(workload_spec.runtime_config.as_str()) {
            Ok(workload_cfg) => Ok(workload_cfg),
            Err(e) => Err(TryFromWorkloadSpecError(e.to_string())),
        }
    }
}

impl From<TryFromWorkloadSpecError> for String {
    fn from(value: TryFromWorkloadSpecError) -> Self {
        value.0
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

    use crate::runtime_connectors::podman::{podman_runtime::PODMAN_RUNTIME_NAME, PodmanRuntimeConfig};

    const DIFFERENT_RUNTIME_NAME: &str = "different-runtime-name";
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";

    #[tokio::test]
    async fn utest_podman_config_failure_missing_image() {
        let mut workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );

        workload_spec.runtime_config = "something without an image".to_string();

        assert!(PodmanRuntimeConfig::try_from(&workload_spec).is_err());
    }

    #[tokio::test]
    async fn utest_podman_config_failure_wrong_runtime() {
        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            DIFFERENT_RUNTIME_NAME.to_string(),
        );

        assert!(PodmanRuntimeConfig::try_from(&workload_spec).is_err());
    }

    #[tokio::test]
    async fn utest_podman_config_success() {
        let mut workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_1_NAME.to_string(),
            PODMAN_RUNTIME_NAME.to_string(),
        );

        let expected_podman_config = PodmanRuntimeConfig {
            general_options: vec!["--version".to_string()],
            command_options: vec!["--network=host".to_string()],
            image: "alpine:latest".to_string(),
            command_args: vec!["bash".to_string()],
        };

        workload_spec.runtime_config = "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n".to_string();

        assert_eq!(
            PodmanRuntimeConfig::try_from(&workload_spec).unwrap(),
            expected_podman_config
        );
    }
}

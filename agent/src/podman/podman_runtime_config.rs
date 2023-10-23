use common::objects::WorkloadSpec;

use crate::runtime::RuntimeError;

#[derive(Debug, serde::Deserialize)]
pub struct PodmanRuntimeConfigCli {
    #[serde(alias = "generalOptions")]
    pub general_options: Option<Vec<String>>,
    #[serde(alias = "commandOptions")]
    pub command_options: Option<Vec<String>>,
    pub image: String,
    #[serde(alias = "commandArgs")]
    pub command_args: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct TryFromWorkloadSpecError(String);

impl TryFrom<&WorkloadSpec> for PodmanRuntimeConfigCli {
    type Error = TryFromWorkloadSpecError;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        match serde_yaml::from_str(workload_spec.runtime_config.as_str()) {
            Ok(workload_cfg) => Ok(workload_cfg),
            Err(e) => Err(TryFromWorkloadSpecError(e.to_string())),
        }
    }
}

impl From<TryFromWorkloadSpecError> for RuntimeError {
    fn from(value: TryFromWorkloadSpecError) -> Self {
        RuntimeError::Create(value.0)
    }
}

impl From<TryFromWorkloadSpecError> for String {
    fn from(value: TryFromWorkloadSpecError) -> Self {
        value.0
    }
}

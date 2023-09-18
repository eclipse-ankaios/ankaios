use std::collections::HashMap;

use common::objects::WorkloadSpec;

use crate::workload_trait::WorkloadError;

#[derive(Debug, serde::Deserialize)]
pub struct PodmanRuntimeConfig {
    pub image: String,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub ports: Vec<Mapping>,
    #[serde(default)]
    pub remove: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    pub container_port: String,
    pub host_port: String,
}

impl PodmanRuntimeConfig {
    pub fn get_entrypoint(&self) -> Vec<String> {
        self.command.clone()
    }

    pub fn get_command(&self) -> Vec<String> {
        self.args.clone()
    }
}

#[derive(Debug)]
pub struct TryFromWorkloadSpecError(String);

impl TryFrom<&WorkloadSpec> for PodmanRuntimeConfig {
    type Error = TryFromWorkloadSpecError;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        match serde_yaml::from_str(workload_spec.workload.runtime_config.as_str()) {
            Ok(workload_cfg) => Ok(workload_cfg),
            Err(e) => Err(TryFromWorkloadSpecError(e.to_string())),
        }
    }
}

impl From<TryFromWorkloadSpecError> for WorkloadError {
    fn from(value: TryFromWorkloadSpecError) -> Self {
        WorkloadError::StartError(value.0)
    }
}

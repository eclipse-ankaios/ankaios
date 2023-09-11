use std::collections::HashMap;

use common::objects::WorkloadSpec;
use podman_api::models::PortMapping;

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
    container_port: String,
    host_port: String,
}

pub fn convert_to_port_mapping(item: &[Mapping]) -> Vec<PortMapping> {
    item.iter()
        .map(|value| PortMapping {
            container_port: value.container_port.parse::<u16>().ok(),
            host_port: value.host_port.parse::<u16>().ok(),
            host_ip: None,
            protocol: None,
            range: None,
        })
        .collect()
}

impl PodmanRuntimeConfig {
    pub fn get_command_with_args(&self) -> Vec<String> {
        let mut command = vec![];
        command.extend(self.command.iter().cloned());
        command.extend(self.args.iter().cloned());
        command
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

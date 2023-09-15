use std::collections::HashMap;

use common::objects::WorkloadSpec;
use podman_api::models::PortMapping;

use crate::workload_trait::WorkloadError;

#[derive(Debug, serde::Deserialize)]
pub struct PodmanRuntimeConfig {
    pub image: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub ports: Vec<Mapping>,
    #[serde(default)]
    pub remove: bool,
    #[serde(default)]
    pub mounts: Vec<Mount>,
}

impl PodmanRuntimeConfig {
    pub fn get_command_with_args(&self) -> Vec<String> {
        let mut command = vec![];
        command.extend(self.command.iter().cloned());
        command.extend(self.args.iter().cloned());
        command
    }
}

impl TryFrom<&WorkloadSpec> for PodmanRuntimeConfig {
    type Error = TryFromWorkloadSpecError;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        match serde_yaml::from_str(workload_spec.workload.runtime_config.as_str()) {
            Ok(workload_cfg) => Ok(workload_cfg),
            Err(e) => Err(TryFromWorkloadSpecError(e.to_string())),
        }
    }
}

#[derive(Debug)]
pub struct TryFromWorkloadSpecError(String);

impl From<TryFromWorkloadSpecError> for WorkloadError {
    fn from(value: TryFromWorkloadSpecError) -> Self {
        WorkloadError::StartError(value.0)
    }
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

#[derive(Debug, serde::Deserialize)]
pub struct Mount {
    #[serde(default)]
    #[serde(alias = "dst")]
    pub destination: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    #[serde(alias = "src")]
    pub source: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    pub _type: Option<String>,
    #[serde(default)]
    pub uid_mappings: Option<Vec<IdMap>>,
    #[serde(default)]
    pub gid_mappings: Option<Vec<IdMap>>,
}

impl From<Mount> for podman_api::models::ContainerMount {
    fn from(value: Mount) -> podman_api::models::ContainerMount {
        Self {
            destination: value.destination,
            options: value.options,
            source: value.source,
            _type: value._type,
            uid_mappings: value
                .uid_mappings
                .map(|v| v.into_iter().map(|x| x.into()).collect()),
            gid_mappings: value
                .gid_mappings
                .map(|v| v.into_iter().map(|x| x.into()).collect()),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct IdMap {
    #[serde(default)]
    pub container_id: Option<i64>,
    #[serde(default)]
    pub host_id: Option<i64>,
    #[serde(default)]
    pub size: Option<i64>,
}

impl From<IdMap> for podman_api::models::IdMap {
    fn from(value: IdMap) -> Self {
        Self {
            container_id: value.container_id,
            host_id: value.host_id,
            size: value.size,
        }
    }
}

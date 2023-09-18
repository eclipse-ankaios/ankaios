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
    #[serde(default)]
    pub mounts: Vec<Mount>,
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

#[derive(Debug, serde::Deserialize)]
pub struct Mount {
    #[serde(default)]
    #[serde(alias = "dst")]
    pub destination: String,
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
            destination: Some(value.destination),
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

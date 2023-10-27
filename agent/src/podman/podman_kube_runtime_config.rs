use common::objects::WorkloadSpec;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodmanKubeRuntimeConfig {
    #[serde(default)]
    pub play_options: Vec<String>,
    #[serde(default)]
    pub down_options: Vec<String>,
    pub manifest: String,
}

impl TryFrom<&WorkloadSpec> for PodmanKubeRuntimeConfig {
    type Error = String;
    fn try_from(workload_spec: &WorkloadSpec) -> Result<Self, Self::Error> {
        match serde_yaml::from_str(workload_spec.runtime_config.as_str()) {
            Ok(workload_cfg) => Ok(workload_cfg),
            Err(e) => Err(e.to_string()),
        }
    }
}

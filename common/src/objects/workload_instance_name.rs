use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use api::proto;
use serde::{Deserialize, Serialize};

use super::{StoredWorkloadSpec, WorkloadSpec};

pub trait ConfigHash {
    fn hash_config(&self) -> String;
}

impl ConfigHash for String {
    fn hash_config(&self) -> String {
        sha256::digest(self.as_str())
    }
}

impl ConfigHash for WorkloadSpec {
    fn hash_config(&self) -> String {
        self.runtime_config.hash_config()
    }
}

pub enum InstanceNameParts {
    WorkloadName = 0,
    ConfigHash = 1,
    AgentName = 2,
}

// This could be std::mem::variant_count::<WorkloadExecutionInstanceParts>(),
// but the function is still in only nightly ...
pub const INSTANCE_NAME_PARTS_COUNT: usize = 3;
pub const INSTANCE_NAME_SEPARATOR: &str = ".";

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkloadInstanceName {
    agent_name: String,
    workload_name: String,
    id: String,
}

impl From<(String, &StoredWorkloadSpec)> for WorkloadInstanceName {
    fn from((workload_name, stored_spec): (String, &StoredWorkloadSpec)) -> Self {
        WorkloadInstanceName {
            workload_name,
            agent_name: stored_spec.agent.clone(),
            id: stored_spec.runtime_config.hash_config(),
        }
    }
}

impl From<proto::WorkloadInstanceName> for WorkloadInstanceName {
    fn from(item: proto::WorkloadInstanceName) -> Self {
        WorkloadInstanceName {
            workload_name: item.workload_name,
            agent_name: item.agent_name,
            id: item.id,
        }
    }
}

impl From<WorkloadInstanceName> for proto::WorkloadInstanceName {
    fn from(item: WorkloadInstanceName) -> Self {
        proto::WorkloadInstanceName {
            workload_name: item.workload_name,
            agent_name: item.agent_name,
            id: item.id,
        }
    }
}

impl WorkloadInstanceName {
    pub fn new(input: &str) -> Option<WorkloadInstanceName> {
        input.try_into().ok()
    }

    pub fn workload_name(&self) -> &str {
        &self.workload_name
    }

    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    pub fn pipes_folder_name(&self, base_path: &Path) -> PathBuf {
        base_path.join(format!(
            "{}{}{}",
            self.workload_name, INSTANCE_NAME_SEPARATOR, self.id
        ))
    }
}

// [impl->swdd~common-workload-execution-instance-naming~1]
impl Display for WorkloadInstanceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}{}{}",
            self.workload_name,
            INSTANCE_NAME_SEPARATOR,
            self.id,
            INSTANCE_NAME_SEPARATOR,
            self.agent_name
        )
    }
}

impl TryFrom<String> for WorkloadInstanceName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        (*value).try_into()
    }
}

impl TryFrom<&str> for WorkloadInstanceName {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value_parts: Vec<&str> = value.split(INSTANCE_NAME_SEPARATOR).collect();
        if value_parts.len() != INSTANCE_NAME_PARTS_COUNT {
            return Err(format!("Could not convert '{}' to a WorkloadInstanceName, as it consist of {} instead of 3.", value, value_parts.len()));
        }

        Ok(WorkloadInstanceName {
            workload_name: value_parts[InstanceNameParts::WorkloadName as usize].to_string(),
            id: value_parts[InstanceNameParts::ConfigHash as usize].to_string(),
            agent_name: value_parts[InstanceNameParts::AgentName as usize].to_string(),
        })
    }
}

impl WorkloadInstanceName {
    pub fn builder() -> WorkloadInstanceNameBuilder {
        Default::default()
    }
}

#[derive(Default)]
pub struct WorkloadInstanceNameBuilder {
    agent_name: String,
    workload_name: String,
    hash: String,
}

impl WorkloadInstanceNameBuilder {
    pub fn agent_name(mut self, agent_name: impl Into<String>) -> Self {
        self.agent_name = agent_name.into();
        self
    }

    pub fn workload_name(mut self, workload_name: impl Into<String>) -> Self {
        self.workload_name = workload_name.into();
        self
    }

    pub fn config(mut self, config: &impl ConfigHash) -> Self {
        self.hash = config.hash_config();
        self
    }

    pub fn build(self) -> WorkloadInstanceName {
        WorkloadInstanceName {
            agent_name: self.agent_name,
            workload_name: self.workload_name,
            id: self.hash,
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
    use super::WorkloadInstanceName;

    const AGENT_NAME: &str = "agent";
    const WORKLOAD_NAME: &str = "workload";
    const CONFIG: &str = "config";
    const EXPECTED_HASH: &str = "b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910";

    // [utest->swdd~common-workload-execution-instance-naming~1]
    #[test]
    fn utest_workload_execution_instance_name_builder() {
        let name = WorkloadInstanceName::builder()
            .agent_name(AGENT_NAME)
            .workload_name(WORKLOAD_NAME)
            .config(&String::from(CONFIG))
            .build();

        assert_eq!(name.workload_name(), WORKLOAD_NAME);
        assert_eq!(name.id, EXPECTED_HASH);
        assert_eq!(
            name.to_string(),
            format!("{WORKLOAD_NAME}.{EXPECTED_HASH}.{AGENT_NAME}")
        )
    }
}

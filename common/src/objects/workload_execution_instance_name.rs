use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use super::WorkloadSpec;

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

pub trait WorkloadInstanceName {
    fn instance_name(&self) -> WorkloadExecutionInstanceName;
}

impl WorkloadInstanceName for WorkloadSpec {
    fn instance_name(&self) -> WorkloadExecutionInstanceName {
        WorkloadExecutionInstanceName::builder()
            .agent_name(self.agent.clone())
            .workload_name(self.name.clone())
            .config(&self.runtime_config)
            .build()
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

#[derive(Clone, Debug, PartialEq)]
pub struct WorkloadExecutionInstanceName {
    agent_name: String,
    workload_name: String,
    hash: String,
}

impl WorkloadExecutionInstanceName {
    pub fn new(input: &str) -> Option<WorkloadExecutionInstanceName> {
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
            self.workload_name, INSTANCE_NAME_SEPARATOR, self.hash
        ))
    }
}

// [impl->swdd~common-workload-execution-instance-naming~1]
impl Display for WorkloadExecutionInstanceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}{}{}{}",
            self.workload_name,
            INSTANCE_NAME_SEPARATOR,
            self.hash,
            INSTANCE_NAME_SEPARATOR,
            self.agent_name
        )
    }
}

impl TryFrom<String> for WorkloadExecutionInstanceName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        (*value).try_into()
    }
}

impl TryFrom<&str> for WorkloadExecutionInstanceName {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value_parts: Vec<&str> = value.split(INSTANCE_NAME_SEPARATOR).collect();
        if value_parts.len() != INSTANCE_NAME_PARTS_COUNT {
            return Err(format!("Could not convert '{}' to a WorkloadExecutionInstanceName, as it consist of {} instead of 3.", value, value_parts.len()));
        }

        Ok(WorkloadExecutionInstanceName {
            workload_name: value_parts[InstanceNameParts::WorkloadName as usize].to_string(),
            hash: value_parts[InstanceNameParts::ConfigHash as usize].to_string(),
            agent_name: value_parts[InstanceNameParts::AgentName as usize].to_string(),
        })
    }
}

impl WorkloadExecutionInstanceName {
    pub fn builder() -> WorkloadExecutionInstanceNameBuilder {
        Default::default()
    }
}

#[derive(Default)]
pub struct WorkloadExecutionInstanceNameBuilder {
    agent_name: String,
    workload_name: String,
    hash: String,
}

impl WorkloadExecutionInstanceNameBuilder {
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

    pub fn build(self) -> WorkloadExecutionInstanceName {
        WorkloadExecutionInstanceName {
            agent_name: self.agent_name,
            workload_name: self.workload_name,
            hash: self.hash,
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
    use super::WorkloadExecutionInstanceName;

    const AGENT_NAME: &str = "agent";
    const WORKLOAD_NAME: &str = "workload";
    const CONFIG: &str = "config";
    const EXPECTED_HASH: &str = "b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910";

    // [utest->swdd~common-workload-execution-instance-naming~1]
    #[test]
    fn utest_workload_execution_instance_name_builder() {
        let name = WorkloadExecutionInstanceName::builder()
            .agent_name(AGENT_NAME)
            .workload_name(WORKLOAD_NAME)
            .config(&String::from(CONFIG))
            .build();

        assert_eq!(name.workload_name(), WORKLOAD_NAME);
        assert_eq!(name.hash, EXPECTED_HASH);
        assert_eq!(
            name.to_string(),
            format!("{WORKLOAD_NAME}.{EXPECTED_HASH}.{AGENT_NAME}")
        )
    }
}

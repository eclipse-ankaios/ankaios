use std::fmt::Display;

use crate::objects::workload_execution_instance_name::INSTANCE_NAME_SEPARATOR;

pub struct AgentName(String);

impl AgentName {
    pub fn get(&self) -> &str {
        &self.0
    }

    // [impl->swdd~agent-adapter-start-finds-existing-workloads~1]
    pub fn get_filter_regex(&self) -> String {
        format!("[{}]{}$", INSTANCE_NAME_SEPARATOR, self.0)
    }

    pub fn get_filter_suffix(&self) -> String {
        format!("{}{}", INSTANCE_NAME_SEPARATOR, self.0)
    }
}

impl From<String> for AgentName {
    fn from(value: String) -> Self {
        AgentName(value)
    }
}

impl Display for AgentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Could not create workload: '{}'", self.0)
    }
}

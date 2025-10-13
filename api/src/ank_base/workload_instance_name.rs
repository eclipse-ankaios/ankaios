// Copyright (c) 2025 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use crate::ank_base::WorkloadInstanceNameInternal;
// use crate::ank_base::{WorkloadInstanceNameInternal, WorkloadInternal};

// This could be std::mem::variant_count::<WorkloadExecutionInstanceParts>(),
// but the function is still in only nightly ...
pub const INSTANCE_NAME_PARTS_COUNT: usize = 3;
pub const INSTANCE_NAME_SEPARATOR: &str = ".";

pub enum InstanceNameParts {
    WorkloadName = 0,
    ConfigHash = 1,
    AgentName = 2,
}

#[derive(Default)]
pub struct WorkloadInstanceNameBuilder {
    agent_name: String,
    workload_name: String,
    hash: String,
}

pub trait ConfigHash {
    fn hash_config(&self) -> String;
}

impl ConfigHash for String {
    fn hash_config(&self) -> String {
        sha256::digest(self.as_str())
    }
}

// impl ConfigHash for WorkloadInternal {
//     fn hash_config(&self) -> String {
//         self.runtime_config.as_ref().unwrap().hash_config()
//     }
// }

impl WorkloadInstanceNameInternal {
    pub fn new(
        agent_name: impl Into<String>,
        workload_name: impl Into<String>,
        id: impl Into<String>,
    ) -> WorkloadInstanceNameInternal {
        WorkloadInstanceNameInternal {
            workload_name: workload_name.into(),
            agent_name: agent_name.into(),
            id: id.into(),
        }
    }

    pub fn workload_name(&self) -> &str {
        &self.workload_name
    }

    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn pipes_folder_name(&self, base_path: &Path) -> PathBuf {
        base_path.join(format!(
            "{}{}{}",
            self.workload_name, INSTANCE_NAME_SEPARATOR, self.id
        ))
    }

    pub fn builder() -> WorkloadInstanceNameBuilder {
        WorkloadInstanceNameBuilder::default()
    }
}

impl TryFrom<String> for WorkloadInstanceNameInternal {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        (*value).try_into()
    }
}

impl TryFrom<&str> for WorkloadInstanceNameInternal {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value_parts: Vec<&str> = value.split(INSTANCE_NAME_SEPARATOR).collect();
        if value_parts.len() != INSTANCE_NAME_PARTS_COUNT {
            return Err(format!(
                "Could not convert '{}' to a WorkloadInstanceNameInternal, as it consist of {} instead of 3.",
                value,
                value_parts.len()
            ));
        }

        Ok(WorkloadInstanceNameInternal {
            workload_name: value_parts[InstanceNameParts::WorkloadName as usize].to_string(),
            id: value_parts[InstanceNameParts::ConfigHash as usize].to_string(),
            agent_name: value_parts[InstanceNameParts::AgentName as usize].to_string(),
        })
    }
}

impl Display for WorkloadInstanceNameInternal {
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

impl WorkloadInstanceNameBuilder {
    pub fn agent_name(mut self, agent_name: impl Into<String>) -> Self {
        self.agent_name = agent_name.into();
        self
    }

    pub fn workload_name(mut self, workload_name: impl Into<String>) -> Self {
        self.workload_name = workload_name.into();
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.hash = id.into();
        self
    }

    pub fn config(mut self, config: &impl ConfigHash) -> Self {
        self.hash = config.hash_config();
        self
    }

    pub fn build(self) -> WorkloadInstanceNameInternal {
        WorkloadInstanceNameInternal {
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

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_instance_name(
    name: impl Into<String>,
) -> WorkloadInstanceNameInternal {
    WorkloadInstanceNameInternal::builder()
        .agent_name("agent_name")
        .workload_name(name)
        .config(&String::from("my cool config"))
        .build()
}

#[cfg(test)]
mod tests {
    use super::WorkloadInstanceNameInternal;

    const AGENT_NAME: &str = "agent";
    const WORKLOAD_NAME: &str = "workload";
    const CONFIG: &str = "config";
    const EXPECTED_HASH: &str = "b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910";

    // [utest->swdd~common-workload-execution-instance-naming~1]
    #[test]
    fn utest_workload_execution_instance_name_builder() {
        let name = WorkloadInstanceNameInternal::builder()
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

// Copyright (c) 2023 Elektrobit Automotive GmbH
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

use common::objects::{ExecutionStateEnum, WorkloadState};
use std::collections::HashMap;

type WorkloadStates = HashMap<String, common::objects::ExecutionState>;
type AgentWorkloadStates = HashMap<String, WorkloadStates>;

pub struct ParameterStorage {
    states_storage: AgentWorkloadStates,
}

impl ParameterStorage {
    pub fn new() -> Self {
        Self {
            states_storage: HashMap::new(),
        }
    }

    // Currently used only in tests. Update tests if you have another "public getter".
    #[allow(dead_code)]
    pub fn get_workload_states(&self, agent_name: &String) -> Option<&WorkloadStates> {
        self.states_storage.get(agent_name)
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadState) {
        let agent_workloads = self
            .states_storage
            .entry(workload_state.instance_name.agent_name().to_owned())
            .or_default();

        if workload_state.execution_state.state != ExecutionStateEnum::Removed {
            agent_workloads.insert(
                workload_state.instance_name.workload_name().to_owned(),
                workload_state.execution_state,
            );
        } else {
            agent_workloads.remove(workload_state.instance_name.workload_name());
        }
        self.remove_empty_hash_maps();
    }

    fn remove_empty_hash_maps(&mut self) {
        self.states_storage
            .retain(|_, workload_states| !workload_states.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use crate::parameter_storage::ParameterStorage;
    use common::objects::{ExecutionState, WorkloadState};

    #[test]
    fn utest_update_storage_empty_storage() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let test_update = WorkloadState {
            workload_name: String::from("test_workload"),
            agent_name: String::from("test_agent"),
            execution_state: ExecutionState::ExecRunning,
        };
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let agent_workloads = storage.states_storage.get(&test_update.agent_name).unwrap();
        assert_eq!(agent_workloads.len(), 1);

        let storage_record = agent_workloads.get(&test_update.workload_name).unwrap();
        assert_eq!(storage_record.to_owned(), ExecutionState::ExecRunning);

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionState::ExecRemoved;
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_update_record() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let test_update = WorkloadState {
            workload_name: String::from("test_workload"),
            agent_name: String::from("test_agent"),
            execution_state: ExecutionState::ExecRunning,
        };
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let mut agent_workloads = storage.states_storage.get(&test_update.agent_name).unwrap();
        assert_eq!(agent_workloads.len(), 1);

        let storage_record = agent_workloads.get(&test_update.workload_name).unwrap();
        assert_eq!(storage_record.to_owned(), ExecutionState::ExecRunning);

        let mut update_record = test_update.clone();
        update_record.execution_state = ExecutionState::ExecSucceeded;

        storage.update_workload_state(update_record.clone());

        assert_eq!(storage.states_storage.len(), 1);

        agent_workloads = storage.states_storage.get(&test_update.agent_name).unwrap();
        let updated_record = agent_workloads.get(&update_record.workload_name).unwrap();
        assert_eq!(updated_record.to_owned(), ExecutionState::ExecSucceeded);
    }

    #[test]
    fn utest_update_storage_add_multiple_records() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let agent_name_a = String::from("test_agent_a");
        let agent_name_b = String::from("test_agent_b");
        let workload_name_1 = String::from("test_workload_1");
        let workload_name_2 = String::from("test_workload_2");

        let test_update1 = WorkloadState {
            workload_name: workload_name_1.clone(),
            agent_name: agent_name_a.clone(),
            execution_state: ExecutionState::ExecRunning,
        };
        storage.update_workload_state(test_update1);

        let test_update2 = WorkloadState {
            workload_name: workload_name_2.clone(),
            agent_name: agent_name_a.clone(),
            execution_state: ExecutionState::ExecFailed,
        };
        storage.update_workload_state(test_update2);

        let test_update3 = WorkloadState {
            workload_name: workload_name_1.clone(),
            agent_name: agent_name_b.clone(),
            execution_state: ExecutionState::ExecSucceeded,
        };
        storage.update_workload_state(test_update3);

        let test_update4 = WorkloadState {
            workload_name: workload_name_2.clone(),
            agent_name: agent_name_b.clone(),
            execution_state: ExecutionState::ExecStarting,
        };
        storage.update_workload_state(test_update4);

        assert_eq!(storage.states_storage.len(), 2);

        let agent_a_workloads = storage.states_storage.get(&agent_name_a).unwrap();
        assert_eq!(agent_a_workloads.len(), 2);

        let storage_record_1 = agent_a_workloads.get(&workload_name_1).unwrap();
        assert_eq!(storage_record_1.to_owned(), ExecutionState::ExecRunning);

        let storage_record_2 = agent_a_workloads.get(&workload_name_2).unwrap();
        assert_eq!(storage_record_2.to_owned(), ExecutionState::ExecFailed);

        let agent_b_workloads = storage.states_storage.get(&agent_name_b).unwrap();
        assert_eq!(agent_b_workloads.len(), 2);

        let storage_record_3 = agent_b_workloads.get(&workload_name_1).unwrap();
        assert_eq!(storage_record_3.to_owned(), ExecutionState::ExecSucceeded);

        let storage_record_4 = agent_b_workloads.get(&workload_name_2).unwrap();
        assert_eq!(storage_record_4.to_owned(), ExecutionState::ExecStarting);
    }
}

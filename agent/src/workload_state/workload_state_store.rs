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

use common::objects::{ExecutionState, WorkloadState};
use std::collections::HashMap;

type WorkloadStateMap = HashMap<String, (common::objects::ExecutionState, String)>;

pub struct WorkloadStateStore {
    states_storage: WorkloadStateMap,
}

impl WorkloadStateStore {
    pub fn new() -> Self {
        Self {
            states_storage: HashMap::new(),
        }
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadState) {
        if workload_state.execution_state != ExecutionState::ExecRemoved {
            self.states_storage.insert(
                workload_state.workload_name,
                (workload_state.execution_state, workload_state.agent_name),
            );
        } else {
            self.states_storage.remove(&workload_state.workload_name);
        }
    }

    pub fn get_state_of_workload(&self, workload_name: &str) -> Option<&ExecutionState> {
        self.states_storage
            .get(workload_name)
            .map(|(state, _agent)| state)
    }
}

#[cfg(test)]
mod tests {

    use super::WorkloadStateStore;
    use common::objects::{ExecutionState, WorkloadState};

    #[test]
    fn utest_update_storage_empty_storage_add_one() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = WorkloadState {
            workload_name: String::from("test_workload"),
            agent_name: String::from("test_agent"),
            execution_state: ExecutionState::ExecRunning,
        };
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let (state, agent) = storage
            .states_storage
            .get(&test_update.workload_name)
            .unwrap();
        assert_eq!(state, &test_update.execution_state);
        assert_eq!(agent, &test_update.agent_name);
    }

    #[test]
    fn utest_update_storage_removed_gets_sdeleted() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = WorkloadState {
            workload_name: String::from("test_workload"),
            agent_name: String::from("test_agent"),
            execution_state: ExecutionState::ExecRunning,
        };
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionState::ExecRemoved;
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_update_record() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = WorkloadState {
            workload_name: String::from("test_workload"),
            agent_name: String::from("test_agent"),
            execution_state: ExecutionState::ExecRunning,
        };
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let (state, agent) = storage
            .states_storage
            .get(&test_update.workload_name)
            .unwrap();
        assert_eq!(state, &test_update.execution_state);
        assert_eq!(agent, &test_update.agent_name);

        let mut update_record = test_update.clone();
        update_record.execution_state = ExecutionState::ExecSucceeded;

        storage.update_workload_state(update_record.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let (state, agent) = storage
            .states_storage
            .get(&test_update.workload_name)
            .unwrap();
        assert_eq!(state, &update_record.execution_state);
        assert_eq!(agent, &test_update.agent_name);
    }

    #[test]
    fn utest_update_storage_add_multiple_records() {
        let mut storage = WorkloadStateStore::new();
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
        storage.update_workload_state(test_update3.clone());

        let test_update4 = WorkloadState {
            workload_name: workload_name_2.clone(),
            agent_name: agent_name_b.clone(),
            execution_state: ExecutionState::ExecStarting,
        };
        storage.update_workload_state(test_update4.clone());

        assert_eq!(storage.states_storage.len(), 2);

        let (state, agent) = storage
            .states_storage
            .get(&test_update3.workload_name)
            .unwrap();
        assert_eq!(state, &test_update3.execution_state);
        assert_eq!(agent, &test_update3.agent_name);

        let (state, agent) = storage
            .states_storage
            .get(&test_update4.workload_name)
            .unwrap();
        assert_eq!(state, &test_update4.execution_state);
        assert_eq!(agent, &test_update4.agent_name);
    }
}

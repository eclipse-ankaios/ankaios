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

#[cfg(test)]
use mockall::automock;

type WorkloadStates = HashMap<String, common::objects::ExecutionState>;
type AgentWorkloadStates = HashMap<String, WorkloadStates>;

pub struct ParameterStorage {
    states_storage: AgentWorkloadStates,
}

#[cfg_attr(test, automock)]
impl ParameterStorage {
    pub fn new() -> Self {
        Self {
            states_storage: HashMap::new(),
        }
    }

    pub fn get_workload_state(
        &self,
        agent_name: &str,
        workload_name: &str,
    ) -> Option<ExecutionState> {
        self.states_storage
            .get(agent_name)
            .and_then(|wl_states| wl_states.get(workload_name))
            .cloned()
    }

    pub fn get_workload_state_by_workload_name(
        &self,
        workload_name: &String,
    ) -> Option<ExecutionState> {
        for per_workload_states in self.states_storage.values() {
            let workload_state = per_workload_states.get(workload_name);
            if workload_state.is_some() {
                return workload_state.cloned();
            }
        }
        None
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadState) {
        let agent_workloads = self
            .states_storage
            .entry(workload_state.instance_name.agent_name().to_owned())
            .or_default();

        if !workload_state.execution_state.is_removed() {
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
    use std::collections::HashMap;

    use super::ParameterStorage;
    use common::objects::ExecutionState;

    #[test]
    fn utest_update_storage_empty_storage() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let test_update = common::objects::generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ExecutionState::running(),
        );
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let agent_workloads = storage
            .states_storage
            .get(test_update.instance_name.agent_name())
            .unwrap();
        assert_eq!(agent_workloads.len(), 1);

        let storage_record = agent_workloads
            .get(test_update.instance_name.workload_name())
            .unwrap();
        assert_eq!(storage_record.to_owned(), ExecutionState::running());

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionState::removed();
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_update_record() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let test_update = common::objects::generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ExecutionState::running(),
        );

        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let mut agent_workloads = storage
            .states_storage
            .get(test_update.instance_name.agent_name())
            .unwrap();
        assert_eq!(agent_workloads.len(), 1);

        let storage_record = agent_workloads
            .get(test_update.instance_name.workload_name())
            .unwrap();
        assert_eq!(storage_record.to_owned(), ExecutionState::running());

        let mut update_record = test_update.clone();
        update_record.execution_state = ExecutionState::succeeded();

        storage.update_workload_state(update_record.clone());

        assert_eq!(storage.states_storage.len(), 1);

        agent_workloads = storage
            .states_storage
            .get(test_update.instance_name.agent_name())
            .unwrap();
        let updated_record = agent_workloads
            .get(update_record.instance_name.workload_name())
            .unwrap();
        assert_eq!(updated_record.to_owned(), ExecutionState::succeeded());
    }

    #[test]
    fn utest_update_storage_add_multiple_records() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let agent_name_a = String::from("test_agent_a");
        let agent_name_b = String::from("test_agent_b");
        let workload_name_1 = String::from("test_workload_1");
        let workload_name_2 = String::from("test_workload_2");

        let test_update1 = common::objects::generate_test_workload_state_with_agent(
            &workload_name_1,
            &agent_name_a,
            ExecutionState::running(),
        );
        storage.update_workload_state(test_update1);

        let test_update2 = common::objects::generate_test_workload_state_with_agent(
            &workload_name_2,
            &agent_name_a,
            ExecutionState::failed("Some error"),
        );
        storage.update_workload_state(test_update2);

        let test_update3 = common::objects::generate_test_workload_state_with_agent(
            &workload_name_1,
            &agent_name_b,
            ExecutionState::succeeded(),
        );
        storage.update_workload_state(test_update3);

        let test_update4 = common::objects::generate_test_workload_state_with_agent(
            &workload_name_2,
            &agent_name_b,
            ExecutionState::starting("Some info"),
        );
        storage.update_workload_state(test_update4);

        assert_eq!(storage.states_storage.len(), 2);

        let agent_a_workloads = storage.states_storage.get(&agent_name_a).unwrap();
        assert_eq!(agent_a_workloads.len(), 2);

        let storage_record_1 = agent_a_workloads.get(&workload_name_1).unwrap();
        assert_eq!(storage_record_1.to_owned(), ExecutionState::running());

        let storage_record_2 = agent_a_workloads.get(&workload_name_2).unwrap();
        assert_eq!(
            storage_record_2.to_owned(),
            ExecutionState::failed("Some error")
        );

        let agent_b_workloads = storage.states_storage.get(&agent_name_b).unwrap();
        assert_eq!(agent_b_workloads.len(), 2);

        let storage_record_3 = agent_b_workloads.get(&workload_name_1).unwrap();
        assert_eq!(storage_record_3.to_owned(), ExecutionState::succeeded());

        let storage_record_4 = agent_b_workloads.get(&workload_name_2).unwrap();
        assert_eq!(
            storage_record_4.to_owned(),
            ExecutionState::starting("Some info")
        );
    }

    #[test]
    fn utest_get_workload_state() {
        let mut parameter_storage = ParameterStorage::new();
        parameter_storage.states_storage.insert(
            "agent_A".to_owned(),
            HashMap::from([("workload_1".to_owned(), ExecutionState::running())]),
        );

        assert_eq!(
            Some(ExecutionState::running()),
            parameter_storage.get_workload_state("agent_A", "workload_1")
        );
    }

    #[test]
    fn utest_get_workload_state_agent_not_found() {
        let mut parameter_storage = ParameterStorage::new();
        parameter_storage.states_storage.insert(
            "agent_A".to_owned(),
            HashMap::from([("workload_1".to_owned(), ExecutionState::running())]),
        );

        assert!(parameter_storage
            .get_workload_state("unknown agent", "workload_1")
            .is_none());
    }

    #[test]
    fn utest_get_workload_state_workload_not_found() {
        let mut parameter_storage = ParameterStorage::new();
        parameter_storage.states_storage.insert(
            "agent_A".to_owned(),
            HashMap::from([("workload_1".to_owned(), ExecutionState::running())]),
        );

        assert!(parameter_storage
            .get_workload_state("agent_A", "unknown workload")
            .is_none());
    }

    #[test]
    fn utest_get_workload_state_by_workload_name() {
        let mut parameter_storage = ParameterStorage::new();
        parameter_storage.states_storage.insert(
            "agent_A".to_owned(),
            HashMap::from([("workload_1".to_owned(), ExecutionState::running())]),
        );

        assert_eq!(
            Some(ExecutionState::running()),
            parameter_storage.get_workload_state_by_workload_name(&"workload_1".to_owned())
        );
    }

    #[test]
    fn utest_get_workload_state_by_workload_name_not_existing_workload() {
        let mut parameter_storage = ParameterStorage::new();
        parameter_storage.states_storage.insert(
            "agent_A".to_owned(),
            HashMap::from([("workload_1".to_owned(), ExecutionState::running())]),
        );

        assert!(parameter_storage
            .get_workload_state_by_workload_name(&"unknown workload".to_owned())
            .is_none());
    }
}

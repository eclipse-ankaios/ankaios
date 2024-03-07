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
use std::collections::VecDeque;

type WorkloadStates = HashMap<String, common::objects::ExecutionState>;

pub struct ParameterStorage {
    states_storage: WorkloadStates,
}

impl ParameterStorage {
    pub fn new() -> Self {
        Self {
            states_storage: HashMap::new(),
        }
    }

    pub fn get_state_of_workload<'a>(&'a self, workload_name: &str) -> Option<&'a ExecutionState> {
        self.states_storage.get(workload_name)
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadState) {
        let workload_name = workload_state.instance_name.workload_name().to_owned();
        if !workload_state.execution_state.is_removed() {
            self.states_storage
                .insert(workload_name, workload_state.execution_state);
        } else {
            self.states_storage.remove(&workload_name);
        }
    }
}

#[cfg(test)]
static NEW_MOCK_PARAMETER_STORAGE: std::sync::Mutex<Option<MockParameterStorage>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub fn mock_parameter_storage_new_returns(mock_parameter_storage: MockParameterStorage) {
    *NEW_MOCK_PARAMETER_STORAGE.lock().unwrap() = Some(mock_parameter_storage);
}

#[cfg(test)]
#[derive(Default)]
pub struct MockParameterStorage {
    pub expected_update_workload_state_parameters: VecDeque<WorkloadState>,
    pub states_storage: HashMap<String, ExecutionState>,
}

#[cfg(test)]
impl MockParameterStorage {
    pub fn new() -> MockParameterStorage {
        NEW_MOCK_PARAMETER_STORAGE
            .lock()
            .expect("Could not get lock for NEW_MOCK_PARAMETER_STORAGE")
            .take()
            .expect("Return value for MockParameterStorage::new() not set")
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadState) {
        let expected_workload_state = self
            .expected_update_workload_state_parameters
            .pop_front()
            .expect("No further call for update_workload_state expected");
        assert_eq!(
            expected_workload_state, workload_state,
            "Expected workload state {:?}, got {:?}",
            expected_workload_state, workload_state
        );
    }

    pub fn get_state_of_workload<'a>(&'a self, workload_name: &str) -> Option<&'a ExecutionState> {
        self.states_storage.get(workload_name)
    }
}

#[cfg(test)]
impl Drop for MockParameterStorage {
    fn drop(&mut self) {
        assert!(self.expected_update_workload_state_parameters.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::ParameterStorage;
    use common::objects::ExecutionState;

    #[test]
    fn utest_update_storage_empty_storage_add_one() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let test_update = common::objects::generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ExecutionState::running(),
        );
        storage.update_workload_state(test_update.clone());

        assert_eq!(
            storage
                .states_storage
                .get(test_update.instance_name.workload_name()),
            Some(&ExecutionState::running())
        );

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionState::removed();
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_removed_gets_state_deleted() {
        let mut storage = ParameterStorage::new();
        assert!(storage.states_storage.is_empty());

        let test_update = common::objects::generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ExecutionState::running(),
        );
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

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

        storage.states_storage.insert(
            test_update.instance_name.workload_name().to_owned(),
            test_update.execution_state.clone(),
        );

        let mut updated_record = test_update.clone();
        updated_record.execution_state = ExecutionState::succeeded();

        storage.update_workload_state(updated_record);

        assert_eq!(
            storage
                .states_storage
                .get(test_update.instance_name.workload_name()),
            Some(&ExecutionState::succeeded())
        );
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
            &agent_name_b,
            ExecutionState::running(),
        );
        storage.update_workload_state(test_update1);

        let test_update2 = common::objects::generate_test_workload_state_with_agent(
            &workload_name_2,
            &agent_name_a,
            ExecutionState::failed("Some error"),
        );
        storage.update_workload_state(test_update2);

        assert_eq!(storage.states_storage.len(), 2);
        assert_eq!(
            storage.states_storage.get(&workload_name_1),
            Some(&ExecutionState::running())
        );

        assert_eq!(
            storage.states_storage.get(&workload_name_2),
            Some(&ExecutionState::failed("Some error"))
        );

        let test_update3 = common::objects::generate_test_workload_state_with_agent(
            &workload_name_1,
            &agent_name_b,
            ExecutionState::starting("Some info"),
        );

        storage.update_workload_state(test_update3);

        assert_eq!(
            storage.states_storage.get(&workload_name_1),
            Some(&ExecutionState::starting("Some info"))
        );
    }

    #[test]
    fn utest_get_state_of_workload() {
        let mut parameter_storage = ParameterStorage::new();
        parameter_storage
            .states_storage
            .insert("workload_1".to_owned(), ExecutionState::running());

        assert_eq!(
            Some(&ExecutionState::running()),
            parameter_storage.get_state_of_workload("workload_1")
        );
    }

    #[test]
    fn utest_get_state_of_workload_not_existing_workload() {
        let mut parameter_storage = ParameterStorage::new();
        parameter_storage
            .states_storage
            .insert("workload_1".to_owned(), ExecutionState::running());

        assert!(parameter_storage
            .get_state_of_workload("unknown workload")
            .is_none());
    }
}

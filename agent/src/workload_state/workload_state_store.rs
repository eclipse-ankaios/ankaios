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

use api::ank_base::{ExecutionStateInternal, WorkloadStateInternal};
use std::collections::HashMap;
#[cfg(test)]
use std::collections::VecDeque;

type WorkloadStates = HashMap<String, ExecutionStateInternal>;

pub struct WorkloadStateStore {
    states_storage: WorkloadStates,
}

impl WorkloadStateStore {
    pub fn new() -> Self {
        Self {
            states_storage: HashMap::new(),
        }
    }

    pub fn get_state_of_workload<'a>(
        &'a self,
        workload_name: &str,
    ) -> Option<&'a ExecutionStateInternal> {
        self.states_storage.get(workload_name)
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadStateInternal) {
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
static NEW_MOCK_WL_STATE_STORE: std::sync::Mutex<Option<MockWorkloadStateStore>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub fn mock_parameter_storage_new_returns(mock_parameter_storage: MockWorkloadStateStore) {
    *NEW_MOCK_WL_STATE_STORE.lock().unwrap() = Some(mock_parameter_storage);
}

#[cfg(test)]
#[derive(Default)]
pub struct MockWorkloadStateStore {
    pub expected_update_workload_state_parameters: VecDeque<WorkloadStateInternal>,
    pub states_storage: HashMap<String, ExecutionStateInternal>,
}

#[cfg(test)]
impl MockWorkloadStateStore {
    pub fn new() -> MockWorkloadStateStore {
        NEW_MOCK_WL_STATE_STORE
            .lock()
            .expect("Could not get lock for NEW_MOCK_WL_STATE_STORE")
            .take()
            .expect("Return value for MockWorkloadStateStore::new() not set")
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadStateInternal) {
        let expected_workload_state = self
            .expected_update_workload_state_parameters
            .pop_front()
            .expect("No further call for update_workload_state expected");
        assert_eq!(
            expected_workload_state, workload_state,
            "Expected workload state {expected_workload_state:?}, got {workload_state:?}"
        );
    }

    pub fn get_state_of_workload<'a>(
        &'a self,
        workload_name: &str,
    ) -> Option<&'a ExecutionStateInternal> {
        self.states_storage.get(workload_name)
    }
}

#[cfg(test)]
impl Drop for MockWorkloadStateStore {
    fn drop(&mut self) {
        assert!(self.expected_update_workload_state_parameters.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::WorkloadStateStore;
    use api::ank_base::ExecutionStateInternal;
    use api::test_utils::generate_test_workload_state_with_agent;

    #[test]
    fn utest_update_storage_empty_storage_add_one() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ExecutionStateInternal::running(),
        );
        storage.update_workload_state(test_update.clone());

        assert_eq!(
            storage
                .states_storage
                .get(test_update.instance_name.workload_name()),
            Some(&ExecutionStateInternal::running())
        );

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionStateInternal::removed();
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_removed_gets_state_deleted() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ExecutionStateInternal::running(),
        );
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionStateInternal::removed();
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_update_record() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = generate_test_workload_state_with_agent(
            "test_workload",
            "test_agent",
            ExecutionStateInternal::running(),
        );

        storage.states_storage.insert(
            test_update.instance_name.workload_name().to_owned(),
            test_update.execution_state.clone(),
        );

        let mut updated_record = test_update.clone();
        updated_record.execution_state = ExecutionStateInternal::succeeded();

        storage.update_workload_state(updated_record);

        assert_eq!(
            storage
                .states_storage
                .get(test_update.instance_name.workload_name()),
            Some(&ExecutionStateInternal::succeeded())
        );
    }

    #[test]
    fn utest_update_storage_add_multiple_records() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let agent_name_a = String::from("test_agent_a");
        let agent_name_b = String::from("test_agent_b");
        let workload_name_1 = String::from("test_workload_1");
        let workload_name_2 = String::from("test_workload_2");

        let test_update1 = generate_test_workload_state_with_agent(
            &workload_name_1,
            &agent_name_b,
            ExecutionStateInternal::running(),
        );
        storage.update_workload_state(test_update1);

        let test_update2 = generate_test_workload_state_with_agent(
            &workload_name_2,
            &agent_name_a,
            ExecutionStateInternal::failed("Some error"),
        );
        storage.update_workload_state(test_update2);

        assert_eq!(storage.states_storage.len(), 2);
        assert_eq!(
            storage.states_storage.get(&workload_name_1),
            Some(&ExecutionStateInternal::running())
        );

        assert_eq!(
            storage.states_storage.get(&workload_name_2),
            Some(&ExecutionStateInternal::failed("Some error"))
        );

        let test_update3 = generate_test_workload_state_with_agent(
            &workload_name_1,
            &agent_name_b,
            ExecutionStateInternal::starting("Some info"),
        );

        storage.update_workload_state(test_update3);

        assert_eq!(
            storage.states_storage.get(&workload_name_1),
            Some(&ExecutionStateInternal::starting("Some info"))
        );
    }

    #[test]
    fn utest_get_state_of_workload() {
        let mut parameter_storage = WorkloadStateStore::new();
        parameter_storage
            .states_storage
            .insert("workload_1".to_owned(), ExecutionStateInternal::running());

        assert_eq!(
            Some(&ExecutionStateInternal::running()),
            parameter_storage.get_state_of_workload("workload_1")
        );
    }

    #[test]
    fn utest_get_state_of_workload_not_existing_workload() {
        let mut parameter_storage = WorkloadStateStore::new();
        parameter_storage
            .states_storage
            .insert("workload_1".to_owned(), ExecutionStateInternal::running());

        assert!(
            parameter_storage
                .get_state_of_workload("unknown workload")
                .is_none()
        );
    }
}

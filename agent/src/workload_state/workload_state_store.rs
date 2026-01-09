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

use ankaios_api::ank_base::WorkloadStateSpec;
use std::collections::HashMap;
#[cfg(test)]
use std::collections::VecDeque;

type WorkloadStates = HashMap<String, Vec<WorkloadStateSpec>>;

pub struct WorkloadStateStore {
    states_storage: WorkloadStates,
}

impl WorkloadStateStore {
    pub fn new() -> Self {
        Self {
            states_storage: HashMap::new(),
        }
    }

    pub fn get_states_of_workload<'a>(
        &'a self,
        workload_name: &str,
    ) -> Option<&'a Vec<WorkloadStateSpec>> {
        self.states_storage.get(workload_name)
    }

    pub fn update_workload_state(&mut self, new_state: WorkloadStateSpec) {
        let workload_name = new_state.instance_name.workload_name().to_owned();

        if new_state.execution_state.is_removed() {

            if let Some(workload_states) = self.states_storage.get_mut(&workload_name) {
                workload_states.retain(|state| state.instance_name != new_state.instance_name);

                if workload_states.is_empty() {
                    self.states_storage.remove(&workload_name);
                }
            }
        } else {
            let workload_states = self.states_storage
                .entry(workload_name)
                .or_default();

            match workload_states.iter_mut().find(|state| state.instance_name == new_state.instance_name) {
                Some(existing_state) => *existing_state = new_state,
                None => workload_states.push(new_state),
            }
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
static NEW_MOCK_WL_STATE_STORE: std::sync::Mutex<Option<MockWorkloadStateStore>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub fn mock_parameter_storage_new_returns(mock_parameter_storage: MockWorkloadStateStore) {
    *NEW_MOCK_WL_STATE_STORE.lock().unwrap() = Some(mock_parameter_storage);
}

#[cfg(test)]
#[derive(Default)]
pub struct MockWorkloadStateStore {
    pub expected_update_workload_state_parameters: VecDeque<WorkloadStateSpec>,
    pub states_storage: HashMap<String, Vec<WorkloadStateSpec>>,
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

    pub fn update_workload_state(&mut self, workload_state: WorkloadStateSpec) {
        let expected_workload_state = self
            .expected_update_workload_state_parameters
            .pop_front()
            .expect("No further call for update_workload_state expected");
        assert_eq!(
            expected_workload_state, workload_state,
            "Expected workload state {expected_workload_state:?}, got {workload_state:?}"
        );
    }

    pub fn get_states_of_workload<'a>(
        &'a self,
        workload_name: &str,
    ) -> Option<&'a Vec<WorkloadStateSpec>> {
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
    use ankaios_api::ank_base::ExecutionStateSpec;
    use ankaios_api::test_utils::{fixtures, generate_test_workload_state_with_agent};

    #[test]
    fn utest_update_storage_empty_storage_add_one() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );
        storage.update_workload_state(test_update.clone());

        assert_eq!(
            storage
                .states_storage
                .get(test_update.instance_name.workload_name()),
            Some(&vec![test_update.clone()])
        );

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionStateSpec::removed();
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_removed_gets_state_deleted() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );
        storage.update_workload_state(test_update.clone());

        assert_eq!(storage.states_storage.len(), 1);

        let mut removed_update = test_update.clone();
        removed_update.execution_state = ExecutionStateSpec::removed();
        storage.update_workload_state(removed_update);

        assert!(storage.states_storage.is_empty());
    }

    #[test]
    fn utest_update_storage_update_record() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let test_update = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );

        storage.states_storage.insert(
            test_update.instance_name.workload_name().to_owned(),
            vec![test_update.clone()],
        );

        let mut updated_record = test_update.clone();
        updated_record.execution_state = ExecutionStateSpec::succeeded();

        storage.update_workload_state(updated_record.clone());

        assert_eq!(
            storage
                .states_storage
                .get(test_update.instance_name.workload_name()),
            Some(&vec![updated_record.clone()])
        );
    }

    #[test]
    fn utest_update_storage_add_multiple_records() {
        let mut storage = WorkloadStateStore::new();
        assert!(storage.states_storage.is_empty());

        let agent_name_a = String::from(fixtures::AGENT_NAMES[0]);
        let agent_name_b = String::from(fixtures::AGENT_NAMES[1]);
        let workload_name_1 = String::from(fixtures::WORKLOAD_NAMES[0]);
        let workload_name_2 = String::from(fixtures::WORKLOAD_NAMES[1]);

        let test_update1 = generate_test_workload_state_with_agent(
            &workload_name_1,
            &agent_name_b,
            ExecutionStateSpec::running(),
        );
        storage.update_workload_state(test_update1.clone());

        let test_update2 = generate_test_workload_state_with_agent(
            &workload_name_2,
            &agent_name_a,
            ExecutionStateSpec::failed("Some error"),
        );
        storage.update_workload_state(test_update2.clone());

        assert_eq!(storage.states_storage.len(), 2);
        assert_eq!(
            storage.states_storage.get(&workload_name_1),
            Some(&vec![test_update1.clone()])
        );

        assert_eq!(
            storage.states_storage.get(&workload_name_2),
            Some(&vec![test_update2.clone()])
        );

        let test_update3 = generate_test_workload_state_with_agent(
            &workload_name_1,
            &agent_name_b,
            ExecutionStateSpec::starting("Some info"),
        );

        storage.update_workload_state(test_update3.clone());

        assert_eq!(
            storage.states_storage.get(&workload_name_1),
            Some(&vec![test_update3.clone()])
        );
    }

    #[test]
    fn utest_get_state_of_workload() {
        let mut parameter_storage = WorkloadStateStore::new();
        let wl_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );
        parameter_storage
            .states_storage
            .insert(fixtures::WORKLOAD_NAMES[0].to_owned(), vec![wl_state]);

        let expected_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );

        assert_eq!(
            Some(&vec![expected_state]),
            parameter_storage.get_states_of_workload(fixtures::WORKLOAD_NAMES[0])
        );
    }

    #[test]
    fn utest_get_state_of_workload_not_existing_workload() {
        let mut parameter_storage = WorkloadStateStore::new();
        let wl_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );
        parameter_storage
            .states_storage
            .insert(fixtures::WORKLOAD_NAMES[0].to_owned(), vec![wl_state]);

        assert!(
            parameter_storage
                .get_states_of_workload("unknown workload")
                .is_none()
        );
    }
}

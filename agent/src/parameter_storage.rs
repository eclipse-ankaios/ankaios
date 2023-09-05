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

use common::objects::{ExecutionState, WorkloadSpec, WorkloadState};
use std::collections::HashMap;

type WorkloadStates = HashMap<String, common::objects::ExecutionState>;
type AgentWorkloadStates = HashMap<String, WorkloadStates>;
type WorkloadName = String;
type RuntimeName = String;
type WorkloadRuntimeMapping = HashMap<WorkloadName, RuntimeName>;

pub struct ParameterStorage {
    states_storage: AgentWorkloadStates,
    workload_storage: WorkloadRuntimeMapping,
}

impl ParameterStorage {
    pub fn new() -> Self {
        Self {
            states_storage: HashMap::new(),
            workload_storage: HashMap::new(),
        }
    }

    // [impl->swdd~agent-manager-deletes-workload-runtime-mapping~1]
    pub fn get_workload_runtime(&self, workload_name: &WorkloadName) -> Option<&RuntimeName> {
        self.workload_storage.get(workload_name)
    }

    // [impl->swdd~agent-manager-stores-workload-runtime-mapping~1]
    pub fn set_workload_runtime(&mut self, workload_spec: &WorkloadSpec) {
        self.workload_storage.insert(
            workload_spec.workload.name.clone(),
            workload_spec.runtime.clone(),
        );
    }

    // [impl->swdd~agent-manager-deletes-workload-runtime-mapping~1]
    pub fn delete_workload_runtime(&mut self, workload_name: &WorkloadName) {
        self.workload_storage.remove(workload_name);
    }

    // Currently used only in tests. Update tests if you have another "public getter".
    #[allow(dead_code)]
    pub fn get_workload_states(&self, agent_name: &String) -> Option<&WorkloadStates> {
        self.states_storage.get(agent_name)
    }

    pub fn update_workload_state(&mut self, workload_state: WorkloadState) {
        let agent_workloads = self
            .states_storage
            .entry(workload_state.agent_name)
            .or_default();

        if workload_state.execution_state != ExecutionState::ExecRemoved {
            agent_workloads.insert(workload_state.workload_name, workload_state.execution_state);
        } else {
            agent_workloads.remove(&workload_state.workload_name);
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
    use common::{
        objects::{ExecutionState, WorkloadState},
        test_utils::generate_test_workload_spec_with_param,
    };

    use crate::parameter_storage::ParameterStorage;

    const AGENT_NAME: &str = "agent_x";

    const WORKLOAD_1_NAME: &str = "workload A";
    const RUNTIME_1_NAME: &str = "runtime B";
    const WORKLOAD_2_NAME: &str = "workload C";
    const RUNTIME_2_NAME: &str = "runtime D";

    // [utest->swdd~agent-manager-stores-workload-runtime-mapping~1]
    // [utest->swdd~agent-manager-deletes-workload-runtime-mapping~1]
    #[test]
    fn utest_parameter_storage_runtime_set_get_del() {
        let mut storage = ParameterStorage::new();
        assert!(storage.workload_storage.is_empty());

        let workload_spec_1 = generate_test_workload_spec_with_param(
            AGENT_NAME.into(),
            WORKLOAD_1_NAME.into(),
            RUNTIME_1_NAME.into(),
        );

        let workload_spec_2 = generate_test_workload_spec_with_param(
            AGENT_NAME.into(),
            WORKLOAD_2_NAME.into(),
            RUNTIME_2_NAME.into(),
        );

        // Nothing in the storage yet
        assert!(storage.workload_storage.is_empty());

        storage.set_workload_runtime(&workload_spec_1);
        storage.set_workload_runtime(&workload_spec_2);

        assert_eq!(
            storage
                .get_workload_runtime(&workload_spec_1.workload.name)
                .expect("runtime for workload is there"),
            &workload_spec_1.runtime
        );

        storage.delete_workload_runtime(&workload_spec_1.workload.name);
        assert!(storage
            .get_workload_runtime(&workload_spec_1.workload.name)
            .is_none());

        assert_eq!(
            storage
                .get_workload_runtime(&workload_spec_2.workload.name)
                .expect("runtime for workload is there"),
            &workload_spec_2.runtime
        );

        storage.delete_workload_runtime(&workload_spec_2.workload.name);
        assert!(storage.workload_storage.is_empty());
    }

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

        let agent_workloads = storage.states_storage.get(&test_update.agent_name);
        assert!(agent_workloads.is_some());
        assert_eq!(agent_workloads.unwrap().len(), 1);

        let storage_record = agent_workloads.unwrap().get(&test_update.workload_name);
        assert!(storage_record.is_some());
        assert_eq!(
            storage_record.unwrap().to_owned(),
            ExecutionState::ExecRunning
        );

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

        let mut agent_workloads = storage.states_storage.get(&test_update.agent_name);
        assert!(agent_workloads.is_some());
        assert_eq!(agent_workloads.unwrap().len(), 1);

        let storage_record = agent_workloads.unwrap().get(&test_update.workload_name);
        assert!(storage_record.is_some());
        assert_eq!(
            storage_record.unwrap().to_owned(),
            ExecutionState::ExecRunning
        );

        let mut update_record = test_update.clone();
        update_record.execution_state = ExecutionState::ExecSucceeded;

        storage.update_workload_state(update_record.clone());

        assert_eq!(storage.states_storage.len(), 1);

        agent_workloads = storage.states_storage.get(&test_update.agent_name);
        let updated_record = agent_workloads.unwrap().get(&update_record.workload_name);
        assert!(updated_record.is_some());
        assert_eq!(
            updated_record.unwrap().to_owned(),
            ExecutionState::ExecSucceeded
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
            execution_state: ExecutionState::ExecPending,
        };
        storage.update_workload_state(test_update4);

        assert_eq!(storage.states_storage.len(), 2);

        let agent_a_workloads = storage.states_storage.get(&agent_name_a);
        assert!(agent_a_workloads.is_some());
        assert_eq!(agent_a_workloads.unwrap().len(), 2);

        let storage_record_1 = agent_a_workloads.unwrap().get(&workload_name_1);
        assert!(storage_record_1.is_some());
        assert_eq!(
            storage_record_1.unwrap().to_owned(),
            ExecutionState::ExecRunning
        );

        let storage_record_2 = agent_a_workloads.unwrap().get(&workload_name_2);
        assert!(storage_record_2.is_some());
        assert_eq!(
            storage_record_2.unwrap().to_owned(),
            ExecutionState::ExecFailed
        );

        let agent_b_workloads = storage.states_storage.get(&agent_name_b);
        assert!(agent_b_workloads.is_some());
        assert_eq!(agent_b_workloads.unwrap().len(), 2);

        let storage_record_3 = agent_b_workloads.unwrap().get(&workload_name_1);
        assert!(storage_record_3.is_some());
        assert_eq!(
            storage_record_3.unwrap().to_owned(),
            ExecutionState::ExecSucceeded
        );

        let storage_record_4 = agent_b_workloads.unwrap().get(&workload_name_2);
        assert!(storage_record_4.is_some());
        assert_eq!(
            storage_record_4.unwrap().to_owned(),
            ExecutionState::ExecPending
        );
    }
}

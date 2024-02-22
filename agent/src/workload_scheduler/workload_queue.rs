// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use common::objects::{DeletedWorkload, FulfilledBy, WorkloadInstanceName, WorkloadSpec};

use std::collections::HashMap;

#[cfg_attr(test, mockall_double::double)]
use crate::parameter_storage::ParameterStorage;

#[cfg(test)]
use mockall::automock;

pub type ReadyWorkloads = Vec<WorkloadSpec>;
pub type WaitingWorkloads = Vec<WorkloadSpec>;

pub type ReadyDeletedWorkloads = Vec<DeletedWorkload>;
pub type WaitingDeletedWorkloads = Vec<DeletedWorkload>;

type StartWorkloadQueue = HashMap<WorkloadInstanceName, WorkloadSpec>;
type DeleteWorkloadQueue = HashMap<WorkloadInstanceName, DeletedWorkload>;

pub struct WorkloadQueue {
    start_queue: StartWorkloadQueue,
    delete_queue: DeleteWorkloadQueue,
}

fn split_by_condition<T, P>(container: Vec<T>, predicate: P) -> (Vec<T>, Vec<T>)
where
    P: Fn(&T) -> bool,
{
    let mut items_matching_condition = Vec::new();
    let mut items_not_matching_condition = Vec::new();

    for item in container {
        if predicate(&item) {
            items_matching_condition.push(item);
        } else {
            items_not_matching_condition.push(item);
        }
    }

    (items_matching_condition, items_not_matching_condition)
}

#[cfg_attr(test, automock)]
impl WorkloadQueue {
    pub fn new() -> Self {
        WorkloadQueue {
            start_queue: StartWorkloadQueue::new(),
            delete_queue: DeleteWorkloadQueue::new(),
        }
    }

    pub fn split_workloads_to_ready_and_waiting(
        new_workloads: Vec<WorkloadSpec>,
        workload_state_db: &ParameterStorage,
    ) -> (ReadyWorkloads, WaitingWorkloads) {
        let (ready_to_start_workloads, waiting_to_start_workloads) =
            split_by_condition(new_workloads, |workload| {
                Self::dependencies_for_workload_fulfilled(workload, workload_state_db)
            });
        (ready_to_start_workloads, waiting_to_start_workloads)
    }

    pub fn put_on_waiting_queue(&mut self, workloads: WaitingWorkloads) {
        self.start_queue.extend(
            workloads
                .into_iter()
                .map(|workload| (workload.instance_name.clone(), workload)),
        );
    }

    fn dependencies_for_deleted_workload_fulfilled(
        workload: &DeletedWorkload,
        workload_state_db: &ParameterStorage,
    ) -> bool {
        workload
            .dependencies
            .iter()
            .all(|(dependency_name, delete_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .map_or(true, |wl_state| delete_condition.fulfilled_by(&wl_state))
            })
    }

    pub fn split_deleted_workloads_to_ready_and_waiting(
        deleted_workloads: Vec<DeletedWorkload>,
        workload_state_db: &ParameterStorage,
    ) -> (ReadyDeletedWorkloads, WaitingDeletedWorkloads) {
        let (ready_to_delete_workloads, waiting_to_delete_workloads) =
            split_by_condition(deleted_workloads, |workload| {
                Self::dependencies_for_deleted_workload_fulfilled(workload, workload_state_db)
            });

        (ready_to_delete_workloads, waiting_to_delete_workloads)
    }

    pub fn put_on_delete_waiting_queue(&mut self, workloads: WaitingDeletedWorkloads) {
        self.delete_queue.extend(
            workloads
                .into_iter()
                .map(|workload| (workload.instance_name.clone(), workload)),
        );
    }

    pub fn dependencies_for_workload_fulfilled(
        workload_spec: &WorkloadSpec,
        workload_state_db: &ParameterStorage,
    ) -> bool {
        workload_spec
            .dependencies
            .iter()
            .all(|(dependency_name, add_condition)| {
                workload_state_db
                    .get_state_of_workload(dependency_name)
                    .map_or(false, |wl_state| add_condition.fulfilled_by(&wl_state))
            })
    }

    pub fn next_workloads_to_start(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) -> ReadyWorkloads {
        let ready_workloads: ReadyWorkloads = self
            .start_queue
            .values()
            .filter_map(|workload_spec| {
                Self::dependencies_for_workload_fulfilled(workload_spec, workload_state_db)
                    .then_some(workload_spec.clone())
            })
            .collect();

        for workload in ready_workloads.iter() {
            self.start_queue.remove(&workload.instance_name);
        }

        ready_workloads
    }

    pub fn next_workloads_to_delete(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) -> ReadyDeletedWorkloads {
        let ready_workloads: ReadyDeletedWorkloads = self
            .delete_queue
            .values()
            .filter_map(|deleted_workload| {
                Self::dependencies_for_deleted_workload_fulfilled(
                    deleted_workload,
                    workload_state_db,
                )
                .then_some(deleted_workload.clone())
            })
            .collect();

        for workload in ready_workloads.iter() {
            self.delete_queue.remove(&workload.instance_name);
        }
        ready_workloads
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
    use std::collections::HashMap;

    use common::{
        objects::{
            generate_test_workload_spec_with_dependencies, generate_test_workload_spec_with_param,
            AddCondition, ExecutionState, WorkloadSpec,
        },
        test_utils::generate_test_deleted_workload,
    };

    use crate::workload_scheduler::workload_queue::{DeleteWorkloadQueue, StartWorkloadQueue};

    use super::WorkloadQueue;
    use crate::parameter_storage::MockParameterStorage;

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const RUNTIME: &str = "runtime";

    #[test]
    fn utest_split_workloads_to_ready_and_waiting() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondFailed)]),
        );

        let mut workload_without_dependencies = workload_with_dependencies.clone();
        workload_without_dependencies.dependencies.clear();

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_once(|_| Some(ExecutionState::running()));

        let (ready_workloads, waiting_workloads) =
            WorkloadQueue::split_workloads_to_ready_and_waiting(
                vec![
                    workload_with_dependencies.clone(),
                    workload_without_dependencies.clone(),
                ],
                &parameter_storage_mock,
            );

        assert_eq!(vec![workload_without_dependencies], ready_workloads);
        assert_eq!(vec![workload_with_dependencies], waiting_workloads);
    }

    #[test]
    fn utest_split_workloads_to_ready_and_waiting_dependencies_already_fulfilled() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondSucceeded)]),
        );

        let mut workload_without_dependencies = workload_with_dependencies.clone();
        workload_without_dependencies.dependencies.clear();

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_once(|_| Some(ExecutionState::succeeded()));

        let (ready_workloads, waiting_workloads) =
            WorkloadQueue::split_workloads_to_ready_and_waiting(
                vec![
                    workload_with_dependencies.clone(),
                    workload_without_dependencies.clone(),
                ],
                &parameter_storage_mock,
            );

        assert_eq!(
            vec![workload_with_dependencies, workload_without_dependencies],
            ready_workloads
        );
        assert_eq!(Vec::<WorkloadSpec>::new(), waiting_workloads);
    }

    #[test]
    fn utest_put_on_waiting_queue() {
        let mut dependency_scheduler = WorkloadQueue::new();
        let new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        dependency_scheduler.put_on_waiting_queue(vec![new_workload.clone()]);

        assert_eq!(
            StartWorkloadQueue::from([(new_workload.instance_name.clone(), new_workload)]),
            dependency_scheduler.start_queue
        );
    }

    #[test]
    fn utest_put_on_delete_waiting_queue() {
        let mut dependency_scheduler = WorkloadQueue::new();
        let new_workload =
            generate_test_deleted_workload(AGENT_A.to_string(), WORKLOAD_NAME_1.to_string());

        dependency_scheduler.put_on_delete_waiting_queue(vec![new_workload.clone()]);

        assert_eq!(
            DeleteWorkloadQueue::from([(new_workload.instance_name.clone(), new_workload)]),
            dependency_scheduler.delete_queue
        );
    }

    #[test]
    fn utest_split_deleted_workloads_to_ready_and_waiting() {
        let workload_with_dependencies =
            generate_test_deleted_workload(AGENT_A.to_string(), WORKLOAD_NAME_1.to_string());

        let mut workload_without_dependencies = workload_with_dependencies.clone();
        workload_without_dependencies.dependencies.clear();

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        let (ready_workloads, waiting_workloads) =
            WorkloadQueue::split_deleted_workloads_to_ready_and_waiting(
                vec![
                    workload_with_dependencies.clone(),
                    workload_without_dependencies.clone(),
                ],
                &parameter_storage_mock,
            );

        assert_eq!(vec![workload_without_dependencies], ready_workloads);
        assert_eq!(vec![workload_with_dependencies], waiting_workloads);
    }

    #[test]
    fn utest_split_deleted_workloads_to_ready_and_waiting_ready_to_delete() {
        let workload_with_dependencies =
            generate_test_deleted_workload(AGENT_A.to_string(), WORKLOAD_NAME_1.to_string());

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::succeeded()));

        let (ready_workloads, waiting_workloads) =
            WorkloadQueue::split_deleted_workloads_to_ready_and_waiting(
                vec![workload_with_dependencies.clone()],
                &parameter_storage_mock,
            );

        assert_eq!(vec![workload_with_dependencies], ready_workloads);
        assert!(waiting_workloads.is_empty());
    }

    #[test]
    fn utest_split_deleted_workloads_to_ready_and_waiting_no_workload_state() {
        let workload_with_dependencies =
            generate_test_deleted_workload(AGENT_A.to_string(), WORKLOAD_NAME_1.to_string());

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        let (ready_workloads, waiting_workloads) =
            WorkloadQueue::split_deleted_workloads_to_ready_and_waiting(
                vec![workload_with_dependencies.clone()],
                &parameter_storage_mock,
            );

        assert_eq!(vec![workload_with_dependencies], ready_workloads);
        assert!(waiting_workloads.is_empty());
    }

    #[test]
    fn utest_dependency_states_for_start_fulfilled() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        assert!(WorkloadQueue::dependencies_for_workload_fulfilled(
            &workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_dependency_states_for_start_not_fulfilled() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::removed()));

        assert!(!WorkloadQueue::dependencies_for_workload_fulfilled(
            &workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_dependency_states_for_start_fulfilled_no_workload_state() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        assert!(!WorkloadQueue::dependencies_for_workload_fulfilled(
            &workload_with_dependencies,
            &parameter_storage_mock
        ));
    }

    #[test]
    fn utest_next_workloads_to_start_fulfilled() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondSucceeded)]),
        );

        let mut dependency_scheduler = WorkloadQueue::new();
        dependency_scheduler.start_queue.insert(
            workload_with_dependencies.instance_name.clone(),
            workload_with_dependencies.clone(),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::succeeded()));

        let ready_workloads = dependency_scheduler.next_workloads_to_start(&parameter_storage_mock);
        assert_eq!(vec![workload_with_dependencies], ready_workloads);
    }

    #[test]
    fn utest_next_workloads_to_start_not_fulfilled() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondFailed)]),
        );

        let mut dependency_scheduler = WorkloadQueue::new();
        dependency_scheduler.start_queue.insert(
            workload_with_dependencies.instance_name.clone(),
            workload_with_dependencies.clone(),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        let ready_workloads = dependency_scheduler.next_workloads_to_start(&parameter_storage_mock);
        assert!(ready_workloads.is_empty());
    }

    #[test]
    fn utest_next_workloads_to_start_no_workload_state() {
        let workload_with_dependencies = generate_test_workload_spec_with_dependencies(
            AGENT_A,
            WORKLOAD_NAME_1,
            RUNTIME,
            HashMap::from([(WORKLOAD_NAME_2.to_string(), AddCondition::AddCondRunning)]),
        );

        let mut dependency_scheduler = WorkloadQueue::new();
        dependency_scheduler.start_queue.insert(
            workload_with_dependencies.instance_name.clone(),
            workload_with_dependencies.clone(),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        let ready_workloads = dependency_scheduler.next_workloads_to_start(&parameter_storage_mock);
        assert!(ready_workloads.is_empty());
    }

    #[test]
    fn utest_next_workloads_to_start_on_empty_queue() {
        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .never();

        let mut dependency_scheduler = WorkloadQueue::new();

        assert!(dependency_scheduler.start_queue.is_empty());
        let ready_workloads = dependency_scheduler.next_workloads_to_start(&parameter_storage_mock);
        assert!(ready_workloads.is_empty());
    }

    #[test]
    fn utest_next_workloads_to_delete_fulfilled() {
        let workload_with_dependencies =
            generate_test_deleted_workload(AGENT_A.to_string(), WORKLOAD_NAME_1.to_string());

        let mut dependency_scheduler = WorkloadQueue::new();
        dependency_scheduler.delete_queue.insert(
            workload_with_dependencies.instance_name.clone(),
            workload_with_dependencies.clone(),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::succeeded()));

        let ready_workloads =
            dependency_scheduler.next_workloads_to_delete(&parameter_storage_mock);
        assert_eq!(vec![workload_with_dependencies], ready_workloads);
    }

    #[test]
    fn utest_next_workloads_to_delete_not_fulfilled() {
        let workload_with_dependencies =
            generate_test_deleted_workload(AGENT_A.to_string(), WORKLOAD_NAME_1.to_string());

        let mut dependency_scheduler = WorkloadQueue::new();
        dependency_scheduler.delete_queue.insert(
            workload_with_dependencies.instance_name.clone(),
            workload_with_dependencies.clone(),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(Some(ExecutionState::running()));

        let ready_workloads =
            dependency_scheduler.next_workloads_to_delete(&parameter_storage_mock);
        assert!(ready_workloads.is_empty());
    }

    #[test]
    fn utest_next_workloads_to_delete_on_empty_queue() {
        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .never();

        let mut dependency_scheduler = WorkloadQueue::new();

        assert!(dependency_scheduler.delete_queue.is_empty());
        let ready_workloads =
            dependency_scheduler.next_workloads_to_delete(&parameter_storage_mock);

        assert!(ready_workloads.is_empty());
    }

    #[test]
    fn utest_next_workloads_to_delete_removed_from_queue() {
        let workload_with_dependencies =
            generate_test_deleted_workload(AGENT_A.to_string(), WORKLOAD_NAME_1.to_string());

        let mut dependency_scheduler = WorkloadQueue::new();
        dependency_scheduler.delete_queue.insert(
            workload_with_dependencies.instance_name.clone(),
            workload_with_dependencies.clone(),
        );

        let mut parameter_storage_mock = MockParameterStorage::default();
        parameter_storage_mock
            .expect_get_state_of_workload()
            .once()
            .return_const(None);

        let _ = dependency_scheduler.next_workloads_to_delete(&parameter_storage_mock);

        assert!(!dependency_scheduler
            .delete_queue
            .contains_key(&workload_with_dependencies.instance_name));
    }
}

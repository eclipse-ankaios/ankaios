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

use super::state_validator::{DependencyState, StateValidator};
use common::{
    objects::{DeletedWorkload, ExecutionState, WorkloadSpec, WorkloadState},
    std_extensions::IllegalStateResult,
    to_server_interface::{ToServerInterface, ToServerSender},
};
use std::collections::HashMap;

#[cfg_attr(test, mockall_double::double)]
use crate::parameter_storage::ParameterStorage;
use crate::workload_operation::{WorkloadOperation, WorkloadOperations};

#[cfg(test)]
use mockall::automock;

type DependencyQueue = HashMap<String, WorkloadOperation>;

pub struct WorkloadQueue {
    queue: DependencyQueue,
    workload_state_sender: ToServerSender,
}

#[cfg_attr(test, automock)]
impl WorkloadQueue {
    pub fn new(workload_state_tx: ToServerSender) -> Self {
        WorkloadQueue {
            queue: DependencyQueue::new(),
            workload_state_sender: workload_state_tx,
        }
    }

    async fn report_pending_state_for_waiting_workload(&self, waiting_workload: &WorkloadSpec) {
        self.workload_state_sender
            .update_workload_state(vec![WorkloadState {
                instance_name: waiting_workload.instance_name.clone(),
                execution_state: ExecutionState::waiting_to_start(),
            }])
            .await
            .unwrap_or_illegal_state();
    }

    async fn report_pending_delete_state_for_waiting_workload(
        &self,
        waiting_deleted_workload: &DeletedWorkload,
    ) {
        self.workload_state_sender
            .update_workload_state(vec![WorkloadState {
                instance_name: waiting_deleted_workload.instance_name.clone(),
                execution_state: ExecutionState::waiting_to_stop(),
            }])
            .await
            .unwrap_or_illegal_state();
    }

    async fn insert_and_notify(&mut self, workload_operation: WorkloadOperation) {
        match workload_operation {
            WorkloadOperation::Create(ref workload_spec) => {
                self.report_pending_state_for_waiting_workload(workload_spec)
                    .await;

                self.queue.insert(
                    workload_spec.instance_name.workload_name().to_owned(),
                    workload_operation,
                );
            }
            WorkloadOperation::Update(_, ref deleted_workload) => {
                self.report_pending_delete_state_for_waiting_workload(deleted_workload)
                    .await;

                self.queue.insert(
                    deleted_workload.instance_name.workload_name().to_owned(),
                    workload_operation,
                );
            }
            WorkloadOperation::Delete(ref deleted_workload) => {
                self.report_pending_delete_state_for_waiting_workload(deleted_workload)
                    .await;

                self.queue.insert(
                    deleted_workload.instance_name.workload_name().to_owned(),
                    workload_operation,
                );
            }
        }
    }

    async fn enqueue_filtered_update_operation(
        &mut self,
        new_workload: WorkloadSpec,
        deleted_workload: DeletedWorkload,
        dependency_state: DependencyState,
        ready_workload_operations: &mut WorkloadOperations,
    ) {
        if !dependency_state.is_pending_delete() {
            /* For an update with pending create dependencies but fulfilled delete dependencies
            the delete can be done immediately but the create must wait in the queue. */
            self.insert_and_notify(WorkloadOperation::Create(new_workload))
                .await;

            ready_workload_operations.push(WorkloadOperation::Delete(deleted_workload));
        } else {
            // For an update with pending delete dependencies, the whole update is pending.
            self.insert_and_notify(WorkloadOperation::Update(new_workload, deleted_workload))
                .await;
        }
    }

    pub async fn enqueue_filtered_workload_operations(
        &mut self,
        new_workload_operations: WorkloadOperations,
        workload_state_db: &ParameterStorage,
    ) -> WorkloadOperations {
        let mut ready_workload_operations = WorkloadOperations::new();
        for workload_operation in new_workload_operations {
            let dependency_state = StateValidator::dependencies_for_workload_fulfilled(
                &workload_operation,
                workload_state_db,
            );

            if dependency_state.is_pending() {
                if let WorkloadOperation::Update(new_workload, deleted_workload) =
                    workload_operation
                {
                    self.enqueue_filtered_update_operation(
                        new_workload,
                        deleted_workload,
                        dependency_state,
                        &mut ready_workload_operations,
                    )
                    .await;
                } else {
                    self.insert_and_notify(workload_operation).await;
                }
            } else {
                ready_workload_operations.push(workload_operation);
            }
        }

        ready_workload_operations
    }

    pub fn next_workload_operations(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) -> WorkloadOperations {
        let mut ready_workload_operations = WorkloadOperations::new();
        let mut retained_entries = DependencyQueue::new();

        self.queue
            .drain()
            .for_each(|(workload_name, workload_operation)| {
                let dependency_state = StateValidator::dependencies_for_workload_fulfilled(
                    &workload_operation,
                    workload_state_db,
                );

                if dependency_state.is_fulfilled() {
                    ready_workload_operations.push(workload_operation);
                } else {
                    retained_entries.insert(workload_name, workload_operation);
                }
            });

        self.queue.extend(retained_entries);
        ready_workload_operations
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
            AddCondition, ExecutionState,
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
    fn utest_put_on_waiting_queue() {
        let mut dependency_scheduler = WorkloadQueue::new();
        let new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        dependency_scheduler.enqueue_filtered_workload_operations(vec![new_workload.clone()]);

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

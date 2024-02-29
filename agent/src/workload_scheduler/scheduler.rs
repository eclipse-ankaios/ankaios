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

#[cfg_attr(test, mockall_double::double)]
use crate::workload_scheduler::dependency_state_validator::DependencyStateValidator;

use crate::workload_scheduler::dependency_state_validator::DependencyState;
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

pub struct WorkloadScheduler {
    queue: DependencyQueue,
    workload_state_sender: ToServerSender,
}

#[cfg_attr(test, automock)]
impl WorkloadScheduler {
    pub fn new(workload_state_tx: ToServerSender) -> Self {
        WorkloadScheduler {
            queue: DependencyQueue::new(),
            workload_state_sender: workload_state_tx,
        }
    }

    // [impl->swdd~agent-reports-pending-create-workload-state~1]
    async fn report_pending_create_state(&self, pending_workload: &WorkloadSpec) {
        self.workload_state_sender
            .update_workload_state(vec![WorkloadState {
                instance_name: pending_workload.instance_name.clone(),
                execution_state: ExecutionState::waiting_to_start(),
            }])
            .await
            .unwrap_or_illegal_state();
    }

    // [impl->swdd~agent-reports-pending-delete-workload-state~1]
    async fn report_pending_delete_state(&self, waiting_deleted_workload: &DeletedWorkload) {
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
            // [impl->swdd~agent-enqueues-pending-create-workload-operations~1]
            WorkloadOperation::Create(ref workload_spec) => {
                // [impl->swdd~agent-reports-pending-create-workload-state~1]
                self.report_pending_create_state(workload_spec).await;

                self.queue.insert(
                    workload_spec.instance_name.workload_name().to_owned(),
                    workload_operation,
                );
            }
            // [impl->swdd~agent-enqueues-pending-update-workload-operations~1]
            WorkloadOperation::Update(_, ref deleted_workload) => {
                self.report_pending_delete_state(deleted_workload).await;

                self.queue.insert(
                    deleted_workload.instance_name.workload_name().to_owned(),
                    workload_operation,
                );
            }
            // [impl->swdd~agent-enqueues-pending-delete-workload-operations~1]
            WorkloadOperation::Delete(ref deleted_workload) => {
                // [impl->swdd~agent-reports-pending-delete-workload-state~1]
                self.report_pending_delete_state(deleted_workload).await;

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
        // [impl->swdd~agent-enqueues-pending-create-on-update-workload-operations~1]
        if !dependency_state.is_pending_delete() {
            /* For an update with pending create dependencies but fulfilled delete dependencies
            the delete can be done immediately but the create must wait in the queue. */
            self.insert_and_notify(WorkloadOperation::Create(new_workload))
                .await;

            ready_workload_operations.push(WorkloadOperation::Delete(deleted_workload));
        } else {
            // For an update with pending delete dependencies, the whole update is pending.

            // [impl->swdd~agent-enqueues-pending-update-workload-operations~1]
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
            let dependency_state = DependencyStateValidator::dependencies_for_workload_fulfilled(
                &workload_operation,
                workload_state_db,
            );

            if dependency_state.is_pending() {
                // [impl->swdd~agent-enqueues-workload-operations-with-unfulfilled-dependencies~1]

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
                // [impl->swdd~agent-updates-workloads-with-fulfilled-dependencies~1]
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
                let dependency_state =
                    DependencyStateValidator::dependencies_for_workload_fulfilled(
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
    use common::{
        commands::UpdateWorkloadState,
        objects::{
            generate_test_workload_spec, generate_test_workload_spec_with_param,
            generate_test_workload_state_with_workload_spec, ExecutionState, WorkloadState,
        },
        test_utils::generate_test_deleted_workload,
        to_server_interface::ToServer,
    };
    use tokio::sync::mpsc::channel;

    use super::WorkloadScheduler;
    use crate::{
        parameter_storage::MockParameterStorage,
        workload_operation::WorkloadOperation,
        workload_scheduler::dependency_state_validator::{
            DependencyState, MockDependencyStateValidator,
        },
    };

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const RUNTIME: &str = "runtime";

    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_create_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let pending_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        workload_scheduler
            .insert_and_notify(WorkloadOperation::Create(pending_workload.clone()))
            .await;

        let expected_workload_state = generate_test_workload_state_with_workload_spec(
            &pending_workload,
            ExecutionState::waiting_to_start(),
        );

        assert_eq!(
            Ok(Some(ToServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![expected_workload_state]
            }))),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );
    }

    #[tokio::test]
    #[should_panic]
    async fn utest_report_pending_create_state_closed_receiver() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, workload_state_receiver) = channel(1);
        let workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        drop(workload_state_receiver);

        let pending_workload = generate_test_workload_spec();
        workload_scheduler
            .report_pending_create_state(&pending_workload)
            .await;
    }

    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_deleted_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let pending_deleted_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        workload_scheduler
            .insert_and_notify(WorkloadOperation::Delete(pending_deleted_workload.clone()))
            .await;

        let expected_workload_state = WorkloadState {
            instance_name: pending_deleted_workload.instance_name,
            execution_state: ExecutionState::waiting_to_stop(),
        };

        assert_eq!(
            Ok(Some(ToServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![expected_workload_state]
            }))),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );
    }

    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_updated_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let pending_deleted_workload = generate_test_deleted_workload(
            new_workload.instance_name.agent_name().to_owned(),
            new_workload.instance_name.workload_name().to_owned(),
        );

        let pending_update_operation =
            WorkloadOperation::Update(new_workload, pending_deleted_workload.clone());

        workload_scheduler
            .insert_and_notify(pending_update_operation)
            .await;

        let expected_workload_state = WorkloadState {
            instance_name: pending_deleted_workload.instance_name,
            execution_state: ExecutionState::waiting_to_stop(),
        };

        assert_eq!(
            Ok(Some(ToServer::UpdateWorkloadState(UpdateWorkloadState {
                workload_states: vec![expected_workload_state]
            }))),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );
    }

    #[tokio::test]
    #[should_panic]
    async fn utest_report_pending_delete_state_closed_receiver() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, workload_state_receiver) = channel(1);
        let workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        drop(workload_state_receiver);

        let pending_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        workload_scheduler
            .report_pending_delete_state(&pending_workload)
            .await;
    }

    #[tokio::test]
    async fn utest_enqueue_filtered_workload_operations_do_not_enqueue_ready_operations() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let state_validator_mock_context =
            MockDependencyStateValidator::dependencies_for_workload_fulfilled_context();
        state_validator_mock_context
            .expect()
            .once()
            .return_const(DependencyState::Fulfilled);

        let ready_create_operation =
            WorkloadOperation::Create(generate_test_workload_spec_with_param(
                AGENT_A.to_owned(),
                WORKLOAD_NAME_1.to_owned(),
                RUNTIME.to_owned(),
            ));
        let workload_operations = vec![ready_create_operation.clone()];

        workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockParameterStorage::default(),
            )
            .await;

        assert!(workload_scheduler.queue.get(WORKLOAD_NAME_1).is_none())
    }

    #[tokio::test]
    async fn utest_enqueue_filtered_workload_operations_enqueue_pending_create() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let state_validator_mock_context =
            MockDependencyStateValidator::dependencies_for_workload_fulfilled_context();
        state_validator_mock_context
            .expect()
            .once()
            .return_const(DependencyState::PendingCreate);

        let pending_create_operation =
            WorkloadOperation::Create(generate_test_workload_spec_with_param(
                AGENT_A.to_owned(),
                WORKLOAD_NAME_1.to_owned(),
                RUNTIME.to_owned(),
            ));
        let workload_operations = vec![pending_create_operation.clone()];

        workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockParameterStorage::default(),
            )
            .await;

        let expected_pending_create = &pending_create_operation;
        assert_eq!(
            Some(expected_pending_create),
            workload_scheduler.queue.get(WORKLOAD_NAME_1)
        )
    }

    #[tokio::test]
    async fn utest_enqueue_filtered_workload_operations_enqueue_pending_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let state_validator_mock_context =
            MockDependencyStateValidator::dependencies_for_workload_fulfilled_context();
        state_validator_mock_context
            .expect()
            .once()
            .return_const(DependencyState::PendingDelete);

        let pending_delete_operation = WorkloadOperation::Delete(generate_test_deleted_workload(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
        ));
        let workload_operations = vec![pending_delete_operation.clone()];

        workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockParameterStorage::default(),
            )
            .await;

        let expected_pending_delete = &pending_delete_operation;
        assert_eq!(
            Some(expected_pending_delete),
            workload_scheduler.queue.get(WORKLOAD_NAME_1)
        )
    }

    #[tokio::test]
    async fn utest_enqueue_filtered_workload_operations_enqueues_update_with_pending_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let state_validator_mock_context =
            MockDependencyStateValidator::dependencies_for_workload_fulfilled_context();
        state_validator_mock_context
            .expect()
            .once()
            .return_const(DependencyState::PendingDelete);

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let deleted_workload = generate_test_deleted_workload(
            new_workload.instance_name.agent_name().to_owned(),
            new_workload.instance_name.workload_name().to_owned(),
        );

        let pending_update_operation = WorkloadOperation::Update(new_workload, deleted_workload);
        let workload_operations = vec![pending_update_operation.clone()];

        workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockParameterStorage::default(),
            )
            .await;

        let expected_pending_update = &pending_update_operation;
        assert_eq!(
            Some(expected_pending_update),
            workload_scheduler.queue.get(WORKLOAD_NAME_1)
        )
    }

    #[tokio::test]
    async fn utest_enqueue_filtered_workload_operations_enqueue_create_on_update_with_pending_create(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let state_validator_mock_context =
            MockDependencyStateValidator::dependencies_for_workload_fulfilled_context();
        state_validator_mock_context
            .expect()
            .once()
            .return_const(DependencyState::PendingCreate);

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let deleted_workload = generate_test_deleted_workload(
            new_workload.instance_name.agent_name().to_owned(),
            new_workload.instance_name.workload_name().to_owned(),
        );

        let pending_update_operation =
            WorkloadOperation::Update(new_workload.clone(), deleted_workload);
        let workload_operations = vec![pending_update_operation];

        workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockParameterStorage::default(),
            )
            .await;

        let expected_pending_update = &WorkloadOperation::Create(new_workload);
        assert_eq!(
            Some(expected_pending_update),
            workload_scheduler.queue.get(WORKLOAD_NAME_1)
        )
    }

    #[tokio::test]
    async fn utest_next_workload_operations_not_available() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let state_validator_mock_context =
            MockDependencyStateValidator::dependencies_for_workload_fulfilled_context();
        state_validator_mock_context
            .expect()
            .once()
            .return_const(DependencyState::PendingCreate);

        let pending_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        workload_scheduler.queue.insert(
            pending_workload.instance_name.workload_name().to_owned(),
            WorkloadOperation::Create(pending_workload.clone()),
        );

        let next_workload_operations =
            workload_scheduler.next_workload_operations(&MockParameterStorage::default());

        assert!(next_workload_operations.is_empty());

        let expected_pending_create = &WorkloadOperation::Create(pending_workload);
        assert_eq!(
            Some(expected_pending_create),
            workload_scheduler.queue.get(WORKLOAD_NAME_1)
        )
    }

    #[tokio::test]
    async fn utest_next_workload_operations_available() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let state_validator_mock_context =
            MockDependencyStateValidator::dependencies_for_workload_fulfilled_context();
        state_validator_mock_context
            .expect()
            .once()
            .return_const(DependencyState::Fulfilled);

        let next_ready_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        workload_scheduler.queue.insert(
            next_ready_workload.instance_name.workload_name().to_owned(),
            WorkloadOperation::Create(next_ready_workload.clone()),
        );

        let next_workload_operations =
            workload_scheduler.next_workload_operations(&MockParameterStorage::default());

        let expected_next_operation = WorkloadOperation::Create(next_ready_workload);

        assert_eq!(vec![expected_next_operation], next_workload_operations);
        assert!(workload_scheduler.queue.is_empty());
    }
}

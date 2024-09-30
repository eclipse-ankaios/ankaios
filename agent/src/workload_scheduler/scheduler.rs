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
use crate::{
    workload_operation::ReusableWorkloadSpec,
    workload_state::{WorkloadStateSender, WorkloadStateSenderInterface},
};
use common::objects::{DeletedWorkload, ExecutionState, WorkloadInstanceName, WorkloadSpec};
use std::{collections::HashMap, fmt::Display};

use crate::workload_operation::WorkloadOperation;
#[cfg_attr(test, mockall_double::double)]
use crate::workload_state::workload_state_store::WorkloadStateStore;

#[cfg(test)]
use mockall::automock;

#[derive(Debug, Clone, PartialEq)]
enum PendingEntry {
    Create(ReusableWorkloadSpec),
    Delete(DeletedWorkload),
    UpdateCreate(WorkloadSpec, DeletedWorkload),
    UpdateDelete(WorkloadSpec, DeletedWorkload),
}

type WorkloadOperationQueue = HashMap<String, PendingEntry>;

pub struct WorkloadScheduler {
    queue: WorkloadOperationQueue,
    workload_state_sender: WorkloadStateSender,
}

#[cfg_attr(test, automock)]
impl WorkloadScheduler {
    pub fn new(workload_state_tx: WorkloadStateSender) -> Self {
        WorkloadScheduler {
            queue: WorkloadOperationQueue::new(),
            workload_state_sender: workload_state_tx,
        }
    }

    fn put_on_queue<T>(&mut self, workload_name: T, pending_entry: PendingEntry)
    where
        T: Into<String> + Display + 'static,
    {
        log::debug!("Putting workload '{}' on waiting queue.", workload_name);
        self.queue.insert(workload_name.into(), pending_entry);
    }

    // [impl->swdd~agent-handles-new-workload-operations]
    // [impl->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    pub async fn enqueue_filtered_workload_operations(
        &mut self,
        new_workload_operations: Vec<WorkloadOperation>,
        workload_state_db: &WorkloadStateStore,
    ) -> Vec<WorkloadOperation> {
        let mut ready_workload_operations: Vec<WorkloadOperation> = Vec::new();
        let notify_on_new_entry = true;
        for workload_operation in new_workload_operations {
            match workload_operation {
                WorkloadOperation::Create(new_workload_spec) => {
                    ready_workload_operations.extend(
                        self.enqueue_pending_create(
                            new_workload_spec,
                            workload_state_db,
                            notify_on_new_entry,
                        )
                        .await,
                    );
                }
                WorkloadOperation::Update(new_workload_spec, deleted_workload) => {
                    ready_workload_operations.extend(
                        // [impl->swdd~agent-enqueues-update-with-unfulfilled-delete~1]
                        // [impl->swdd~agent-enqueues-update-with-unfulfilled-delete~1]
                        self.enqueue_pending_update(
                            new_workload_spec,
                            deleted_workload,
                            workload_state_db,
                            notify_on_new_entry,
                        )
                        .await,
                    );
                }
                WorkloadOperation::Delete(deleted_workload) => {
                    ready_workload_operations.extend(
                        self.enqueue_pending_delete(
                            deleted_workload,
                            workload_state_db,
                            notify_on_new_entry,
                        )
                        .await,
                    );
                }
                // [impl->swdd~agent-shall-not-enqueue-update-delete-only-workload-operation~1]
                WorkloadOperation::UpdateDeleteOnly(_) => {
                    log::warn!("Skip UpdateDeleteOnly. This shall never be enqueued.")
                }
            };
        }

        // extend with existing pending update entries of the queue if their dependencies are fulfilled now
        ready_workload_operations.extend(self.next_workload_operations(workload_state_db).await);
        ready_workload_operations
    }

    // [impl->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    // [impl->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    pub async fn next_workload_operations(
        &mut self,
        workload_state_db: &WorkloadStateStore,
    ) -> Vec<WorkloadOperation> {
        // clear the whole queue without deallocating memory
        let queue_entries: Vec<PendingEntry> = self
            .queue
            .drain()
            .map(|(_, pending_workload_operation)| pending_workload_operation)
            .collect();

        // return ready workload operations and enqueue still pending workload operations again
        let mut ready_workload_operations: Vec<WorkloadOperation> = Vec::new();
        let notify_on_new_entry = false;
        for queue_entry in queue_entries {
            match queue_entry {
                PendingEntry::Create(new_workload_spec) => {
                    ready_workload_operations.extend(
                        self.enqueue_pending_create(
                            new_workload_spec,
                            workload_state_db,
                            notify_on_new_entry,
                        )
                        .await,
                    );
                }
                PendingEntry::UpdateCreate(new_workload_spec, deleted_workload) => {
                    // [impl->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
                    if DependencyStateValidator::create_fulfilled(
                        &new_workload_spec,
                        workload_state_db,
                    ) {
                        ready_workload_operations.push(WorkloadOperation::Update(
                            new_workload_spec,
                            deleted_workload,
                        ));
                    } else {
                        self.put_on_queue(
                            new_workload_spec.instance_name.workload_name().to_owned(),
                            PendingEntry::UpdateCreate(new_workload_spec, deleted_workload),
                        );
                    }
                }
                PendingEntry::UpdateDelete(new_workload_spec, deleted_workload) => {
                    ready_workload_operations.extend(
                        self.enqueue_pending_update(
                            new_workload_spec,
                            deleted_workload,
                            workload_state_db,
                            notify_on_new_entry,
                        )
                        .await,
                    );
                }
                PendingEntry::Delete(deleted_workload) => {
                    ready_workload_operations.extend(
                        self.enqueue_pending_delete(
                            deleted_workload,
                            workload_state_db,
                            notify_on_new_entry,
                        )
                        .await,
                    );
                }
            }
        }
        ready_workload_operations
    }

    // [impl->swdd~agent-enqueues-unfulfilled-create~1]
    async fn enqueue_pending_create(
        &mut self,
        new_workload_spec: ReusableWorkloadSpec,
        workload_state_db: &WorkloadStateStore,
        notify_on_new_entry: bool,
    ) -> Vec<WorkloadOperation> {
        let mut ready_workload_operations = Vec::new();
        // [impl->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
        if DependencyStateValidator::create_fulfilled(
            &new_workload_spec.workload_spec,
            workload_state_db,
        ) {
            ready_workload_operations.push(WorkloadOperation::Create(new_workload_spec));
        } else {
            if notify_on_new_entry {
                self.report_pending_create_state(&new_workload_spec.workload_spec.instance_name)
                    .await;
            }

            self.put_on_queue(
                new_workload_spec
                    .workload_spec
                    .instance_name
                    .workload_name()
                    .to_owned(),
                PendingEntry::Create(new_workload_spec),
            );
        }

        ready_workload_operations
    }

    async fn enqueue_pending_update(
        &mut self,
        new_workload_spec: WorkloadSpec,
        deleted_workload: DeletedWorkload,
        workload_state_db: &WorkloadStateStore,
        notify_on_new_entry: bool,
    ) -> Vec<WorkloadOperation> {
        let mut ready_workload_operations = Vec::new();

        // [impl->swdd~workload-ready-to-create-on-fulfilled-dependencies~1]
        let create_fulfilled =
            DependencyStateValidator::create_fulfilled(&new_workload_spec, workload_state_db);

        // [impl->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
        let delete_fulfilled =
            DependencyStateValidator::delete_fulfilled(&deleted_workload, workload_state_db);

        // [impl->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
        if create_fulfilled && delete_fulfilled {
            // dependencies for create and delete are fulfilled, the update can be done immediately
            ready_workload_operations.push(WorkloadOperation::Update(
                new_workload_spec.clone(),
                deleted_workload.clone(),
            ));
            return ready_workload_operations;
        }

        // [impl->swdd~agent-handles-update-with-fulfilled-delete~1]
        if delete_fulfilled {
            /* For an update with pending create dependencies but fulfilled delete dependencies
            the delete can be done immediately but the create must wait in the queue.
            If the create dependencies are already fulfilled the update must wait until the
            old workload is deleted (AT_MOST_ONCE default update strategy) */

            /* once the delete conditions are fulfilled the pending update delete is
            transformed into a pending create since the current update strategy is at most once.
            We notify a pending create state. */
            self.report_pending_create_state(&new_workload_spec.instance_name)
                .await;

            self.put_on_queue(
                new_workload_spec.instance_name.workload_name().to_owned(),
                PendingEntry::UpdateCreate(new_workload_spec, deleted_workload.clone()),
            );

            ready_workload_operations.push(WorkloadOperation::UpdateDeleteOnly(deleted_workload));
        } else {
            // [impl->swdd~agent-enqueues-update-with-unfulfilled-delete~1]

            // For an update with pending delete dependencies, the whole update is pending.
            if notify_on_new_entry {
                self.report_pending_delete_state(&deleted_workload.instance_name)
                    .await;
            }

            self.put_on_queue(
                new_workload_spec.instance_name.workload_name().to_owned(),
                PendingEntry::UpdateDelete(new_workload_spec, deleted_workload),
            );
        }
        ready_workload_operations
    }

    // [impl->swdd~agent-enqueues-unfulfilled-delete~1]
    async fn enqueue_pending_delete(
        &mut self,
        deleted_workload: DeletedWorkload,
        workload_state_db: &WorkloadStateStore,
        notify_on_new_entry: bool,
    ) -> Vec<WorkloadOperation> {
        let mut ready_workload_operations = Vec::new();

        // [impl->swdd~workload-ready-to-delete-on-fulfilled-dependencies~1]
        if DependencyStateValidator::delete_fulfilled(&deleted_workload, workload_state_db) {
            ready_workload_operations.push(WorkloadOperation::Delete(deleted_workload));
        } else {
            if notify_on_new_entry {
                self.report_pending_delete_state(&deleted_workload.instance_name)
                    .await;
            }

            self.put_on_queue(
                deleted_workload.instance_name.workload_name().to_owned(),
                PendingEntry::Delete(deleted_workload),
            );
        }

        ready_workload_operations
    }

    async fn report_pending_create_state(&self, instance_name: &WorkloadInstanceName) {
        self.workload_state_sender
            .report_workload_execution_state(instance_name, ExecutionState::waiting_to_start())
            .await;
    }

    async fn report_pending_delete_state(&self, instance_name: &WorkloadInstanceName) {
        self.workload_state_sender
            .report_workload_execution_state(instance_name, ExecutionState::waiting_to_stop())
            .await;
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
        objects::{
            generate_test_workload_spec, generate_test_workload_spec_with_param,
            generate_test_workload_state_with_workload_spec, ExecutionState, WorkloadState,
        },
        test_utils::generate_test_deleted_workload,
    };
    use tokio::sync::mpsc::channel;

    use super::WorkloadScheduler;
    use crate::{
        workload_operation::{ReusableWorkloadSpec, WorkloadOperation},
        workload_scheduler::{
            dependency_state_validator::MockDependencyStateValidator, scheduler::PendingEntry,
        },
        workload_state::{
            assert_execution_state_sequence, workload_state_store::MockWorkloadStateStore,
        },
    };

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const RUNTIME: &str = "runtime";

    // [utest->swdd~agent-handles-new-workload-operations]
    // [utest->swdd~agent-enqueues-unfulfilled-create~1]
    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_create() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_context
            .expect()
            .return_const(false);

        let pending_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let pending_reusable_workload = ReusableWorkloadSpec::new(pending_workload, None);

        let workload_operations =
            vec![WorkloadOperation::Create(pending_reusable_workload.clone())];

        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        let expected_workload_state = generate_test_workload_state_with_workload_spec(
            &pending_reusable_workload.workload_spec.clone(),
            ExecutionState::waiting_to_start(),
        );

        assert_eq!(
            Ok(Some(expected_workload_state)),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );

        assert!(workload_scheduler.queue.contains_key(
            pending_reusable_workload
                .workload_spec
                .instance_name
                .workload_name()
        ));

        assert!(ready_workload_operations.is_empty());
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_no_enqueue_and_report_for_ready_create() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_context
            .expect()
            .return_const(true);

        let ready_workload = ReusableWorkloadSpec::new(
            generate_test_workload_spec_with_param(
                AGENT_A.to_owned(),
                WORKLOAD_NAME_1.to_owned(),
                RUNTIME.to_owned(),
            ),
            None,
        );

        let workload_operations = vec![WorkloadOperation::Create(ready_workload.clone())];

        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert_eq!(
            vec![WorkloadOperation::Create(ready_workload)],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());
        assert!(workload_state_receiver.try_recv().is_err());
    }

    // [utest->swdd~agent-enqueues-unfulfilled-create~1]
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
            .report_pending_create_state(&pending_workload.instance_name)
            .await;
    }

    // [utest->swdd~agent-handles-new-workload-operations]
    // [utest->swdd~agent-enqueues-unfulfilled-delete~1]
    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_context
            .expect()
            .return_const(false);

        let pending_deleted_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        let workload_operations = vec![WorkloadOperation::Delete(pending_deleted_workload.clone())];
        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert!(ready_workload_operations.is_empty());

        assert_eq!(
            Some(&PendingEntry::Delete(pending_deleted_workload.clone())),
            workload_scheduler
                .queue
                .get(pending_deleted_workload.instance_name.workload_name())
        );

        let expected_workload_state = WorkloadState {
            instance_name: pending_deleted_workload.instance_name,
            execution_state: ExecutionState::waiting_to_stop(),
        };

        assert_eq!(
            Ok(Some(expected_workload_state)),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_no_enqueue_and_report_workload_state_for_ready_delete() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_context
            .expect()
            .return_const(true);

        let ready_deleted_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        let workload_operations = vec![WorkloadOperation::Delete(ready_deleted_workload.clone())];
        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert_eq!(
            vec![WorkloadOperation::Delete(ready_deleted_workload)],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());

        assert!(workload_state_receiver.try_recv().is_err());
    }

    // [utest->swdd~agent-enqueues-unfulfilled-delete~1]
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
            .report_pending_delete_state(&pending_workload.instance_name)
            .await;
    }

    // [utest->swdd~agent-handles-new-workload-operations]
    // [utest->swdd~agent-enqueues-update-with-unfulfilled-delete~1]
    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_update_delete_at_most_once() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(true);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(false);

        let ready_new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let pending_deleted_workload = generate_test_deleted_workload(
            ready_new_workload.instance_name.agent_name().to_owned(),
            ready_new_workload.instance_name.workload_name().to_owned(),
        );

        let workload_operations = vec![WorkloadOperation::Update(
            ready_new_workload.clone(),
            pending_deleted_workload.clone(),
        )];
        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert!(ready_workload_operations.is_empty());

        assert_eq!(
            Some(&PendingEntry::UpdateDelete(
                ready_new_workload.clone(),
                pending_deleted_workload.clone()
            )),
            workload_scheduler
                .queue
                .get(pending_deleted_workload.instance_name.workload_name())
        );

        let expected_workload_state = WorkloadState {
            instance_name: pending_deleted_workload.instance_name,
            execution_state: ExecutionState::waiting_to_stop(),
        };

        assert_eq!(
            Ok(Some(expected_workload_state)),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );
    }

    // [utest->swdd~agent-handles-new-workload-operations]
    // [utest->swdd~agent-enqueues-update-with-unfulfilled-delete~1]
    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_update_at_most_once() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(false);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(false);

        let ready_new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let pending_deleted_workload = generate_test_deleted_workload(
            ready_new_workload.instance_name.agent_name().to_owned(),
            ready_new_workload.instance_name.workload_name().to_owned(),
        );

        let workload_operations = vec![WorkloadOperation::Update(
            ready_new_workload.clone(),
            pending_deleted_workload.clone(),
        )];
        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert!(ready_workload_operations.is_empty());

        assert_eq!(
            Some(&PendingEntry::UpdateDelete(
                ready_new_workload.clone(),
                pending_deleted_workload.clone()
            )),
            workload_scheduler
                .queue
                .get(pending_deleted_workload.instance_name.workload_name())
        );

        let expected_workload_state = WorkloadState {
            instance_name: pending_deleted_workload.instance_name,
            execution_state: ExecutionState::waiting_to_stop(),
        };

        assert_eq!(
            Ok(Some(expected_workload_state)),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );
    }

    // [utest->swdd~agent-handles-new-workload-operations]
    // [utest->swdd~agent-handles-update-with-fulfilled-delete~1]
    #[tokio::test]
    async fn utest_enqueue_and_report_workload_state_for_pending_update_create_at_most_once() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(false);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let pending_new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let ready_deleted_workload = generate_test_deleted_workload(
            pending_new_workload.instance_name.agent_name().to_owned(),
            pending_new_workload
                .instance_name
                .workload_name()
                .to_owned(),
        );

        let workload_operations = vec![WorkloadOperation::Update(
            pending_new_workload.clone(),
            ready_deleted_workload.clone(),
        )];

        workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert_eq!(
            Some(&PendingEntry::UpdateCreate(
                pending_new_workload.clone(),
                ready_deleted_workload.clone()
            )),
            workload_scheduler
                .queue
                .get(pending_new_workload.instance_name.workload_name())
        );

        let expected_workload_state = WorkloadState {
            instance_name: pending_new_workload.instance_name,
            execution_state: ExecutionState::waiting_to_start(),
        };

        assert_eq!(
            Ok(Some(expected_workload_state)),
            tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                workload_state_receiver.recv()
            )
            .await
        );
    }

    // [utest->swdd~agent-handles-update-with-fulfilled-delete~1]
    #[tokio::test]
    async fn utest_immediate_delete_for_pending_update_create_at_most_once() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(false);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let pending_new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let ready_deleted_workload = generate_test_deleted_workload(
            pending_new_workload.instance_name.agent_name().to_owned(),
            pending_new_workload
                .instance_name
                .workload_name()
                .to_owned(),
        );

        let workload_operations = vec![WorkloadOperation::Update(
            pending_new_workload,
            ready_deleted_workload.clone(),
        )];

        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert_eq!(
            vec![WorkloadOperation::UpdateDeleteOnly(ready_deleted_workload)],
            ready_workload_operations
        );
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_no_enqueue_and_report_pending_state_on_fulfilled_update_at_most_once() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(true);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let ready_new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let ready_deleted_workload = generate_test_deleted_workload(
            ready_new_workload.instance_name.agent_name().to_owned(),
            ready_new_workload.instance_name.workload_name().to_owned(),
        );

        let workload_operations = vec![WorkloadOperation::Update(
            ready_new_workload.clone(),
            ready_deleted_workload.clone(),
        )];
        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert_eq!(
            vec![WorkloadOperation::Update(
                ready_new_workload,
                ready_deleted_workload
            )],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());

        assert!(workload_state_receiver.try_recv().is_err());
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_enqueue_filtered_workload_operations_get_next_ready_workload_operations() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .once()
            .return_const(true);

        let ready_new_workload = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let ready_deleted_workload = generate_test_deleted_workload(
            ready_new_workload.instance_name.agent_name().to_owned(),
            ready_new_workload.instance_name.workload_name().to_owned(),
        );

        let workload_operations = vec![];

        workload_scheduler.queue.insert(
            ready_new_workload.instance_name.workload_name().to_owned(),
            PendingEntry::UpdateCreate(ready_new_workload.clone(), ready_deleted_workload.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert_eq!(
            vec![WorkloadOperation::Update(
                ready_new_workload,
                ready_deleted_workload
            )],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());
    }

    // [utest->swdd~agent-shall-not-enqueue-update-delete-only-workload-operation~1]
    #[tokio::test]
    async fn utest_enqueue_filtered_workload_operations_ignore_update_delete_only_workload_operations(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let ready_deleted_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        let workload_operations = vec![WorkloadOperation::UpdateDeleteOnly(
            ready_deleted_workload.clone(),
        )];

        let ready_workload_operations = workload_scheduler
            .enqueue_filtered_workload_operations(
                workload_operations,
                &MockWorkloadStateStore::default(),
            )
            .await;

        assert!(ready_workload_operations.is_empty());

        assert!(workload_scheduler.queue.is_empty());
    }

    // [utest->swdd~agent-handles-update-with-fulfilled-delete~1]
    #[tokio::test]
    async fn utest_next_workload_operations_enqueue_pending_update_create_on_delete_fulfilled_update(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .once()
            .return_const(false);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let new_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name_new_workload = new_workload_spec.instance_name.clone();

        let ready_deleted_workload = generate_test_deleted_workload(
            instance_name_new_workload.agent_name().to_owned(),
            instance_name_new_workload.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name_new_workload.workload_name().to_owned(),
            PendingEntry::UpdateDelete(new_workload_spec.clone(), ready_deleted_workload.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert_eq!(
            vec![WorkloadOperation::UpdateDeleteOnly(
                ready_deleted_workload.clone()
            )],
            ready_workload_operations
        );

        assert_eq!(
            Some(&PendingEntry::UpdateCreate(
                new_workload_spec,
                ready_deleted_workload
            )),
            workload_scheduler
                .queue
                .get(instance_name_new_workload.workload_name())
        );
    }

    // [utest->swdd~agent-handles-update-with-fulfilled-delete~1]
    #[tokio::test]
    async fn utest_next_workload_operations_report_pending_create_on_delete_fulfilled_update() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .once()
            .return_const(false);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let new_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name_new_workload = new_workload_spec.instance_name.clone();

        let ready_deleted_workload = generate_test_deleted_workload(
            instance_name_new_workload.agent_name().to_owned(),
            instance_name_new_workload.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name_new_workload.workload_name().to_owned(),
            PendingEntry::UpdateDelete(new_workload_spec.clone(), ready_deleted_workload.clone()),
        );

        workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert_execution_state_sequence(
            workload_state_receiver,
            vec![(
                &instance_name_new_workload,
                ExecutionState::waiting_to_start(),
            )],
        )
        .await;
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    // [utest->swdd~agent-enqueues-unfulfilled-delete~1]
    #[tokio::test]
    async fn utest_next_workload_operations_keep_pending_delete_in_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(false);

        let pending_deleted_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        let instance_name_deleted_workload = pending_deleted_workload.instance_name.clone();

        workload_scheduler.queue.insert(
            instance_name_deleted_workload.workload_name().to_owned(),
            PendingEntry::Delete(pending_deleted_workload.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(ready_workload_operations.is_empty());

        assert!(workload_scheduler
            .queue
            .contains_key(instance_name_deleted_workload.workload_name()));
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    #[tokio::test]
    async fn utest_next_workload_operations_no_report_pending_delete_on_reenqueue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(false);

        let pending_deleted_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        let instance_name_deleted_workload = pending_deleted_workload.instance_name.clone();

        workload_scheduler.queue.insert(
            instance_name_deleted_workload.workload_name().to_owned(),
            PendingEntry::Delete(pending_deleted_workload.clone()),
        );

        workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(workload_state_receiver.try_recv().is_err());
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    #[tokio::test]
    async fn utest_next_workload_operations_keep_pending_create_in_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(false);

        let pending_workload_spec = ReusableWorkloadSpec::new(
            generate_test_workload_spec_with_param(
                AGENT_A.to_owned(),
                WORKLOAD_NAME_1.to_owned(),
                RUNTIME.to_owned(),
            ),
            None,
        );

        let instance_name_create_workload =
            pending_workload_spec.workload_spec.instance_name.clone();

        workload_scheduler.queue.insert(
            instance_name_create_workload.workload_name().to_owned(),
            PendingEntry::Create(pending_workload_spec.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(ready_workload_operations.is_empty());

        assert!(workload_scheduler
            .queue
            .contains_key(instance_name_create_workload.workload_name()));
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    #[tokio::test]
    async fn utest_next_workload_operations_no_report_pending_create_on_reenqueue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(false);

        let pending_workload_spec = ReusableWorkloadSpec::new(
            generate_test_workload_spec_with_param(
                AGENT_A.to_owned(),
                WORKLOAD_NAME_1.to_owned(),
                RUNTIME.to_owned(),
            ),
            None,
        );

        let instance_name_create_workload =
            pending_workload_spec.workload_spec.instance_name.clone();

        workload_scheduler.queue.insert(
            instance_name_create_workload.workload_name().to_owned(),
            PendingEntry::Create(pending_workload_spec.clone()),
        );

        workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(workload_state_receiver.try_recv().is_err());
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    #[tokio::test]
    async fn utest_next_workload_operations_keep_pending_update_in_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(true);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(false);

        let ready_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name = ready_workload_spec.instance_name.clone();

        let pending_deleted_workload = generate_test_deleted_workload(
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name.workload_name().to_owned(),
            PendingEntry::UpdateDelete(
                ready_workload_spec.clone(),
                pending_deleted_workload.clone(),
            ),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(ready_workload_operations.is_empty());

        assert!(workload_scheduler
            .queue
            .contains_key(instance_name.workload_name()));
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    #[tokio::test]
    async fn utest_next_workload_operations_no_report_pending_delete_on_pending_update_reenqueue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(true);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(false);

        let ready_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name = ready_workload_spec.instance_name.clone();

        let pending_deleted_workload = generate_test_deleted_workload(
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name.workload_name().to_owned(),
            PendingEntry::UpdateDelete(
                ready_workload_spec.clone(),
                pending_deleted_workload.clone(),
            ),
        );

        workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(workload_state_receiver.try_recv().is_err());
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    #[tokio::test]
    async fn utest_next_workload_operations_keep_pending_update_create_in_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(false);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let ready_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name = ready_workload_spec.instance_name.clone();

        let pending_deleted_workload = generate_test_deleted_workload(
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name.workload_name().to_owned(),
            PendingEntry::UpdateCreate(
                ready_workload_spec.clone(),
                pending_deleted_workload.clone(),
            ),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(ready_workload_operations.is_empty());

        assert!(workload_scheduler
            .queue
            .contains_key(instance_name.workload_name()));
    }

    // [utest->swdd~agent-keeps-workloads-with-unfulfilled-workload-dependencies-in-queue~1]
    #[tokio::test]
    async fn utest_next_workload_operations_no_report_pending_create_on_pending_update_create_reenqueue(
    ) {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, mut workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(false);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let ready_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name = ready_workload_spec.instance_name.clone();

        let pending_deleted_workload = generate_test_deleted_workload(
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name.workload_name().to_owned(),
            PendingEntry::UpdateCreate(
                ready_workload_spec.clone(),
                pending_deleted_workload.clone(),
            ),
        );

        workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert!(workload_state_receiver.try_recv().is_err());
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_next_workload_operations_remove_ready_create_from_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(true);

        let ready_workload_spec = ReusableWorkloadSpec::new(
            generate_test_workload_spec_with_param(
                AGENT_A.to_owned(),
                WORKLOAD_NAME_1.to_owned(),
                RUNTIME.to_owned(),
            ),
            None,
        );

        workload_scheduler.queue.insert(
            ready_workload_spec
                .workload_spec
                .instance_name
                .workload_name()
                .to_owned(),
            PendingEntry::Create(ready_workload_spec.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert_eq!(
            vec![WorkloadOperation::Create(ready_workload_spec)],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_next_workload_operations_remove_ready_delete_from_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let ready_deleted_workload =
            generate_test_deleted_workload(AGENT_A.to_owned(), WORKLOAD_NAME_1.to_owned());

        workload_scheduler.queue.insert(
            ready_deleted_workload
                .instance_name
                .workload_name()
                .to_owned(),
            PendingEntry::Delete(ready_deleted_workload.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert_eq!(
            vec![WorkloadOperation::Delete(ready_deleted_workload)],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_next_workload_operations_remove_ready_update_create_at_most_once_from_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(true);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let ready_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name = ready_workload_spec.instance_name.clone();

        let ready_deleted_workload = generate_test_deleted_workload(
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name.workload_name().to_owned(),
            PendingEntry::UpdateCreate(ready_workload_spec.clone(), ready_deleted_workload.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert_eq!(
            vec![WorkloadOperation::Update(
                ready_workload_spec,
                ready_deleted_workload
            )],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());
    }

    // [utest->swdd~agent-handles-workloads-with-fulfilled-dependencies~1]
    #[tokio::test]
    async fn utest_next_workload_operations_remove_ready_update_delete_at_most_once_from_queue() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let (workload_state_sender, _workload_state_receiver) = channel(1);
        let mut workload_scheduler = WorkloadScheduler::new(workload_state_sender);

        let mock_dependency_state_validator_create_context =
            MockDependencyStateValidator::create_fulfilled_context();
        mock_dependency_state_validator_create_context
            .expect()
            .return_const(true);

        let mock_dependency_state_validator_delete_context =
            MockDependencyStateValidator::delete_fulfilled_context();
        mock_dependency_state_validator_delete_context
            .expect()
            .return_const(true);

        let ready_workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
        );

        let instance_name = ready_workload_spec.instance_name.clone();

        let ready_deleted_workload = generate_test_deleted_workload(
            instance_name.agent_name().to_owned(),
            instance_name.workload_name().to_owned(),
        );

        workload_scheduler.queue.insert(
            instance_name.workload_name().to_owned(),
            PendingEntry::UpdateDelete(ready_workload_spec.clone(), ready_deleted_workload.clone()),
        );

        let ready_workload_operations = workload_scheduler
            .next_workload_operations(&MockWorkloadStateStore::default())
            .await;

        assert_eq!(
            vec![WorkloadOperation::Update(
                ready_workload_spec,
                ready_deleted_workload
            )],
            ready_workload_operations
        );

        assert!(workload_scheduler.queue.is_empty());
    }
}

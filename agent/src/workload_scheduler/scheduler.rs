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
use crate::parameter_storage::ParameterStorage;

#[cfg_attr(test, mockall_double::double)]
use super::workload_queue::WorkloadQueue;
use super::workload_queue::{ReadyDeletedWorkloads, ReadyWorkloads};
use common::{
    objects::{DeletedWorkload, ExecutionState, WorkloadSpec, WorkloadState},
    std_extensions::IllegalStateResult,
    to_server_interface::{ToServerInterface, ToServerSender},
};

#[cfg(test)]
use mockall::automock;

pub struct WorkloadScheduler {
    workload_queue: WorkloadQueue,
    workload_state_sender: ToServerSender,
}

#[cfg_attr(test, automock)]
impl WorkloadScheduler {
    pub fn new(workload_state_tx: ToServerSender) -> Self {
        WorkloadScheduler {
            workload_queue: WorkloadQueue::new(),
            workload_state_sender: workload_state_tx,
        }
    }

    async fn report_pending_state_for_waiting_workloads(&self, waiting_workloads: &[WorkloadSpec]) {
        for workload in waiting_workloads.iter() {
            self.workload_state_sender
                .update_workload_state(vec![WorkloadState {
                    instance_name: workload.instance_name.clone(),
                    execution_state: ExecutionState::waiting_to_start(),
                }])
                .await
                .unwrap_or_illegal_state();
        }
    }

    async fn report_pending_delete_state_for_waiting_workloads(
        &self,
        waiting_workloads: &[DeletedWorkload],
    ) {
        for workload in waiting_workloads.iter() {
            self.workload_state_sender
                .update_workload_state(vec![WorkloadState {
                    instance_name: workload.instance_name.clone(),
                    execution_state: ExecutionState::waiting_to_stop(),
                }])
                .await
                .unwrap_or_illegal_state();
        }
    }

    pub async fn schedule_workloads(
        &mut self,
        added_workloads: Vec<WorkloadSpec>,
        deleted_workloads: Vec<DeletedWorkload>,
        workload_state_db: &ParameterStorage,
    ) -> (ReadyWorkloads, ReadyDeletedWorkloads) {
        let (ready_workloads, waiting_workloads) =
            WorkloadQueue::split_workloads_to_ready_and_waiting(added_workloads, workload_state_db);

        self.report_pending_state_for_waiting_workloads(&waiting_workloads)
            .await;

        self.workload_queue.put_on_waiting_queue(waiting_workloads);

        let (ready_deleted_workloads, waiting_deleted_workloads) =
            WorkloadQueue::split_deleted_workloads_to_ready_and_waiting(
                deleted_workloads,
                workload_state_db,
            );

        self.report_pending_delete_state_for_waiting_workloads(&waiting_deleted_workloads)
            .await;

        self.workload_queue
            .put_on_delete_waiting_queue(waiting_deleted_workloads);

        (ready_workloads, ready_deleted_workloads)
    }

    pub fn next_added_and_deleted_workloads(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) -> (ReadyWorkloads, ReadyDeletedWorkloads) {
        (
            self.workload_queue
                .next_workloads_to_start(workload_state_db),
            self.workload_queue
                .next_workloads_to_delete(workload_state_db),
        )
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
mod tests {}

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

use common::objects::{DeletedWorkload, FulfilledBy, WorkloadSpec};

use std::collections::HashMap;

use crate::parameter_storage::ParameterStorage;

pub type ReadyWorkloads = Vec<WorkloadSpec>;
pub type WaitingWorkloads = Vec<WorkloadSpec>;
pub type ReadyDeletedWorkloads = Vec<DeletedWorkload>;
pub type WaitingDeletedWorkloads = Vec<DeletedWorkload>;

type StartWorkloadQueue = HashMap<String, WorkloadSpec>;
type DeleteWorkloadQueue = HashMap<String, DeletedWorkload>;

pub struct DependencyScheduler {
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

impl DependencyScheduler {
    pub fn new() -> Self {
        DependencyScheduler {
            start_queue: StartWorkloadQueue::new(),
            delete_queue: DeleteWorkloadQueue::new(),
        }
    }

    pub fn split_workloads_to_ready_and_waiting(
        new_workloads: Vec<WorkloadSpec>,
    ) -> (ReadyWorkloads, WaitingWorkloads) {
        let (ready_to_start_workloads, waiting_to_start_workloads) =
            split_by_condition(new_workloads, |workload| workload.dependencies.is_empty());
        (ready_to_start_workloads, waiting_to_start_workloads)
    }

    pub fn put_on_waiting_queue(&mut self, workloads: WaitingWorkloads) {
        self.start_queue.extend(
            workloads
                .into_iter()
                .map(|workload| (workload.name.clone(), workload)),
        );
    }

    pub fn split_deleted_workloads_to_ready_and_waiting(
        deleted_workloads: Vec<DeletedWorkload>,
        workload_state_db: &ParameterStorage,
    ) -> (ReadyDeletedWorkloads, WaitingDeletedWorkloads) {
        let (ready_to_delete_workloads, waiting_to_delete_workloads) =
            split_by_condition(deleted_workloads, |workload| {
                workload
                    .dependencies
                    .iter()
                    .all(|(dependency_name, delete_condition)| {
                        if let Some(wl_state) =
                            workload_state_db.get_workload_state(dependency_name)
                        {
                            delete_condition.fulfilled_by(wl_state)
                        } else {
                            false
                        }
                    })
            });

        (ready_to_delete_workloads, waiting_to_delete_workloads)
    }

    pub fn put_on_delete_waiting_queue(&mut self, workloads: WaitingDeletedWorkloads) {
        self.delete_queue.extend(
            workloads
                .into_iter()
                .map(|workload| (workload.name.clone(), workload)),
        );
    }

    pub fn next_workloads_to_start(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) -> ReadyWorkloads {
        let ready_workloads: ReadyWorkloads = self
            .start_queue
            .values()
            .filter_map(|workload_spec| {
                workload_spec
                    .dependencies
                    .iter()
                    .all(|(dependency_name, add_condition)| {
                        if let Some(wl_state) =
                            workload_state_db.get_workload_state(dependency_name)
                        {
                            add_condition.fulfilled_by(wl_state)
                        } else {
                            false
                        }
                    })
                    .then_some(workload_spec.clone())
            })
            .collect();

        for workload in ready_workloads.iter() {
            self.start_queue.remove(&workload.name);
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
                deleted_workload
                    .dependencies
                    .iter()
                    .all(|(dependency_name, delete_condition)| {
                        if let Some(wl_state) =
                            workload_state_db.get_workload_state(dependency_name)
                        {
                            delete_condition.fulfilled_by(wl_state)
                        } else {
                            false
                        }
                    })
                    .then_some(deleted_workload.clone())
            })
            .collect();

        for workload in ready_workloads.iter() {
            self.delete_queue.remove(&workload.name);
        }
        ready_workloads
    }
}

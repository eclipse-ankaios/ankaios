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

use common::objects::{DeleteCondition, DeletedWorkload, ExecutionState, WorkloadSpec};

use std::collections::HashMap;

use crate::parameter_storage::ParameterStorage;

type StartWorkloadQueue = HashMap<String, WorkloadSpec>;
type StopWorkloadQueue = HashMap<String, DeletedWorkload>;

pub struct DependencyScheduler {
    start_queue: StartWorkloadQueue,
    delete_queue: StopWorkloadQueue,
}

impl DependencyScheduler {
    pub fn new() -> Self {
        DependencyScheduler {
            start_queue: StartWorkloadQueue::new(),
            delete_queue: StopWorkloadQueue::new(),
        }
    }

    pub fn schedule_start(&mut self, mut new_workloads: Vec<WorkloadSpec>) -> Vec<WorkloadSpec> {
        self.start_queue.extend(
            new_workloads
                .iter()
                .filter(|workload| !workload.dependencies.is_empty())
                .map(|w| (w.name.clone(), w.clone())),
        );
        new_workloads.retain(|workload| workload.dependencies.is_empty());
        new_workloads
    }

    pub fn schedule_stop(
        &mut self,
        mut deleted_workloads: Vec<DeletedWorkload>,
    ) -> Vec<DeletedWorkload> {
        self.delete_queue.extend(
            deleted_workloads
                .iter()
                .filter(|workload| !workload.dependencies.is_empty())
                .map(|w| (w.name.clone(), w.clone())),
        );

        deleted_workloads.retain(|workload| workload.dependencies.is_empty());
        deleted_workloads
    }

    pub fn next_workloads_to_start(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) -> Vec<WorkloadSpec> {
        let mut ready_workloads = Vec::new();
        for workload_spec in self.start_queue.values() {
            if workload_spec
                .dependencies
                .iter()
                .all(|(dependency_name, add_condition)| {
                    if let Some(wl_state) = workload_state_db.get_workload_state(dependency_name) {
                        *wl_state == (*add_condition).into()
                    } else {
                        false
                    }
                })
            {
                ready_workloads.push(workload_spec.clone());
            }
        }

        for workload in ready_workloads.iter() {
            self.start_queue.remove(&workload.name);
        }

        ready_workloads
    }

    pub fn next_workloads_to_delete(
        &mut self,
        workload_state_db: &ParameterStorage,
    ) -> Vec<DeletedWorkload> {
        let mut ready_workloads = Vec::new();
        for deleted_workload in self.delete_queue.values() {
            if deleted_workload
                .dependencies
                .iter()
                .all(|(dependency_name, delete_condition)| {
                    if let Some(wl_state) = workload_state_db.get_workload_state(dependency_name) {
                        (*delete_condition == DeleteCondition::DelCondNotPendingNorRunning
                            && *wl_state != ExecutionState::ExecPending
                            && *wl_state != ExecutionState::ExecRunning)
                            || (*delete_condition == DeleteCondition::DelCondRunning
                                && *wl_state == ExecutionState::ExecRunning)
                    } else {
                        false
                    }
                })
            {
                ready_workloads.push(deleted_workload.clone());
            }
        }

        for workload in ready_workloads.iter() {
            self.delete_queue.remove(&workload.name);
        }
        ready_workloads
    }
}

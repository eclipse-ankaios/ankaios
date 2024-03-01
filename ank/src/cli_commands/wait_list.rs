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

use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    io::{self, Write},
};

use common::{
    commands::UpdateStateSuccess,
    objects::{WorkloadInstanceName, WorkloadState},
};

use crate::{print, println};

pub struct WaitList {
    pub added_workloads: HashSet<WorkloadInstanceName>,
    pub deleted_workloads: HashSet<WorkloadInstanceName>,
}

impl WaitList {
    pub fn new(value: UpdateStateSuccess) -> Result<Self, String> {
        let mut workloads =
            Vec::with_capacity(value.added_workloads.len() + value.deleted_workloads.len());
        workloads.append(
            &mut value
                .added_workloads
                .iter()
                .cloned()
                .map(|x| format!("starting {x}"))
                .collect(),
        );
        workloads.append(
            &mut value
                .deleted_workloads
                .iter()
                .cloned()
                .map(|x| format!("stopping {x}"))
                .collect(),
        );

        Ok(Self {
            added_workloads: value
                .added_workloads
                .iter()
                .map(|x| WorkloadInstanceName::try_from(x.as_ref()))
                .collect::<Result<_, String>>()?,

            deleted_workloads: value
                .deleted_workloads
                .iter()
                .map(|x| WorkloadInstanceName::try_from(x.as_ref()))
                .collect::<Result<_, String>>()?,
        })
    }

    pub fn update(&mut self, values: impl IntoIterator<Item = WorkloadState>) {
        for workload in values.into_iter() {
            match workload.execution_state.state {
                common::objects::ExecutionStateEnum::Running(_) => {
                    self.remove_added(workload.instance_name)
                }
                common::objects::ExecutionStateEnum::Succeeded(_) => {
                    self.remove_added(workload.instance_name)
                }
                common::objects::ExecutionStateEnum::Failed(_) => {
                    self.remove_added(workload.instance_name)
                }
                common::objects::ExecutionStateEnum::Removed => {
                    self.remove_deleted(workload.instance_name)
                }
                _ => {}
            };
        }
    }

    fn remove_added(&mut self, name: WorkloadInstanceName) {
        if self.added_workloads.remove(&name) {
            println!("Workload {name} started");
        }
    }

    fn remove_deleted(&mut self, name: WorkloadInstanceName) {
        if self.deleted_workloads.remove(&name) {
            println!("Workload {name} removed");
        }
    }

    pub fn is_empty(&self) -> bool {
        self.added_workloads.is_empty() && self.deleted_workloads.is_empty()
    }
}

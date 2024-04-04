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

use std::{collections::HashSet, fmt::Display};

use common::{
    commands::UpdateStateSuccess,
    objects::{WorkloadInstanceName, WorkloadState},
};

use crate::output_update;

pub struct ParsedUpdateStateSuccess {
    pub added_workloads: Vec<WorkloadInstanceName>,
    pub deleted_workloads: Vec<WorkloadInstanceName>,
}

impl TryFrom<UpdateStateSuccess> for ParsedUpdateStateSuccess {
    type Error = String;

    fn try_from(value: UpdateStateSuccess) -> Result<Self, Self::Error> {
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
}

pub trait WaitListDisplayTrait: Display {
    fn update(&mut self, workload_state: &WorkloadState);
    fn set_complete(&mut self, workload: &WorkloadInstanceName);
    fn step_spinner(&mut self);
}

pub struct WaitList<T> {
    pub added_workloads: HashSet<WorkloadInstanceName>,
    pub deleted_workloads: HashSet<WorkloadInstanceName>,
    display: T,
}

impl<T: WaitListDisplayTrait> WaitList<T> {
    pub fn new(value: ParsedUpdateStateSuccess, display: T) -> Self {
        Self {
            added_workloads: value.added_workloads.into_iter().collect(),
            deleted_workloads: value.deleted_workloads.into_iter().collect(),
            display,
        }
    }

    pub fn update(&mut self, values: impl IntoIterator<Item = WorkloadState>) {
        for workload in values.into_iter() {
            self.display.update(&workload);
            // [impl->swdd~cli-checks-for-final-workload-state~1]
            match workload.execution_state.state {
                common::objects::ExecutionStateEnum::Running(_) => {
                    if self.added_workloads.remove(&workload.instance_name) {
                        self.display.set_complete(&workload.instance_name)
                    }
                }
                common::objects::ExecutionStateEnum::Succeeded(_) => {
                    if self.added_workloads.remove(&workload.instance_name) {
                        self.display.set_complete(&workload.instance_name)
                    }
                }
                common::objects::ExecutionStateEnum::Failed(_) => {
                    if self.added_workloads.remove(&workload.instance_name) {
                        self.display.set_complete(&workload.instance_name)
                    }
                }
                common::objects::ExecutionStateEnum::Removed => {
                    if self.deleted_workloads.remove(&workload.instance_name) {
                        self.display.set_complete(&workload.instance_name)
                    }
                }
                _ => {}
            };
        }

        output_update!("{}", &self.display);
    }

    pub fn step_spinner(&mut self) {
        self.display.step_spinner();
        output_update!("{}", &self.display);
    }

    pub fn is_empty(&self) -> bool {
        self.added_workloads.is_empty() && self.deleted_workloads.is_empty()
    }
}

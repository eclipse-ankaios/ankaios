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

use common::objects::{DeletedWorkload, WorkloadSpec};

#[derive(Debug, Clone, PartialEq)]
pub struct ReusableWorkloadSpec {
    pub workload_spec: WorkloadSpec,
    pub workload_id: Option<String>,
}

impl ReusableWorkloadSpec {
    pub fn new(workload_spec: WorkloadSpec, workload_id: Option<String>) -> ReusableWorkloadSpec {
        ReusableWorkloadSpec {
            workload_spec,
            workload_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
// [impl->swdd~agent-transforms-update-workload-message-to-workload-operations~1]
pub enum WorkloadOperation {
    Create(ReusableWorkloadSpec),
    Update(WorkloadSpec, DeletedWorkload),
    UpdateDeleteOnly(DeletedWorkload),
    Delete(DeletedWorkload),
}

// Copyright (c) 2023 Elektrobit Automotive GmbH
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

use ankaios_api::ank_base::{
    CpuUsageSpec, DeletedWorkload, FreeMemorySpec, Tags, WorkloadNamed, WorkloadStateSpec,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentHello {
    pub agent_name: String,
    pub tags: Tags,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentGone {
    pub agent_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateWorkloadState {
    pub workload_states: Vec<WorkloadStateSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ServerHello {
    pub agent_name: Option<String>,
    pub added_workloads: Vec<WorkloadNamed>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UpdateWorkload {
    pub added_workloads: Vec<WorkloadNamed>,
    pub deleted_workloads: Vec<DeletedWorkload>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Goodbye {
    pub connection_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Stop {}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AgentLoadStatus {
    pub agent_name: String,
    pub cpu_usage: CpuUsageSpec,
    pub free_memory: FreeMemorySpec,
}

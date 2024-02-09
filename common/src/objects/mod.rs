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

// [impl->swdd~common-object-representation~1]

// [impl->swdd~common-conversions-between-ankaios-and-proto~1]

mod state;
pub use state::State;

mod workload_state;
pub use workload_state::{ExecutionState, WorkloadState};

mod workload_spec;
pub use workload_spec::{
    get_workloads_per_agent, AddCondition, DeleteCondition, DeletedWorkload,
    DeletedWorkloadCollection, FulfilledBy, UpdateStrategy, WorkloadCollection, WorkloadSpec,
};

mod cronjob;
pub use cronjob::{Cronjob, Interval};

mod tag;
pub use tag::Tag;

mod access_rights;
pub use access_rights::{AccessRights, AccessRightsRule, PatchOperation};

mod workload_execution_instance_name;
pub use workload_execution_instance_name::{
    WorkloadExecutionInstanceName, WorkloadExecutionInstanceNameBuilder, WorkloadInstanceName,
};

mod agent_name;
pub use agent_name::AgentName;

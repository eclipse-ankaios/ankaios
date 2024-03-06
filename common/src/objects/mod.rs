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

mod complete_state;
pub use complete_state::{ApiVersion, CompleteState};

mod stored_workload_spec;
#[cfg(any(feature = "test_utils", test))]
pub use stored_workload_spec::{
    generate_test_stored_workload_spec, generate_test_stored_workload_spec_with_config,
};

pub use stored_workload_spec::StoredWorkloadSpec;

mod workload_state;
#[cfg(any(feature = "test_utils", test))]
pub use workload_state::{
    generate_test_workload_state, generate_test_workload_state_with_agent,
    generate_test_workload_state_with_workload_spec,
};
pub use workload_state::{ExecutionState, ExecutionStateEnum, WorkloadState};

mod workload_spec;
#[cfg(any(feature = "test_utils", test))]
pub use workload_spec::{
    generate_test_workload_spec, generate_test_workload_spec_with_dependencies,
    generate_test_workload_spec_with_param, generate_test_workload_spec_with_runtime_config,
};

pub use workload_spec::{
    get_workloads_per_agent, AddCondition, DeleteCondition, DeletedWorkload,
    DeletedWorkloadCollection, FulfilledBy, WorkloadCollection, WorkloadSpec,
};

mod tag;
pub use tag::Tag;

mod workload_instance_name;
pub use workload_instance_name::{ConfigHash, WorkloadInstanceName, WorkloadInstanceNameBuilder};

mod agent_name;
pub use agent_name::AgentName;

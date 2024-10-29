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

pub mod state;
pub use state::State;

mod complete_state;
pub use complete_state::CompleteState;

mod agent_map;
#[cfg(any(feature = "test_utils", test))]
pub use agent_map::{generate_test_agent_map, generate_test_agent_map_from_specs};
pub use agent_map::{AgentAttributes, AgentMap, CpuUsage, FreeMemory};

mod workload_states_map;
pub use workload_states_map::WorkloadStatesMap;
#[cfg(any(feature = "test_utils", test))]
pub use workload_states_map::{
    generate_test_workload_states_map_from_specs, generate_test_workload_states_map_with_data,
};

mod stored_workload_spec;
#[cfg(any(feature = "test_utils", test))]
pub use stored_workload_spec::{
    generate_test_stored_workload_spec, generate_test_stored_workload_spec_with_config,
};

pub use stored_workload_spec::{StoredWorkloadSpec, STR_RE_CONFIG_REFERENCES};

mod workload_state;
#[cfg(any(feature = "test_utils", test))]
pub use workload_state::{
    generate_test_workload_state, generate_test_workload_state_with_agent,
    generate_test_workload_state_with_workload_spec,
};
pub use workload_state::{
    ExecutionState, ExecutionStateEnum, FailedSubstate, PendingSubstate, RunningSubstate,
    StoppingSubstate, SucceededSubstate, WorkloadState, NO_MORE_RETRIES_MSG,
};

mod workload_spec;
#[cfg(any(feature = "test_utils", test))]
pub use workload_spec::{
    generate_test_runtime_config, generate_test_workload_spec,
    generate_test_workload_spec_with_control_interface_access,
    generate_test_workload_spec_with_dependencies, generate_test_workload_spec_with_param,
    generate_test_workload_spec_with_runtime_config,
};
pub use workload_spec::{STR_RE_AGENT, STR_RE_WORKLOAD};

pub use workload_spec::{
    get_workloads_per_agent, AddCondition, DeleteCondition, DeletedWorkload,
    DeletedWorkloadCollection, FulfilledBy, RestartPolicy, WorkloadCollection, WorkloadSpec,
};

mod tag;
pub use tag::Tag;

mod workload_instance_name;
#[cfg(any(feature = "test_utils", test))]
pub use workload_instance_name::generate_test_workload_instance_name;
pub use workload_instance_name::{ConfigHash, WorkloadInstanceName, WorkloadInstanceNameBuilder};

mod agent_name;
pub use agent_name::AgentName;

mod control_interface_access;
#[cfg(any(feature = "test_utils", test))]
pub use control_interface_access::generate_test_control_interface_access;
pub use control_interface_access::{
    AccessRightsRule, ControlInterfaceAccess, ReadWriteEnum, StateRule,
};

mod config;
#[cfg(any(feature = "test_utils", test))]
pub use config::generate_test_configs;
pub use config::ConfigItem;

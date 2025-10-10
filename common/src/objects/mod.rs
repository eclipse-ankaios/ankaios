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
pub use state::CURRENT_API_VERSION;
pub use state::State;

mod complete_state;
pub use complete_state::CompleteState;

mod agent_map;
pub use agent_map::{AgentAttributes, AgentMap, CpuUsage, FreeMemory};
#[cfg(any(feature = "test_utils", test))]
pub use agent_map::{generate_test_agent_map, generate_test_agent_map_from_specs};

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
    generate_test_stored_workload_spec_with_files,
};

pub use stored_workload_spec::{STR_RE_CONFIG_REFERENCES, StoredWorkloadSpec};

mod workload_state;
pub use workload_state::{
    ExecutionState, ExecutionStateEnum, FailedSubstate, NO_MORE_RETRIES_MSG, PendingSubstate,
    RunningSubstate, StoppingSubstate, SucceededSubstate, WorkloadState,
};
#[cfg(any(feature = "test_utils", test))]
pub use workload_state::{
    generate_test_workload_state, generate_test_workload_state_with_agent,
    generate_test_workload_state_with_workload_spec,
};

mod workload_spec;
pub use workload_spec::{ALLOWED_SYMBOLS, STR_RE_AGENT};
#[cfg(any(feature = "test_utils", test))]
pub use workload_spec::{
    generate_test_runtime_config, generate_test_workload_spec,
    generate_test_workload_spec_with_control_interface_access,
    generate_test_workload_spec_with_dependencies, generate_test_workload_spec_with_param,
    generate_test_workload_spec_with_rendered_files,
    generate_test_workload_spec_with_runtime_config,
};

pub use workload_spec::{
    AddCondition, DeleteCondition, DeletedWorkload, DeletedWorkloadCollection, FulfilledBy,
    RestartPolicy, WorkloadCollection, WorkloadSpec, get_workloads_per_agent,
    verify_workload_name_format,
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
    AccessRightsRule, ControlInterfaceAccess, LogRule, ReadWriteEnum, StateRule, WILDCARD_SYMBOL,
};

mod config;
pub use config::ConfigItem;
#[cfg(any(feature = "test_utils", test))]
pub use config::generate_test_configs;

mod file;
#[cfg(any(feature = "test_utils", test))]
pub use file::generate_test_rendered_workload_files;
pub use file::{File, FileContent};

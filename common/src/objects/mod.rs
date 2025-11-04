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

pub mod agent_load_status;
pub use agent_load_status::AgentLoadStatus;
pub mod state;
pub use state::State;
pub use state::{CURRENT_API_VERSION, PREVIOUS_API_VERSION};

mod complete_state;
pub use complete_state::CompleteState;

mod workload_states_map;
pub use workload_states_map::WorkloadStatesMap;
#[cfg(any(feature = "test_utils", test))]
pub use workload_states_map::{
    generate_test_workload_states_map_from_specs, generate_test_workload_states_map_with_data,
};

mod workload_state;
pub use workload_state::WorkloadState;
#[cfg(any(feature = "test_utils", test))]
pub use workload_state::{
    generate_test_workload_state, generate_test_workload_state_with_agent,
    generate_test_workload_state_with_workload_named,
};

mod agent_name;
pub use agent_name::AgentName;

mod config;
pub use config::ConfigItem;
#[cfg(any(feature = "test_utils", test))]
pub use config::generate_test_configs;

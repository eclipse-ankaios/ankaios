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

use crate::ank_base::{
    CompleteState, DeleteCondition, DeletedWorkload, State, Workload, WorkloadInstanceNameInternal,
    WorkloadMap,
};
pub use crate::ank_base::{
    agent_map::{generate_test_agent_map, generate_test_agent_map_from_workloads},
    control_interface_access::generate_test_control_interface_access,
    file_internal::generate_test_workload_files,
    workload::{
        TestWorkloadFixture, generate_test_runtime_config, generate_test_workload,
        generate_test_workload_with_param, generate_test_workload_with_runtime_config,
    },
    workload_instance_name::generate_test_workload_instance_name,
};
use std::collections::HashMap;

const API_VERSION: &str = "v1";
const WORKLOAD_1_NAME: &str = "workload_name_1";
const WORKLOAD_2_NAME: &str = "workload_name_2";

// pub fn generate_test_state_from_workloads

pub fn generate_test_proto_complete_state(workloads: &[(&str, Workload)]) -> CompleteState {
    CompleteState {
        desired_state: Some(State {
            api_version: API_VERSION.to_string(),
            workloads: Some(WorkloadMap {
                workloads: workloads
                    .iter()
                    .map(|(x, y)| (x.to_string(), y.clone()))
                    .collect(),
            }),
            configs: Some(Default::default()),
        }),
        workload_states: None,
        agents: None,
    }
}

// generate_test_complete_state

// generate_test_complete_state_with_configs

// generate_test_state

pub fn generate_test_proto_state() -> State {
    let workload_name_1 = WORKLOAD_1_NAME.to_string();
    let workload_name_2 = WORKLOAD_2_NAME.to_string();

    let mut workloads = HashMap::new();
    workloads.insert(workload_name_1, generate_test_workload());
    workloads.insert(workload_name_2, generate_test_workload());
    let proto_workloads: Option<WorkloadMap> = Some(WorkloadMap { workloads });

    State {
        api_version: API_VERSION.into(),
        workloads: proto_workloads,
        configs: Some(Default::default()),
    }
}

fn generate_test_delete_dependencies() -> HashMap<String, DeleteCondition> {
    HashMap::from([(
        String::from("workload_A"),
        DeleteCondition::DelCondNotPendingNorRunning,
    )])
}

pub fn generate_test_deleted_workload(agent: String, name: String) -> DeletedWorkload {
    let instance_name = WorkloadInstanceNameInternal::builder()
        .agent_name(agent)
        .workload_name(name)
        .config(&String::from("config"))
        .build();
    DeletedWorkload {
        instance_name,
        dependencies: generate_test_delete_dependencies(),
    }
}

pub fn generate_test_deleted_workload_with_dependencies(
    agent: String,
    name: String,
    dependencies: HashMap<String, DeleteCondition>,
) -> DeletedWorkload {
    let mut deleted_workload = generate_test_deleted_workload(agent, name);
    deleted_workload.dependencies = dependencies;
    deleted_workload
}

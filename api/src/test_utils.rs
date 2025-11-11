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
    CompleteState, CompleteStateInternal, ConfigMap, ConfigMapInternal, ConfigMappingsInternal,
    DeleteCondition, DeletedWorkload, State, StateInternal, Workload, WorkloadInstanceNameInternal,
    WorkloadMap, WorkloadMapInternal, WorkloadNamed,
};
pub use crate::ank_base::{
    agent_map::{generate_test_agent_map, generate_test_agent_map_from_workloads},
    config::generate_test_config_item,
    control_interface_access::generate_test_control_interface_access,
    workload::{
        generate_test_runtime_config, generate_test_workload, generate_test_workload_with_param,
        generate_test_workload_with_runtime_config,
    },
    workload_instance_name::generate_test_workload_instance_name,
    workload_state::{
        generate_test_workload_state, generate_test_workload_state_with_agent,
        generate_test_workload_state_with_workload_named,
    },
    workload_states_map::{
        generate_test_workload_states_map_from_specs,
        generate_test_workload_states_map_from_workload_states,
        generate_test_workload_states_map_with_data,
    },
};
use std::collections::HashMap;

const API_VERSION: &str = "v1";
const WORKLOAD_1_NAME: &str = "workload_name_1";
const WORKLOAD_2_NAME: &str = "workload_name_2";

pub fn generate_test_state_from_workloads(workloads: Vec<WorkloadNamed>) -> StateInternal {
    StateInternal {
        api_version: API_VERSION.into(),
        workloads: WorkloadMapInternal {
            workloads: workloads
                .into_iter()
                .map(|mut w| {
                    let name = w.instance_name.workload_name().to_owned();
                    w.workload.configs = ConfigMappingsInternal {
                        configs: HashMap::from([
                            ("ref1".into(), "config_1".into()),
                            ("ref2".into(), "config_2".into()),
                        ]),
                    };
                    (name, w.workload)
                })
                .collect(),
        },
        configs: ConfigMapInternal {
            configs: HashMap::from([
                (
                    "config_1".to_owned(),
                    generate_test_config_item("value 1".to_owned()),
                ),
                (
                    "config_2".to_owned(),
                    generate_test_config_item("value 2".to_owned()),
                ),
                (
                    "config_3".to_owned(),
                    generate_test_config_item("value 3".to_owned()),
                ),
            ]),
        },
    }
}

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
            configs: Some(ConfigMap {
                configs: HashMap::from([
                    (
                        "config_1".to_string(),
                        generate_test_config_item("value 1".to_string()).into(),
                    ),
                    (
                        "config_2".to_string(),
                        generate_test_config_item("value 2".to_string()).into(),
                    ),
                    (
                        "config_3".to_string(),
                        generate_test_config_item("value 3".to_string()).into(),
                    ),
                ]),
            }),
        }),
        workload_states: None,
        agents: None,
    }
}

pub fn generate_test_complete_state(workloads: Vec<WorkloadNamed>) -> CompleteStateInternal {
    let agents = generate_test_agent_map_from_workloads(
        workloads
            .iter()
            .map(|w| w.workload.clone())
            .collect::<Vec<_>>()
            .as_slice(),
    );
    CompleteStateInternal {
        desired_state: generate_test_state_from_workloads(workloads.clone()),
        workload_states: generate_test_workload_states_map_from_specs(workloads),
        agents,
    }
}

pub fn generate_test_complete_state_with_configs(configs: Vec<String>) -> CompleteStateInternal {
    CompleteStateInternal {
        desired_state: StateInternal {
            api_version: API_VERSION.into(),
            configs: ConfigMapInternal {
                configs: configs
                    .into_iter()
                    .map(|value| (value.clone(), generate_test_config_item(value)))
                    .collect(),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn generate_test_state() -> StateInternal {
    StateInternal {
        api_version: API_VERSION.into(),
        workloads: WorkloadMapInternal {
            workloads: HashMap::from([
                (WORKLOAD_1_NAME.to_owned(), generate_test_workload()),
                (WORKLOAD_2_NAME.to_owned(), generate_test_workload()),
            ]),
        },
        configs: Default::default(),
    }
}

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

pub fn generate_test_configs() -> ConfigMapInternal {
    serde_yaml::from_str(
        "
        config_1:
          values:
            value_1: value123
            value_2:
              - list_value_1
              - list_value_2
          agent_name: agent_A
          config_file: text data
          binary_file: base64_data
        config_2: value_3
        ",
    )
    .unwrap()
}

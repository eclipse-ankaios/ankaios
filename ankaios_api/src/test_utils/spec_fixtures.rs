// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use super::vars;
use crate::ank_base::{
    AccessRightsRuleSpec, AddCondition, AgentAttributesSpec, AgentMapSpec, AgentStatusSpec,
    CompleteStateSpec, ConfigItemEnumSpec, ConfigItemSpec, ConfigMapSpec, ConfigMappingsSpec,
    ControlInterfaceAccessSpec, DependenciesSpec, ExecutionStateSpec,
    FileContentSpec, FileSpec, FilesSpec, ReadWriteEnum, RestartPolicy, StateSpec,
    TagsSpec, WorkloadInstanceNameBuilder, WorkloadInstanceNameSpec, WorkloadMapSpec,
    WorkloadNamed, WorkloadSpec, WorkloadStateSpec, WorkloadStatesMapSpec,
};
use std::collections::HashMap;

// ## CompleteStateSpec fixtures ##

pub fn generate_test_complete_state(workloads: Vec<WorkloadNamed>) -> CompleteStateSpec {
    let agents = generate_test_agent_map_from_workloads(
        workloads
            .iter()
            .map(|w| w.workload.clone())
            .collect::<Vec<_>>()
            .as_slice(),
    );
    CompleteStateSpec {
        desired_state: generate_test_state_from_workloads(workloads.clone()),
        workload_states: generate_test_workload_states_map_from_workloads(workloads),
        agents,
    }
}

pub fn generate_test_complete_state_with_configs(configs: Vec<String>) -> CompleteStateSpec {
    CompleteStateSpec {
        desired_state: StateSpec {
            api_version: vars::API_VERSION.into(),
            configs: ConfigMapSpec {
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

// ## StateSpec fixtures ##

pub fn generate_test_state() -> StateSpec {
    StateSpec {
        api_version: vars::API_VERSION.into(),
        workloads: WorkloadMapSpec {
            workloads: HashMap::from([
                (vars::WORKLOAD_NAMES[0].to_owned(), generate_test_workload()),
                (vars::WORKLOAD_NAMES[1].to_owned(), generate_test_workload()),
            ]),
        },
        configs: Default::default(),
    }
}

pub fn generate_test_state_from_workloads(workloads: Vec<WorkloadNamed>) -> StateSpec {
    StateSpec {
        api_version: vars::API_VERSION.into(),
        workloads: WorkloadMapSpec {
            workloads: workloads
                .into_iter()
                .map(|mut w| {
                    let name = w.instance_name.workload_name().to_owned();
                    w.workload.configs = generate_test_config_mappings();
                    (name, w.workload)
                })
                .collect(),
        },
        configs: ConfigMapSpec {
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

pub fn generate_test_config_item<T>(item: T) -> ConfigItemSpec
where
    T: Into<ConfigItemSpec>,
{
    item.into()
}

impl From<String> for ConfigItemSpec {
    fn from(s: String) -> Self {
        ConfigItemSpec {
            config_item_enum: ConfigItemEnumSpec::String(s),
        }
    }
}

// ## WorkloadStateSpec fixtures ##

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_agent(
    workload_name: &str,
    agent_name: &str,
    execution_state: ExecutionStateSpec,
) -> WorkloadStateSpec {
    WorkloadStateSpec {
        instance_name: WorkloadInstanceNameSpec::builder()
            .workload_name(workload_name)
            .agent_name(agent_name)
            .config(&vars::RUNTIME_CONFIGS[0].to_owned())
            .build(),
        execution_state,
    }
}
#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state_with_workload_named(
    workload_named: &WorkloadNamed,
    execution_state: ExecutionStateSpec,
) -> WorkloadStateSpec {
    WorkloadStateSpec {
        instance_name: workload_named.instance_name.clone(),
        execution_state,
    }
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_state(
    workload_name: &str,
    execution_state: ExecutionStateSpec,
) -> WorkloadStateSpec {
    generate_test_workload_state_with_agent(workload_name, vars::AGENT_NAMES[0], execution_state)
}

// ## AgentMapSpec fixtures ##

pub fn generate_test_agent_map(agent_name: impl Into<String>) -> AgentMapSpec {
    let mut agent_map = AgentMapSpec::default();
    agent_map
        .agents
        .entry(agent_name.into())
        .or_insert(AgentAttributesSpec {
            status: Some(generate_test_agent_status()),
            ..Default::default()
        });
    agent_map
}

pub fn generate_test_agent_map_from_workloads(workloads: &[WorkloadSpec]) -> AgentMapSpec {
    workloads
        .iter()
        .fold(AgentMapSpec::default(), |mut agent_map, wl| {
            let agent_name = &wl.agent;
            agent_map
                .agents
                .entry(agent_name.to_owned())
                .or_insert(AgentAttributesSpec {
                    status: Some(generate_test_agent_status()),
                    ..Default::default()
                });
            agent_map
        })
}

pub fn generate_test_agent_status() -> AgentStatusSpec {
    AgentStatusSpec {
        cpu_usage: Some(vars::CPU_USAGE_SPEC),
        free_memory: Some(vars::FREE_MEMORY_SPEC),
    }
}

// ## ConfigMapSpec fixtures ##

pub fn generate_test_config_map() -> ConfigMapSpec {
    serde_yaml::from_str(
        format!(
            "
        config_1:
          values:
            value_1: value123
            value_2:
              - list_value_1
              - list_value_2
          agent_name: {}
          config_file: {}
          binary_file: {}
        config_2: value_3
        ",
            vars::AGENT_NAMES[0],
            vars::FILE_TEXT_DATA,
            vars::FILE_BINARY_DATA
        )
        .as_str(),
    )
    .unwrap()
}

// ## WorkloadStatesMap fixtures ##

pub fn generate_test_workload_states_map_from_workloads(
    workloads: Vec<WorkloadNamed>,
) -> WorkloadStatesMapSpec {
    let mut wl_states_map = WorkloadStatesMapSpec::new();

    workloads.into_iter().for_each(|workload| {
        wl_states_map
            .entry(workload.instance_name.agent_name().to_owned())
            .or_default()
            .wl_name_state_map
            .entry(workload.instance_name.workload_name().to_owned())
            .or_default()
            .id_state_map
            .insert(
                workload.instance_name.id().to_owned(),
                ExecutionStateSpec::running(),
            );
    });

    wl_states_map
}

pub fn generate_test_workload_states_map_with_data(
    agent_name: impl Into<String>,
    wl_name: impl Into<String>,
    id: impl Into<String>,
    exec_state: ExecutionStateSpec,
) -> WorkloadStatesMapSpec {
    let mut wl_states_map = WorkloadStatesMapSpec::new();

    wl_states_map
        .entry(agent_name.into())
        .or_default()
        .wl_name_state_map
        .entry(wl_name.into())
        .or_default()
        .id_state_map
        .insert(id.into(), exec_state);

    wl_states_map
}

pub fn generate_test_workload_states_map_from_workload_states(
    workload_states: Vec<WorkloadStateSpec>,
) -> WorkloadStatesMapSpec {
    let mut wl_states_map = WorkloadStatesMapSpec::new();

    workload_states.into_iter().for_each(|wl_state| {
        wl_states_map
            .entry(wl_state.instance_name.agent_name().to_owned())
            .or_default()
            .wl_name_state_map
            .entry(wl_state.instance_name.workload_name().to_owned())
            .or_default()
            .id_state_map
            .insert(
                wl_state.instance_name.id().to_owned(),
                wl_state.execution_state,
            );
    });

    wl_states_map
}

// ## WorkloadNamed fixtures ##

pub fn generate_test_workload_named() -> WorkloadNamed {
    generate_test_workload_named_with_params(
        vars::WORKLOAD_NAMES[0],
        vars::AGENT_NAMES[0],
        vars::RUNTIME_NAMES[0],
    )
}

pub fn generate_test_workload_named_with_params(
    workload_name: impl Into<String>,
    agent_name: impl Into<String>,
    runtime_name: impl Into<String>,
) -> WorkloadNamed {
    generate_test_workload_named_with_runtime_config(
        workload_name,
        agent_name,
        runtime_name,
        vars::RUNTIME_CONFIGS[0],
    )
}

pub fn generate_test_workload_named_with_runtime_config(
    workload_name: impl Into<String>,
    agent_name: impl Into<String>,
    runtime_name: impl Into<String>,
    runtime_config: impl Into<String>,
) -> WorkloadNamed {
    let agent_name = agent_name.into();
    let runtime_config = runtime_config.into();
    WorkloadNamed {
        instance_name: generate_test_workload_instance_name_with_runtime_config(
            workload_name,
            agent_name.clone(),
            runtime_config.clone(),
        ),
        workload: generate_test_workload_with_runtime_config(
            agent_name,
            runtime_name,
            runtime_config,
        ),
    }
}

pub fn generate_test_workload_instance_name() -> WorkloadInstanceNameSpec {
    generate_test_workload_instance_name_with_name(vars::WORKLOAD_NAMES[0])
}

pub fn generate_test_workload_instance_name_with_name(
    workload_name: impl Into<String>,
) -> WorkloadInstanceNameSpec {
    generate_test_workload_instance_name_with_params(workload_name, vars::AGENT_NAMES[0])
}

pub fn generate_test_workload_instance_name_with_params(
    workload_name: impl Into<String>,
    agent_name: impl Into<String>,
) -> WorkloadInstanceNameSpec {
    generate_test_workload_instance_name_with_runtime_config(
        workload_name,
        agent_name,
        vars::RUNTIME_CONFIGS[0],
    )
}

pub fn generate_test_workload_instance_name_with_runtime_config(
    workload_name: impl Into<String>,
    agent_name: impl Into<String>,
    runtime_config: impl Into<String>,
) -> WorkloadInstanceNameSpec {
    WorkloadInstanceNameBuilder::default()
        .workload_name(workload_name)
        .agent_name(agent_name)
        .config(&runtime_config.into())
        .build()
}

// ## WorkloadSpec fixtures ##

pub fn generate_test_workload() -> WorkloadSpec {
    generate_test_workload_with_params(vars::AGENT_NAMES[0], vars::RUNTIME_NAMES[0])
}

pub fn generate_test_workload_with_params(
    agent_name: impl Into<String>,
    runtime_name: impl Into<String>,
) -> WorkloadSpec {
    generate_test_workload_with_runtime_config(agent_name, runtime_name, vars::RUNTIME_CONFIGS[0])
}

pub fn generate_test_workload_with_runtime_config(
    agent_name: impl Into<String>,
    runtime_name: impl Into<String>,
    runtime_config: impl Into<String>,
) -> WorkloadSpec {
    WorkloadSpec {
        agent: agent_name.into(),
        dependencies: generate_test_dependencies(),
        restart_policy: RestartPolicy::Always,
        runtime: runtime_name.into(),
        tags: generate_test_tags(),
        runtime_config: runtime_config.into(),
        configs: generate_test_config_mappings(),
        control_interface_access: generate_test_control_interface_access(),
        files: generate_test_files(),
    }
}

pub fn generate_test_dependencies() -> DependenciesSpec {
    DependenciesSpec {
        dependencies: HashMap::from([
            (
                String::from(vars::WORKLOAD_NAMES[1]),
                AddCondition::AddCondRunning,
            ),
            (
                String::from(vars::WORKLOAD_NAMES[2]),
                AddCondition::AddCondSucceeded,
            ),
        ]),
    }
}

pub fn generate_test_tags() -> TagsSpec {
    TagsSpec {
        tags: HashMap::from([
            ("tag1".into(), "val_1".into()),
            ("tag2".into(), "val_2".into()),
        ]),
    }
}

pub fn generate_test_config_mappings() -> ConfigMappingsSpec {
    ConfigMappingsSpec {
        configs: HashMap::from([
            ("ref1".into(), "config_1".into()),
            ("ref2".into(), "config_2".into()),
        ]),
    }
}

pub fn generate_test_control_interface_access() -> ControlInterfaceAccessSpec {
    ControlInterfaceAccessSpec {
        allow_rules: vec![
            AccessRightsRuleSpec::state_rule(
                ReadWriteEnum::RwReadWrite,
                vec!["desiredState".to_string()],
            ),
            AccessRightsRuleSpec::log_rule(
                vars::WORKLOAD_NAMES[0..1]
                    .iter()
                    .map(|&s| s.to_string())
                    .collect(),
            ),
        ],
        deny_rules: vec![AccessRightsRuleSpec::state_rule(
            ReadWriteEnum::RwWrite,
            vec![format!(
                "desiredState.workloads.{}",
                vars::WORKLOAD_NAMES[1]
            )],
        )],
    }
}

pub fn generate_test_files() -> FilesSpec {
    FilesSpec {
        files: vec![
            FileSpec {
                mount_point: vars::FILE_TEXT_PATH.to_string(),
                file_content: FileContentSpec::Data {
                    data: vars::FILE_TEXT_DATA.into(),
                },
            },
            FileSpec {
                mount_point: vars::FILE_BINARY_PATH.to_string(),
                file_content: FileContentSpec::BinaryData {
                    binary_data: vars::FILE_BINARY_DATA.into(),
                },
            },
        ],
    }
}

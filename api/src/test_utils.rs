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
    AddCondition, CompleteState, ConfigMappings, DeleteCondition, DeletedWorkload, Dependencies,
    File, FileContent, Files, RestartPolicy, State, Tag, Tags, Workload,
    WorkloadInstanceNameInternal, WorkloadMap, ControlInterfaceAccess,
};
pub use crate::ank_base::{
    agent_map::{generate_test_agent_map, generate_test_agent_map_from_specs},
    control_interface_access::generate_test_control_interface_access,
    file_internal::generate_test_rendered_workload_files,
    workload::{
        generate_test_runtime_config, generate_test_workload,
        generate_test_workload_with_control_interface_access,
        generate_test_workload_with_dependencies, generate_test_workload_with_param,
        generate_test_workload_with_files, generate_test_workload_with_runtime_config,
    },
    workload_instance_name::generate_test_workload_instance_name,
};
use std::collections::HashMap;

const RUNTIME_NAME: &str = "runtime";
const API_VERSION: &str = "v0.1";
const AGENT_NAME: &str = "agent";
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
    workloads.insert(workload_name_1, generate_test_proto_workload());
    workloads.insert(workload_name_2, generate_test_proto_workload());
    let proto_workloads: Option<WorkloadMap> = Some(WorkloadMap { workloads });

    State {
        api_version: API_VERSION.into(),
        workloads: proto_workloads,
        configs: Some(Default::default()),
    }
}

fn generate_test_proto_dependencies() -> Dependencies {
    Dependencies {
        dependencies: (HashMap::from([
            (
                String::from("workload_A"),
                AddCondition::AddCondRunning.into(),
            ),
            (
                String::from("workload_C"),
                AddCondition::AddCondSucceeded.into(),
            ),
        ])),
    }
}

fn generate_test_delete_dependencies() -> HashMap<String, DeleteCondition> {
    HashMap::from([(
        String::from("workload_A"),
        DeleteCondition::DelCondNotPendingNorRunning,
    )])
}

pub fn generate_test_proto_workload_with_param(
    agent_name: impl Into<String>,
    runtime_name: impl Into<String>,
) -> Workload {
    Workload {
        agent: Some(agent_name.into()),
        dependencies: Some(generate_test_proto_dependencies()),
        restart_policy: Some(RestartPolicy::Always.into()),
        runtime: Some(runtime_name.into()),
        runtime_config: Some("generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
            .to_string()),
        tags: Some(Tags{tags:vec![Tag {
            key: "key".into(),
            value: "value".into(),
        }]}),
        control_interface_access: Default::default(),
        configs: Some(ConfigMappings{configs: [
            ("ref1".into(), "config_1".into()),
            ("ref2".into(), "config_2".into()),
        ].into()}),
        files: Some(generate_test_proto_workload_files()),
    }
}

pub fn generate_test_proto_workload() -> Workload {
    Workload {
        agent: Some(String::from(AGENT_NAME)),
        dependencies: Some(generate_test_proto_dependencies()),
        restart_policy: Some(RestartPolicy::Always.into()),
        runtime: Some(String::from(RUNTIME_NAME)),
        runtime_config: Some("generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
            .to_string()),
        tags: Some(Tags{tags:vec![Tag {
            key: "key".into(),
            value: "value".into(),
        }]}),
        control_interface_access: Some(ControlInterfaceAccess::default()),
        configs: Some(ConfigMappings{configs: [
            ("ref1".into(), "config_1".into()),
            ("ref2".into(), "config_2".into()),
        ].into()}),
        files: Some(generate_test_proto_workload_files()),
    }
}

pub fn generate_test_proto_workload_files() -> Files {
    Files {
        files: vec![
            File {
                mount_point: "/file.json".into(),
                file_content: Some(FileContent::Data("text data".into())),
            },
            File {
                mount_point: "/binary_file".into(),
                file_content: Some(FileContent::BinaryData("base64_data".into())),
            },
        ],
    }
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

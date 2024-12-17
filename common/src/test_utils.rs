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

use std::collections::HashMap;

use api::ank_base::{self, ConfigMappings, Dependencies, Tags, WorkloadMap};
use serde::{Serialize, Serializer};

use crate::objects::{
    generate_test_runtime_config, generate_test_stored_workload_spec_with_config, ConfigItem,
    DeleteCondition, DeletedWorkload, State, StoredWorkloadSpec, WorkloadInstanceName,
    WorkloadSpec,
};

const RUNTIME_NAME: &str = "runtime";
const API_VERSION: &str = "v0.1";
const AGENT_NAME: &str = "agent";
const WORKLOAD_1_NAME: &str = "workload_name_1";
const WORKLOAD_2_NAME: &str = "workload_name_2";

pub fn generate_test_state_from_workloads(workloads: Vec<WorkloadSpec>) -> State {
    State {
        api_version: API_VERSION.into(),
        workloads: workloads
            .into_iter()
            .map(|v| {
                let name = v.instance_name.workload_name().to_owned();
                let mut w = StoredWorkloadSpec::from(v);
                w.configs = [
                    ("ref1".into(), "config_1".into()),
                    ("ref2".into(), "config_2".into()),
                ]
                .into();
                (name, w)
            })
            .collect(),
        configs: [
            ("config_1".into(), ConfigItem::String("value 1".into())),
            ("config_2".into(), ConfigItem::String("value 2".into())),
            ("config_3".into(), ConfigItem::String("value 3".into())),
        ]
        .into(),
    }
}

pub fn generate_test_proto_complete_state(
    workloads: &[(&str, ank_base::Workload)],
) -> ank_base::CompleteState {
    ank_base::CompleteState {
        desired_state: Some(ank_base::State {
            api_version: API_VERSION.to_string(),
            workloads: Some(ank_base::WorkloadMap {
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

pub fn generate_test_complete_state(workloads: Vec<WorkloadSpec>) -> crate::objects::CompleteState {
    use crate::objects::{
        generate_test_agent_map_from_specs, generate_test_workload_states_map_from_specs,
        CompleteState,
    };

    let agents = generate_test_agent_map_from_specs(&workloads);
    CompleteState {
        desired_state: State {
            api_version: API_VERSION.into(),
            workloads: workloads
                .clone()
                .into_iter()
                .map(|v| (v.instance_name.workload_name().to_owned(), v.into()))
                .collect(),
            configs: HashMap::new(),
        },
        workload_states: generate_test_workload_states_map_from_specs(workloads),
        agents,
    }
}

pub fn generate_test_complete_state_with_configs(
    configs: Vec<String>,
) -> crate::objects::CompleteState {
    use crate::objects::CompleteState;
    CompleteState {
        desired_state: State {
            api_version: API_VERSION.into(),
            configs: configs
                .into_iter()
                .map(|value| (value.clone(), ConfigItem::String(String::default())))
                .collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn generate_test_state() -> State {
    let workload_name_1 = WORKLOAD_1_NAME.to_string();
    let workload_name_2 = WORKLOAD_2_NAME.to_string();

    let mut ankaios_workloads = HashMap::new();

    let workload_1 = generate_test_stored_workload_spec_with_config(
        AGENT_NAME.to_owned(),
        RUNTIME_NAME.to_owned(),
        generate_test_runtime_config(),
    );

    let workload_2 = generate_test_stored_workload_spec_with_config(
        AGENT_NAME.to_owned(),
        RUNTIME_NAME.to_owned(),
        generate_test_runtime_config(),
    );

    ankaios_workloads.insert(workload_name_1, workload_1);
    ankaios_workloads.insert(workload_name_2, workload_2);

    State {
        api_version: API_VERSION.into(),
        workloads: ankaios_workloads,
        configs: HashMap::new(),
    }
}

pub fn generate_test_proto_state() -> ank_base::State {
    let workload_name_1 = WORKLOAD_1_NAME.to_string();
    let workload_name_2 = WORKLOAD_2_NAME.to_string();

    let mut workloads = HashMap::new();
    workloads.insert(workload_name_1, generate_test_proto_workload());
    workloads.insert(workload_name_2, generate_test_proto_workload());
    let proto_workloads: Option<WorkloadMap> = Some(WorkloadMap { workloads });

    ank_base::State {
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
                ank_base::AddCondition::AddCondRunning.into(),
            ),
            (
                String::from("workload_C"),
                ank_base::AddCondition::AddCondSucceeded.into(),
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
) -> ank_base::Workload {
    ank_base::Workload {
        agent: Some(agent_name.into()),
        dependencies: Some(generate_test_proto_dependencies()),
        restart_policy: Some(ank_base::RestartPolicy::Always.into()),
        runtime: Some(runtime_name.into()),
        runtime_config: Some("generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
            .to_string()),
        tags: Some(Tags{tags:vec![ank_base::Tag {
            key: "key".into(),
            value: "value".into(),
        }]}),
        control_interface_access: Default::default(),
        configs: Some(ConfigMappings{configs: [
            ("ref1".into(), "config_1".into()),
            ("ref2".into(), "config_2".into()),
        ].into()})
    }
}

pub fn generate_test_proto_workload() -> ank_base::Workload {
    ank_base::Workload {
        agent: Some(String::from(AGENT_NAME)),
        dependencies: Some(generate_test_proto_dependencies()),
        restart_policy: Some(ank_base::RestartPolicy::Always.into()),
        runtime: Some(String::from(RUNTIME_NAME)),
        runtime_config: Some("generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
            .to_string()),
        tags: Some(Tags{tags:vec![ank_base::Tag {
            key: "key".into(),
            value: "value".into(),
        }]}),
        control_interface_access: Default::default(),
        configs: Some(ConfigMappings{configs: [
            ("ref1".into(), "config_1".into()),
            ("ref2".into(), "config_2".into()),
        ].into()})
    }
}

pub fn generate_test_deleted_workload(
    agent: String,
    name: String,
) -> crate::objects::DeletedWorkload {
    let instance_name = WorkloadInstanceName::builder()
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
) -> crate::objects::DeletedWorkload {
    let mut deleted_workload = generate_test_deleted_workload(agent, name);
    deleted_workload.dependencies = dependencies;
    deleted_workload
}

pub struct MockAllContextSync {
    mutex_tokio: tokio::sync::Mutex<()>,
    mutex_std: std::sync::Mutex<()>,
}
impl MockAllContextSync {
    pub fn new() -> Self {
        Self {
            mutex_tokio: tokio::sync::Mutex::new(()),
            mutex_std: std::sync::Mutex::new(()),
        }
    }
    pub async fn get_lock_async(&self) -> tokio::sync::MutexGuard<()> {
        self.mutex_tokio.lock().await
    }

    pub fn get_lock(&self) -> std::sync::MutexGuard<()> {
        match self.mutex_std.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl Default for MockAllContextSync {
    fn default() -> Self {
        Self::new()
    }
}

pub fn serialize_as_map<A, B, S>(x: &[(A, B)], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    A: Clone + Serialize + Eq + std::hash::Hash,
    B: Clone + Serialize,
{
    let x: HashMap<A, B> = x.iter().cloned().collect();
    x.serialize(s)
}

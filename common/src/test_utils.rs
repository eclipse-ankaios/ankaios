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

use api::proto;
use serde::{Serialize, Serializer};

use crate::objects::{
    AccessRights, AddCondition, Cronjob, DeleteCondition, DeletedWorkload, Interval, State, Tag,
    UpdateStrategy, WorkloadSpec,
};

#[cfg(feature = "test_utils")]
pub fn generate_test_state_from_workloads(workloads: Vec<WorkloadSpec>) -> State {
    State {
        workloads: workloads.into_iter().map(|v| (v.name.clone(), v)).collect(),
        configs: HashMap::new(),
        cron_jobs: HashMap::new(),
    }
}

#[cfg(feature = "test_utils")]
pub fn generate_test_complete_state(
    workloads: Vec<WorkloadSpec>,
) -> crate::commands::CompleteState {
    use crate::{
        commands::CompleteState,
        objects::{ExecutionState, WorkloadExecutionInstanceName, WorkloadState},
    };

    CompleteState {
        desired_state: State {
            workloads: workloads
                .clone()
                .into_iter()
                .map(|v| (v.name.clone(), v))
                .collect(),
            configs: HashMap::new(),
            cron_jobs: HashMap::new(),
        },
        workload_states: workloads
            .into_iter()
            .map(|v| WorkloadState {
                instance_name: WorkloadExecutionInstanceName::builder()
                    .workload_name(&v.name)
                    .agent_name(&v.agent)
                    .config(&v.runtime_config)
                    .build(),
                workload_id: "some strange Id".to_string(),
                execution_state: ExecutionState::running(),
            })
            .collect(),
        ..Default::default()
    }
}

pub fn generate_test_state() -> State {
    let workload_name_1 = "workload_name_1".to_string();
    let workload_name_2 = "workload_name_2".to_string();
    let config_key_1 = "key1".to_string();
    let config_value_1 = "value1".to_string();
    let config_key_2 = "key2".to_string();
    let config_value_2 = "value2".to_string();
    let cronjob_name_1 = "cronjob1".to_string();
    let cronjob_name_2 = "cronjob2".to_string();

    let mut ankaios_workloads = HashMap::new();

    let mut workload_1 = generate_test_workload_spec();
    let mut workload_2 = generate_test_workload_spec();

    workload_1.name = "workload_name_1".to_string();
    workload_2.name = "workload_name_2".to_string();

    ankaios_workloads.insert(workload_name_1, workload_1);
    ankaios_workloads.insert(workload_name_2, workload_2);
    let mut ankaios_configs = HashMap::new();
    ankaios_configs.insert(config_key_1, config_value_1);
    ankaios_configs.insert(config_key_2, config_value_2);
    let mut ankaios_cronjobs = HashMap::new();
    ankaios_cronjobs.insert(cronjob_name_1, generate_test_cronjob());
    ankaios_cronjobs.insert(cronjob_name_2, generate_test_cronjob());

    State {
        workloads: ankaios_workloads,
        configs: ankaios_configs,
        cron_jobs: ankaios_cronjobs,
    }
}

pub fn generate_test_proto_state() -> proto::State {
    let workload_name_1 = "workload_name_1".to_string();
    let workload_name_2 = "workload_name_2".to_string();
    let config_key_1 = "key1".to_string();
    let config_value_1 = "value1".to_string();
    let config_key_2 = "key2".to_string();
    let config_value_2 = "value2".to_string();
    let cronjob_name_1 = "cronjob1".to_string();
    let cronjob_name_2 = "cronjob2".to_string();

    let mut proto_workloads = HashMap::new();
    proto_workloads.insert(workload_name_1, generate_test_proto_workload());
    proto_workloads.insert(workload_name_2, generate_test_proto_workload());
    let mut proto_configs = HashMap::new();
    proto_configs.insert(config_key_1, config_value_1);
    proto_configs.insert(config_key_2, config_value_2);
    let mut proto_cronjobs = HashMap::new();
    proto_cronjobs.insert(cronjob_name_1, generate_test_proto_cronjob());
    proto_cronjobs.insert(cronjob_name_2, generate_test_proto_cronjob());

    proto::State {
        workloads: proto_workloads,
        configs: proto_configs,
        cronjobs: proto_cronjobs,
    }
}

fn generate_test_dependencies() -> HashMap<String, AddCondition> {
    HashMap::from([
        (String::from("workload A"), AddCondition::AddCondRunning),
        (String::from("workload C"), AddCondition::AddCondSucceeded),
    ])
}

fn generate_test_proto_dependencies() -> HashMap<String, i32> {
    HashMap::from([
        (
            String::from("workload A"),
            proto::AddCondition::AddCondRunning.into(),
        ),
        (
            String::from("workload C"),
            proto::AddCondition::AddCondSucceeded.into(),
        ),
    ])
}

fn generate_test_delete_dependencies() -> HashMap<String, DeleteCondition> {
    HashMap::from([(
        String::from("workload A"),
        DeleteCondition::DelCondNotPendingNorRunning,
    )])
}

fn generate_test_proto_delete_dependencies() -> HashMap<String, i32> {
    HashMap::from([(
        String::from("workload A"),
        proto::DeleteCondition::DelCondNotPendingNorRunning.into(),
    )])
}

pub fn generate_test_workload_spec_with_param(
    agent_name: String,
    workload_name: String,
    runtime_name: String,
) -> crate::objects::WorkloadSpec {
    WorkloadSpec {
        dependencies: generate_test_dependencies(),
        update_strategy: UpdateStrategy::Unspecified,
        restart: true,
        access_rights: AccessRights::default(),
        runtime: runtime_name,
        name: workload_name,
        agent: agent_name,
        tags: vec![Tag {
            key: "key".into(),
            value: "value".into(),
        }],
        runtime_config: "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
            .to_string(),
    }
}

pub fn generate_test_workload_spec() -> WorkloadSpec {
    generate_test_workload_spec_with_param(
        "agent".to_string(),
        "name".to_string(),
        "runtime".to_string(),
    )
}

pub fn generate_test_proto_workload() -> proto::Workload {
    proto::Workload {
        agent: String::from("agent"),
        dependencies: generate_test_proto_dependencies(),
        restart: true,
        update_strategy: proto::UpdateStrategy::Unspecified.into(),
        access_rights: None,
        runtime: String::from("runtime"),
        runtime_config: "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
            .to_string(),
        tags: vec![proto::Tag {
            key: "key".into(),
            value: "value".into(),
        }],
    }
}

pub fn generate_test_deleted_workload(
    agent: String,
    name: String,
) -> crate::objects::DeletedWorkload {
    DeletedWorkload {
        agent,
        name,
        dependencies: generate_test_delete_dependencies(),
    }
}

pub fn generate_test_proto_deleted_workload() -> proto::DeletedWorkload {
    proto::DeletedWorkload {
        name: "workload X".to_string(),
        dependencies: generate_test_proto_delete_dependencies(),
    }
}

pub fn generate_test_cronjob() -> Cronjob {
    Cronjob {
        workload: String::from("some job"),
        interval: Interval {
            hours: 4,
            minutes: 3,
            seconds: 42,
        },
    }
}

pub fn generate_test_proto_cronjob() -> proto::Cronjob {
    proto::Cronjob {
        workload: String::from("some job"),
        interval: Some(proto::Interval {
            hours: 4,
            minutes: 3,
            seconds: 42,
        }),
    }
}

pub fn generate_test_cronjob_empty_interval() -> Cronjob {
    Cronjob {
        workload: String::from("some job"),
        interval: Interval {
            hours: 0,
            minutes: 0,
            seconds: 0,
        },
    }
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

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

use api::ank_base;
use serde::{Serialize, Serializer};

use crate::objects::{
    generate_test_workload_spec_with_param, DeleteCondition, DeletedWorkload, State,
    WorkloadInstanceName, WorkloadSpec,
};

#[cfg(feature = "test_utils")]
pub fn generate_test_state_from_workloads(workloads: Vec<WorkloadSpec>) -> State {
    State {
        api_version: "v0.1".into(),
        workloads: workloads
            .into_iter()
            .map(|v| (v.instance_name.workload_name().to_owned(), v.into()))
            .collect(),
    }
}

#[cfg(feature = "test_utils")]
pub fn generate_test_complete_state(workloads: Vec<WorkloadSpec>) -> crate::objects::CompleteState {
    use crate::{
        objects::CompleteState,
        objects::{ExecutionState, WorkloadState},
    };

    CompleteState {
        desired_state: State {
            api_version: "v0.1".into(),
            workloads: workloads
                .clone()
                .into_iter()
                .map(|v| (v.instance_name.workload_name().to_owned(), v.into()))
                .collect(),
        },
        workload_states: workloads
            .into_iter()
            .map(|v| WorkloadState {
                instance_name: v.instance_name,
                execution_state: ExecutionState::running(),
            })
            .collect(),
        ..Default::default()
    }
}

pub fn generate_test_state() -> State {
    let workload_name_1 = "workload_name_1".to_string();
    let workload_name_2 = "workload_name_2".to_string();

    let mut ankaios_workloads = HashMap::new();

    let workload_1 = generate_test_workload_spec_with_param(
        "agent".to_owned(),
        "workload_name_1".to_owned(),
        "runtime".to_owned(),
    );

    let workload_2 = generate_test_workload_spec_with_param(
        "agent".to_owned(),
        "workload_name_2".to_owned(),
        "runtime".to_owned(),
    );

    ankaios_workloads.insert(workload_name_1, workload_1.into());
    ankaios_workloads.insert(workload_name_2, workload_2.into());

    State {
        api_version: "v0.1".into(),
        workloads: ankaios_workloads,
    }
}

pub fn generate_test_proto_state() -> ank_base::State {
    let workload_name_1 = "workload_name_1".to_string();
    let workload_name_2 = "workload_name_2".to_string();

    let mut proto_workloads = HashMap::new();
    proto_workloads.insert(workload_name_1, generate_test_proto_workload());
    proto_workloads.insert(workload_name_2, generate_test_proto_workload());

    ank_base::State {
        api_version: "v0.1".into(),
        workloads: proto_workloads,
    }
}

fn generate_test_proto_dependencies() -> HashMap<String, i32> {
    HashMap::from([
        (
            String::from("workload A"),
            ank_base::AddCondition::AddCondRunning.into(),
        ),
        (
            String::from("workload C"),
            ank_base::AddCondition::AddCondSucceeded.into(),
        ),
    ])
}

fn generate_test_delete_dependencies() -> HashMap<String, DeleteCondition> {
    HashMap::from([(
        String::from("workload A"),
        DeleteCondition::DelCondNotPendingNorRunning,
    )])
}



pub fn generate_test_proto_workload() -> ank_base::Workload {
    ank_base::Workload {
        agent: String::from("agent"),
        dependencies: generate_test_proto_dependencies(),
        restart_policy: ank_base::RestartPolicy::Always.into(),
        runtime: String::from("runtime"),
        runtime_config: "generalOptions: [\"--version\"]\ncommandOptions: [\"--network=host\"]\nimage: alpine:latest\ncommandArgs: [\"bash\"]\n"
            .to_string(),
        tags: vec![ank_base::Tag {
            key: "key".into(),
            value: "value".into(),
        }],
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

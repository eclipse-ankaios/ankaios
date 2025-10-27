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

use crate::objects::{ConfigItem, State};
use api::ank_base::{ConfigMappingsInternal, WorkloadInternal};
#[cfg(any(feature = "test_utils", test))]
use api::test_utils::{
    generate_test_rendered_workload_files, generate_test_runtime_config,
    generate_test_workload_with_runtime_config,
};
use serde::{Serialize, Serializer};

const RUNTIME_NAME: &str = "runtime";
const API_VERSION: &str = "v0.1";
const AGENT_NAME: &str = "agent";
const WORKLOAD_1_NAME: &str = "workload_name_1";
const WORKLOAD_2_NAME: &str = "workload_name_2";

pub fn generate_test_state_from_workloads(workloads: Vec<WorkloadInternal>) -> State {
    State {
        api_version: API_VERSION.into(),
        workloads: workloads
            .into_iter()
            .map(|mut w| {
                let name = w.instance_name.workload_name().to_owned();
                w.configs = ConfigMappingsInternal {
                    configs: HashMap::from([
                        ("ref1".into(), "config_1".into()),
                        ("ref2".into(), "config_2".into()),
                    ]),
                };
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

pub fn generate_test_complete_state(
    workloads: Vec<WorkloadInternal>,
) -> crate::objects::CompleteState {
    use crate::objects::{CompleteState, generate_test_workload_states_map_from_specs};
    use api::test_utils::generate_test_agent_map_from_specs;

    let agents = generate_test_agent_map_from_specs(&workloads);
    CompleteState {
        desired_state: State {
            api_version: API_VERSION.into(),
            workloads: workloads
                .clone()
                .into_iter()
                .map(|w| (w.instance_name.workload_name().to_owned(), w))
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

    let mut workload_1 = generate_test_workload_with_runtime_config(
        AGENT_NAME,
        WORKLOAD_1_NAME,
        RUNTIME_NAME,
        generate_test_runtime_config(),
    );
    workload_1.files = generate_test_rendered_workload_files();

    let mut workload_2 = generate_test_workload_with_runtime_config(
        AGENT_NAME,
        WORKLOAD_2_NAME,
        RUNTIME_NAME,
        generate_test_runtime_config(),
    );
    workload_2.files = generate_test_rendered_workload_files();

    ankaios_workloads.insert(workload_name_1, workload_1);
    ankaios_workloads.insert(workload_name_2, workload_2);

    State {
        api_version: API_VERSION.into(),
        workloads: ankaios_workloads,
        configs: HashMap::new(),
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

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

use serde::{Serialize, Serializer};
use std::collections::HashMap;

use crate::objects::CompleteState;

use api::ank_base::{
    ConfigItemEnumInternal, ConfigItemInternal, ConfigMapInternal, StateInternal, WorkloadNamed,
};
use api::test_utils::{
    generate_test_agent_map_from_workloads, generate_test_state_from_workloads,
    generate_test_workload_states_map_from_specs,
};

const API_VERSION: &str = "v1";

pub fn generate_test_complete_state(
    workloads: Vec<WorkloadNamed>,
) -> crate::objects::CompleteState {
    let agents = generate_test_agent_map_from_workloads(
        workloads
            .iter()
            .map(|w| w.workload.clone())
            .collect::<Vec<_>>()
            .as_slice(),
    );
    CompleteState {
        desired_state: generate_test_state_from_workloads(workloads.clone()),
        workload_states: generate_test_workload_states_map_from_specs(workloads),
        agents,
    }
}

pub fn generate_test_complete_state_with_configs(
    configs: Vec<String>,
) -> crate::objects::CompleteState {
    use crate::objects::CompleteState;
    CompleteState {
        desired_state: StateInternal {
            api_version: API_VERSION.into(),
            configs: ConfigMapInternal {
                configs: configs
                    .into_iter()
                    .map(|value| {
                        (
                            value.clone(),
                            ConfigItemInternal {
                                config_item_enum: ConfigItemEnumInternal::String(value),
                            },
                        )
                    })
                    .collect(),
            },
            ..Default::default()
        },
        ..Default::default()
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

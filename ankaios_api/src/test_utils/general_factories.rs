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

use super::fixtures;
use crate::{
    ank_base::{
        CompleteState, ConfigMap, DeleteCondition, DeletedWorkload, State, Workload, WorkloadMap,
    },
    test_utils::{generate_test_config_item, generate_test_workload_instance_name_with_params},
};
use std::collections::HashMap;

// ## CompleteState fixtures ##

pub fn generate_test_proto_complete_state(workloads: &[(&str, Workload)]) -> CompleteState {
    CompleteState {
        desired_state: Some(State {
            api_version: fixtures::API_VERSION.to_string(),
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

// ## DeletedWorkload fixtures ##

pub fn generate_test_deleted_workload() -> DeletedWorkload {
    generate_test_deleted_workload_with_params(fixtures::AGENT_NAMES[0], fixtures::WORKLOAD_NAMES[0])
}

pub fn generate_test_deleted_workload_with_params(
    agent: impl Into<String>,
    workload_name: impl Into<String>,
) -> DeletedWorkload {
    generate_test_deleted_workload_with_dependencies(
        agent,
        workload_name,
        generate_test_delete_dependencies(),
    )
}

pub fn generate_test_deleted_workload_with_dependencies(
    agent: impl Into<String>,
    workload_name: impl Into<String>,
    dependencies: HashMap<String, DeleteCondition>,
) -> DeletedWorkload {
    DeletedWorkload {
        instance_name: generate_test_workload_instance_name_with_params(workload_name, agent),
        dependencies,
    }
}

fn generate_test_delete_dependencies() -> HashMap<String, DeleteCondition> {
    HashMap::from([(
        String::from(fixtures::WORKLOAD_NAMES[0]),
        DeleteCondition::DelCondNotPendingNorRunning,
    )])
}

// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use std::collections::{hash_map::Entry, HashMap};

use api::ank_base;
use serde::{Deserialize, Serialize};

use super::{ExecutionState, WorkloadInstanceName, WorkloadSpec, WorkloadState};

type AgentName = String;
type WorkloadName = String;
type WorkloadId = String;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct WorkloadStatesMap(
    HashMap<AgentName, HashMap<WorkloadName, HashMap<WorkloadId, ExecutionState>>>,
);

// [impl->swdd~state-map-for-workload-execution-states~1]
impl WorkloadStatesMap {
    pub fn new() -> WorkloadStatesMap {
        WorkloadStatesMap(HashMap::new())
    }

    pub fn entry(
        &mut self,
        key: String,
    ) -> Entry<'_, String, HashMap<String, HashMap<String, ExecutionState>>> {
        self.0.entry(key)
    }

    pub fn get_workload_state_for_agent(&self, agent_name: &str) -> Vec<WorkloadState> {
        self.0
            .get(agent_name)
            .map(|name_map| {
                name_map
                    .iter()
                    .flat_map(|(wl_name, id_map)| {
                        id_map.iter().map(move |(wl_id, exec_state)| WorkloadState {
                            instance_name: WorkloadInstanceName::new(agent_name, wl_name, wl_id),
                            execution_state: exec_state.to_owned(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_workload_state_excluding_agent(
        &self,
        excluding_agent_name: &str,
    ) -> Vec<WorkloadState> {
        self.0
            .iter()
            .filter(|(agent_name, _)| *agent_name != excluding_agent_name)
            .flat_map(|(agent_name, name_state_map)| {
                name_state_map
                    .iter()
                    .flat_map(move |(wl_name, id_state_map)| {
                        id_state_map
                            .iter()
                            .map(move |(wl_id, exec_state)| WorkloadState {
                                instance_name: WorkloadInstanceName::new(
                                    agent_name, wl_name, wl_id,
                                ),
                                execution_state: exec_state.to_owned(),
                            })
                    })
            })
            .collect()
    }

    pub fn agent_disconnected(&mut self, agent_name: &str) {
        if let Some(agent_states) = self.0.get_mut(agent_name) {
            agent_states.iter_mut().for_each(|(_, name_map)| {
                name_map
                    .iter_mut()
                    .for_each(|(_, exec_state)| *exec_state = ExecutionState::agent_disconnected())
            })
        }
    }

    pub fn initial_state(&mut self, workload_specs: &Vec<WorkloadSpec>) {
        for spec in workload_specs {
            self.entry(spec.instance_name.agent_name().to_owned())
                .or_default()
                .entry(spec.instance_name.workload_name().to_owned())
                .or_default()
                .entry(spec.instance_name.id().to_owned())
                .or_insert(if spec.instance_name.agent_name().is_empty() {
                    ExecutionState::not_scheduled()
                } else {
                    ExecutionState::initial()
                });
        }
    }

    pub fn remove(&mut self, instance_name: &WorkloadInstanceName) {
        if let Some(agent_states) = self.0.get_mut(instance_name.agent_name()) {
            if let Some(workload_states) = agent_states.get_mut(instance_name.workload_name()) {
                workload_states.remove(instance_name.id());
                // the following part is needed to cleanup empty paths in the state map
                if workload_states.is_empty() {
                    agent_states.remove(instance_name.workload_name());
                    if agent_states.is_empty() {
                        self.0.remove(instance_name.agent_name());
                    }
                }
            }
        }
    }

    pub fn process_new_states(&mut self, workload_states: Vec<WorkloadState>) {
        workload_states.into_iter().for_each(|workload_state| {
            if workload_state.execution_state.is_removed() {
                self.remove(&workload_state.instance_name);
            } else {
                self.entry(workload_state.instance_name.agent_name().to_owned())
                    .or_default()
                    .entry(workload_state.instance_name.workload_name().to_owned())
                    .or_default()
                    .insert(
                        workload_state.instance_name.id().to_owned(),
                        workload_state.execution_state,
                    );
            }
        });
    }
}

impl From<WorkloadStatesMap> for Vec<WorkloadState> {
    fn from(value: WorkloadStatesMap) -> Self {
        value
            .into_iter()
            .flat_map(|(agent_name, name_state_map)| {
                name_state_map
                    .into_iter()
                    .flat_map(move |(wl_name, id_state_map)| {
                        let agent_name = agent_name.clone();
                        id_state_map
                            .into_iter()
                            .map(move |(wl_id, exec_state)| WorkloadState {
                                instance_name: WorkloadInstanceName::new(
                                    agent_name.clone(),
                                    wl_name.clone(),
                                    wl_id,
                                ),
                                execution_state: exec_state,
                            })
                    })
            })
            .collect()
    }
}

impl From<HashMap<String, HashMap<String, HashMap<String, ExecutionState>>>> for WorkloadStatesMap {
    fn from(value: HashMap<String, HashMap<String, HashMap<String, ExecutionState>>>) -> Self {
        WorkloadStatesMap(value)
    }
}

impl IntoIterator for WorkloadStatesMap {
    type Item =
        <HashMap<String, HashMap<String, HashMap<String, ExecutionState>>> as IntoIterator>::Item;

    type IntoIter = <HashMap<String, HashMap<String, HashMap<String, ExecutionState>>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl From<WorkloadStatesMap> for Option<ank_base::WorkloadStatesMap> {
    fn from(item: WorkloadStatesMap) -> Option<ank_base::WorkloadStatesMap> {
        if item.0.is_empty() {
            return None;
        }
        Some(ank_base::WorkloadStatesMap {
            agent_state_map: item
                .into_iter()
                .map(|(agent_name, wl_map)| {
                    (
                        agent_name,
                        ank_base::ExecutionsStatesOfWorkload {
                            wl_name_state_map: wl_map
                                .into_iter()
                                .map(|(wl_name, id_map)| {
                                    (
                                        wl_name,
                                        ank_base::ExecutionsStatesForId {
                                            id_state_map: id_map
                                                .into_iter()
                                                .map(|(id, exec_state)| (id, exec_state.into()))
                                                .collect(),
                                        },
                                    )
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
        })
    }
}

impl From<ank_base::WorkloadStatesMap> for WorkloadStatesMap {
    fn from(item: ank_base::WorkloadStatesMap) -> WorkloadStatesMap {
        WorkloadStatesMap(
            item.agent_state_map
                .into_iter()
                .map(|(agent_name, wl_map)| {
                    (
                        agent_name,
                        wl_map
                            .wl_name_state_map
                            .into_iter()
                            .map(|(workload_name, id_map)| {
                                (
                                    workload_name,
                                    id_map
                                        .id_state_map
                                        .into_iter()
                                        .map(|(id, exec_state)| (id, exec_state.into()))
                                        .collect(),
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        )
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_states_map_from_specs(
    workloads: Vec<WorkloadSpec>,
) -> WorkloadStatesMap {
    let mut wl_states_map = WorkloadStatesMap::new();

    workloads.into_iter().for_each(|workload| {
        wl_states_map
            .entry(workload.instance_name.agent_name().to_owned())
            .or_default()
            .entry(workload.instance_name.workload_name().to_owned())
            .or_default()
            .insert(
                workload.instance_name.id().to_owned(),
                ExecutionState::running(),
            );
    });

    wl_states_map
}

#[cfg(any(feature = "test_utils", test))]
pub fn generate_test_workload_states_map_with_data(
    agent_name: impl Into<String>,
    wl_name: impl Into<String>,
    id: impl Into<String>,
    exec_state: ExecutionState,
) -> WorkloadStatesMap {
    let mut wl_states_map = WorkloadStatesMap::new();

    wl_states_map
        .entry(agent_name.into())
        .or_default()
        .entry(wl_name.into())
        .or_default()
        .insert(id.into(), exec_state);

    wl_states_map
}

#[cfg(test)]
pub fn generate_test_workload_states_map_from_workload_states(
    workload_states: Vec<WorkloadState>,
) -> WorkloadStatesMap {
    let mut wl_states_map = WorkloadStatesMap::new();

    workload_states.into_iter().for_each(|wl_state| {
        wl_states_map
            .entry(wl_state.instance_name.agent_name().to_owned())
            .or_default()
            .entry(wl_state.instance_name.workload_name().to_owned())
            .or_default()
            .insert(
                wl_state.instance_name.id().to_owned(),
                wl_state.execution_state,
            );
    });

    wl_states_map
}

// [utest->swdd~state-map-for-workload-execution-states~1]
#[cfg(test)]
mod tests {
    use std::vec;

    use crate::objects::{
        generate_test_workload_spec_with_runtime_config, generate_test_workload_state_with_agent,
        WorkloadState,
    };

    use crate::objects::ExecutionState;

    use super::{generate_test_workload_states_map_from_workload_states, WorkloadStatesMap};

    const AGENT_A: &str = "agent_A";
    const AGENT_B: &str = "agent_B";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const WORKLOAD_NAME_4: &str = "workload_4";

    fn create_test_setup() -> WorkloadStatesMap {
        generate_test_workload_states_map_from_workload_states(vec![
            generate_test_workload_state_with_agent(
                WORKLOAD_NAME_1,
                AGENT_A,
                ExecutionState::succeeded(),
            ),
            generate_test_workload_state_with_agent(
                WORKLOAD_NAME_2,
                AGENT_A,
                ExecutionState::starting("additional_info"),
            ),
            generate_test_workload_state_with_agent(
                WORKLOAD_NAME_3,
                AGENT_B,
                ExecutionState::running(),
            ),
        ])
    }

    #[test]
    fn utest_workload_states_map_into_vec_of_workload_states() {
        let wls_db = create_test_setup();

        let mut wls_res: Vec<WorkloadState> = wls_db.into();
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res,
            vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_1,
                    AGENT_A,
                    ExecutionState::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_2,
                    AGENT_A,
                    ExecutionState::starting("additional_info"),
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_3,
                    AGENT_B,
                    ExecutionState::running()
                ),
            ]
        )
    }

    #[test]
    fn utest_workload_states_store_new() {
        let mut wls_db = create_test_setup();

        let wl_state_4 = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_4,
            AGENT_A,
            ExecutionState::starting("test info"),
        );

        wls_db.process_new_states(vec![wl_state_4.clone()]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_1,
                    AGENT_A,
                    ExecutionState::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_2,
                    AGENT_A,
                    ExecutionState::starting("additional_info"),
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_3,
                    AGENT_B,
                    ExecutionState::running()
                ),
                wl_state_4
            ])
        )
    }

    #[test]
    fn utest_workload_states_store_update() {
        let mut wls_db = create_test_setup();

        let wl_state_2_update = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_2,
            AGENT_A,
            ExecutionState::running(),
        );

        wls_db.process_new_states(vec![wl_state_2_update.clone()]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_1,
                    AGENT_A,
                    ExecutionState::succeeded()
                ),
                wl_state_2_update,
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_3,
                    AGENT_B,
                    ExecutionState::running()
                )
            ])
        )
    }

    #[test]
    fn utest_get_workload_states_excluding_agent_returns_correct() {
        let wls_db = create_test_setup();

        let mut wls_res = wls_db.get_workload_state_excluding_agent(AGENT_B);
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res,
            vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_1,
                    AGENT_A,
                    ExecutionState::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_2,
                    AGENT_A,
                    ExecutionState::starting("additional_info"),
                ),
            ]
        )
    }

    #[test]
    fn utest_mark_all_workload_state_for_agent_disconnected() {
        let mut wls_db = create_test_setup();

        wls_db.agent_disconnected(AGENT_A);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_1,
                    AGENT_A,
                    ExecutionState::agent_disconnected()
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_2,
                    AGENT_A,
                    ExecutionState::agent_disconnected()
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_3,
                    AGENT_B,
                    ExecutionState::running()
                ),
            ])
        )
    }

    #[test]
    fn utest_get_workload_state_for_agent_returns_workload_state_of_existing_agent_name() {
        let wls_db = create_test_setup();

        let mut wls_res = wls_db.get_workload_state_for_agent(AGENT_A);
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res,
            vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_1,
                    AGENT_A,
                    ExecutionState::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_2,
                    AGENT_A,
                    ExecutionState::starting("additional_info"),
                ),
            ]
        )
    }

    #[test]
    fn utest_get_workload_state_for_agent_returns_empty_list_of_non_existing_agent_name() {
        let wls_db = create_test_setup();
        assert_eq!(
            wls_db.get_workload_state_for_agent("non_existing_agent"),
            vec![]
        );
    }

    #[test]
    fn utest_workload_states_deletes_removed() {
        let mut wls_db = create_test_setup();

        let wl_state_1 = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_1,
            AGENT_A,
            ExecutionState::removed(),
        );

        let wl_state_3 = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_3,
            AGENT_B,
            ExecutionState::removed(),
        );

        wls_db.process_new_states(vec![wl_state_1, wl_state_3]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_2,
                    AGENT_A,
                    ExecutionState::starting("additional_info"),
                )
            ])
        )
    }

    #[test]
    fn utest_workload_states_initial_state() {
        let mut wls_db = WorkloadStatesMap::new();

        let wl_state_1 = generate_test_workload_spec_with_runtime_config(
            "".to_string(),
            WORKLOAD_NAME_1.to_string(),
            "some runtime".to_string(),
            "config".to_string(),
        );
        let wl_state_3 = generate_test_workload_spec_with_runtime_config(
            AGENT_B.to_string(),
            WORKLOAD_NAME_3.to_string(),
            "some runtime".to_string(),
            "config".to_string(),
        );

        wls_db.initial_state(&vec![wl_state_1, wl_state_3]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_1,
                    "",
                    ExecutionState::not_scheduled(),
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_3,
                    AGENT_B,
                    ExecutionState::initial(),
                )
            ])
        )
    }
}

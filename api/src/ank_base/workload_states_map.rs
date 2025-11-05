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

// use std::collections::{HashMap, hash_map::Entry};
// use api::ank_base;
// use api::ank_base::{ExecutionStateInternal, WorkloadInstanceNameInternal, WorkloadNamed};
// use serde::{Deserialize, Serialize};
// use super::WorkloadState;

use crate::ank_base::{
    ExecutionStateInternal, ExecutionsStatesOfWorkloadInternal, WorkloadInstanceNameInternal,
    WorkloadNamed, WorkloadState, WorkloadStatesMapInternal,
};
use std::collections::{HashMap, hash_map::Entry};

type AgentName = String;
type WorkloadName = String;
type WorkloadId = String;

type ExecutionState = ExecutionStateInternal;

// #[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
// pub struct WorkloadStatesMap(
//     HashMap<AgentName, HashMap<WorkloadName, HashMap<WorkloadId, ExecutionState>>>,
// );

// [impl->swdd~state-map-for-workload-execution-states~2]
impl WorkloadStatesMapInternal {
    pub fn new() -> WorkloadStatesMapInternal {
        Default::default()
    }

    fn to_hashmap(self) -> HashMap<String, HashMap<String, HashMap<String, ExecutionState>>> {
        self.agent_state_map
            .into_iter()
            .map(|(agent_name, wl_map)| {
                (
                    agent_name,
                    wl_map
                        .wl_name_state_map
                        .into_iter()
                        .map(|(wl_name, id_map)| {
                            (
                                wl_name,
                                id_map
                                    .id_state_map
                                    .into_iter()
                                    .map(|(id, exec_state)| (id, exec_state))
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn entry(&mut self, key: String) -> Entry<'_, String, ExecutionsStatesOfWorkloadInternal> {
        self.agent_state_map.entry(key)
    }

    pub fn get_workload_state_for_agent(&self, agent_name: &str) -> Vec<WorkloadState> {
        self.agent_state_map
            .get(agent_name)
            .map(|name_map| {
                name_map
                    .wl_name_state_map
                    .iter()
                    .flat_map(|(wl_name, id_map)| {
                        id_map
                            .id_state_map
                            .iter()
                            .map(move |(wl_id, exec_state)| WorkloadState {
                                instance_name: Some(
                                    WorkloadInstanceNameInternal::new(agent_name, wl_name, wl_id)
                                        .into(),
                                ),
                                execution_state: Some(exec_state.clone().into()),
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
        self.agent_state_map
            .iter()
            .filter(|(agent_name, _)| *agent_name != excluding_agent_name)
            .flat_map(|(agent_name, name_map)| {
                name_map
                    .wl_name_state_map
                    .iter()
                    .flat_map(move |(wl_name, id_map)| {
                        id_map
                            .id_state_map
                            .iter()
                            .map(move |(wl_id, exec_state)| WorkloadState {
                                instance_name: Some(
                                    WorkloadInstanceNameInternal::new(agent_name, wl_name, wl_id)
                                        .into(),
                                ),
                                execution_state: Some(exec_state.clone().into()),
                            })
                    })
            })
            .collect()
    }

    pub fn get_workload_state_for_workload(
        &self,
        instance_name: &WorkloadInstanceNameInternal,
    ) -> Option<&ExecutionState> {
        self.agent_state_map
            .get(instance_name.agent_name())
            .and_then(|name_map| {
                name_map
                    .wl_name_state_map
                    .get(instance_name.workload_name())
            })
            .and_then(|id_map| id_map.id_state_map.get(instance_name.id()))
    }

    pub fn agent_disconnected(&mut self, agent_name: &str) {
        if let Some(agent_states) = self.agent_state_map.get_mut(agent_name) {
            agent_states
                .wl_name_state_map
                .iter_mut()
                .for_each(|(_, id_map)| {
                    id_map.id_state_map.iter_mut().for_each(|(_, exec_state)| {
                        *exec_state = ExecutionState::agent_disconnected()
                    })
                })
        }
    }

    pub fn initial_state(&mut self, workloads: &Vec<WorkloadNamed>) {
        for wl in workloads {
            self.agent_state_map
                .entry(wl.instance_name.agent_name().to_owned())
                .or_default()
                .wl_name_state_map
                .entry(wl.instance_name.workload_name().to_owned())
                .or_default()
                .id_state_map
                .entry(wl.instance_name.id().to_owned())
                .or_insert(if wl.instance_name.agent_name().is_empty() {
                    ExecutionState::not_scheduled()
                } else {
                    ExecutionState::initial()
                });
        }
    }

    pub fn remove(&mut self, instance_name: &WorkloadInstanceNameInternal) {
        if let Some(agent_states) = self.agent_state_map.get_mut(instance_name.agent_name())
            && let Some(workload_states) = agent_states
                .wl_name_state_map
                .get_mut(instance_name.workload_name())
        {
            workload_states.id_state_map.remove(instance_name.id());
            // the following part is needed to cleanup empty paths in the state map
            if workload_states.id_state_map.is_empty() {
                agent_states
                    .wl_name_state_map
                    .remove(instance_name.workload_name());
                if agent_states.wl_name_state_map.is_empty() {
                    self.agent_state_map.remove(instance_name.agent_name());
                }
            }
        }
    }

    pub fn process_new_states(&mut self, workload_states: Vec<WorkloadState>) {
        // for workload_state in workload_states {
        //     if workload_state.execution_state.is_removed() {
        //         self.remove(&workload_state.instance_name);
        //     } else {
        //         self.entry(workload_state.instance_name.agent_name().to_owned())
        //             .or_default()
        //             .wl_name_state_map
        //             .entry(workload_state.instance_name.workload_name().to_owned())
        //             .or_default()
        //             .id_state_map
        //             .insert(
        //                 workload_state.instance_name.id().to_owned(),
        //                 workload_state.execution_state,
        //             );
        //     }
        // }
        for workload_state in workload_states {
            if let Some(instance_name) = workload_state.instance_name && let Some(exec_state) = workload_state.execution_state {
                if exec_state.is_removed() {
                    self.remove(&instance_name);
                } else {
                    self.entry(instance_name.agent_name().to_owned())
                        .or_default()
                        .wl_name_state_map
                        .entry(instance_name.workload_name().to_owned())
                        .or_default()
                        .id_state_map
                        .insert(instance_name.id().to_owned(), exec_state);
                }
            }
        }
    }
}

impl From<WorkloadStatesMapInternal> for Vec<WorkloadState> {
    fn from(value: WorkloadStatesMapInternal) -> Self {
        value
            .agent_state_map
            .into_iter()
            .flat_map(|(agent_name, name_state_map)| {
                name_state_map
                    .into_iter()
                    .flat_map(move |(wl_name, id_state_map)| {
                        let agent_name = agent_name.clone();
                        id_state_map
                            .into_iter()
                            .map(move |(wl_id, exec_state)| WorkloadState {
                                instance_name: WorkloadInstanceNameInternal::new(
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
                                        .map(|(id, exec_state)| {
                                            (id, exec_state.try_into().unwrap())
                                        })
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
    workloads: Vec<WorkloadNamed>,
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

// [utest->swdd~state-map-for-workload-execution-states~2]
#[cfg(test)]
mod tests {
    use super::{
        ExecutionState, WorkloadStatesMap, generate_test_workload_states_map_from_workload_states,
    };
    use crate::objects::{WorkloadState, generate_test_workload_state_with_agent};
    use api::ank_base::WorkloadNamed;
    use api::test_utils::generate_test_workload_with_param;
    use std::vec;

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

        let wl_state_1 = generate_test_workload_with_param::<WorkloadNamed>("", "some runtime")
            .name(WORKLOAD_NAME_1);
        let wl_state_3 =
            generate_test_workload_with_param::<WorkloadNamed>(AGENT_B, "some runtime")
                .name(WORKLOAD_NAME_3);

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

    #[test]
    fn utest_get_workload_state_for_workload_existing_workload() {
        let wls_db = create_test_setup();

        let wl_state = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_1,
            AGENT_A,
            ExecutionState::succeeded(),
        );

        assert_eq!(
            wls_db.get_workload_state_for_workload(&wl_state.instance_name),
            Some(&ExecutionState::succeeded())
        )
    }

    #[test]
    fn utest_get_workload_state_for_workload_not_existing_workload() {
        let wls_db = create_test_setup();

        let wl_state = generate_test_workload_state_with_agent(
            "not_existing_workload",
            AGENT_A,
            ExecutionState::running(),
        );

        assert!(
            wls_db
                .get_workload_state_for_workload(&wl_state.instance_name)
                .is_none()
        )
    }
}

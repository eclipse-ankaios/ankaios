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

use crate::ank_base::{
    ExecutionStateSpec, ExecutionsStatesOfWorkloadSpec, WorkloadInstanceName,
    WorkloadInstanceNameSpec, WorkloadNamed, WorkloadState, WorkloadStateSpec, WorkloadStatesMap,
    WorkloadStatesMapSpec,
};
use std::collections::hash_map::Entry;

// [impl->swdd~api-state-map-for-workload-execution-states~1]
impl WorkloadStatesMapSpec {
    pub fn new() -> WorkloadStatesMapSpec {
        Default::default()
    }

    pub fn entry(&mut self, key: String) -> Entry<'_, String, ExecutionsStatesOfWorkloadSpec> {
        self.agent_state_map.entry(key)
    }

    pub fn get_workload_state_for_agent(&self, agent_name: &str) -> Vec<WorkloadStateSpec> {
        self.agent_state_map
            .get(agent_name)
            .map(|name_map| {
                name_map
                    .wl_name_state_map
                    .iter()
                    .flat_map(|(wl_name, id_map)| {
                        id_map.id_state_map.iter().map(move |(wl_id, exec_state)| {
                            WorkloadStateSpec {
                                instance_name: WorkloadInstanceNameSpec::new(
                                    agent_name, wl_name, wl_id,
                                ),
                                execution_state: exec_state.clone(),
                            }
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_workload_state_excluding_agent(
        &self,
        excluding_agent_name: &str,
    ) -> Vec<WorkloadStateSpec> {
        self.agent_state_map
            .iter()
            .filter(|(agent_name, _)| *agent_name != excluding_agent_name)
            .flat_map(|(agent_name, name_map)| {
                name_map
                    .wl_name_state_map
                    .iter()
                    .flat_map(move |(wl_name, id_map)| {
                        id_map.id_state_map.iter().map(move |(wl_id, exec_state)| {
                            WorkloadStateSpec {
                                instance_name: WorkloadInstanceNameSpec::new(
                                    agent_name, wl_name, wl_id,
                                ),
                                execution_state: exec_state.clone(),
                            }
                        })
                    })
            })
            .collect()
    }

    pub fn get_workload_state_for_workload(
        &self,
        instance_name: &WorkloadInstanceNameSpec,
    ) -> Option<&ExecutionStateSpec> {
        self.agent_state_map
            .get(instance_name.agent_name())
            .and_then(|name_map| {
                name_map
                    .wl_name_state_map
                    .get(instance_name.workload_name())
            })
            .and_then(|id_map| id_map.id_state_map.get(instance_name.id()))
    }

    pub fn get_states_for_workload_name(&self, workload_name: &str) -> Vec<WorkloadStateSpec> {
        self.agent_state_map
            .iter()
            .flat_map(|(agent_name, name_map)| {
                name_map
                    .wl_name_state_map
                    .get(workload_name)
                    .into_iter()
                    .flat_map(move |id_map| {
                        id_map.id_state_map.iter().map(move |(wl_id, exec_state)| {
                            WorkloadStateSpec {
                                instance_name: WorkloadInstanceNameSpec::new(
                                    agent_name,
                                    workload_name,
                                    wl_id,
                                ),
                                execution_state: exec_state.clone(),
                            }
                        })
                    })
            })
            .collect()
    }

    pub fn agent_disconnected(&mut self, agent_name: &str) {
        if let Some(agent_states) = self.agent_state_map.get_mut(agent_name) {
            agent_states
                .wl_name_state_map
                .iter_mut()
                .for_each(|(_, id_map)| {
                    id_map.id_state_map.iter_mut().for_each(|(_, exec_state)| {
                        *exec_state = ExecutionStateSpec::agent_disconnected()
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
                    ExecutionStateSpec::not_scheduled()
                } else {
                    ExecutionStateSpec::initial()
                });
        }
    }

    pub fn remove(&mut self, instance_name: &WorkloadInstanceNameSpec) {
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

    pub fn process_new_states(&mut self, workload_states: Vec<WorkloadStateSpec>) {
        for workload_state in workload_states {
            if workload_state.execution_state.is_removed() {
                self.remove(&workload_state.instance_name);
            } else {
                self.entry(workload_state.instance_name.agent_name().to_owned())
                    .or_default()
                    .wl_name_state_map
                    .entry(workload_state.instance_name.workload_name().to_owned())
                    .or_default()
                    .id_state_map
                    .insert(
                        workload_state.instance_name.id().to_owned(),
                        workload_state.execution_state,
                    );
            }
        }
    }
}

impl From<WorkloadStatesMap> for Vec<WorkloadState> {
    fn from(value: WorkloadStatesMap) -> Self {
        value
            .agent_state_map
            .into_iter()
            .flat_map(|(agent_name, name_state_map)| {
                name_state_map.wl_name_state_map.into_iter().flat_map(
                    move |(wl_name, id_state_map)| {
                        let agent_name = agent_name.clone();
                        id_state_map
                            .id_state_map
                            .into_iter()
                            .map(move |(wl_id, exec_state)| WorkloadState {
                                instance_name: Some(WorkloadInstanceName {
                                    agent_name: agent_name.clone(),
                                    workload_name: wl_name.clone(),
                                    id: wl_id,
                                }),
                                execution_state: Some(exec_state),
                            })
                    },
                )
            })
            .collect()
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

// [utest->swdd~api-state-map-for-workload-execution-states~1]
#[cfg(test)]
mod tests {
    use crate::ank_base::{
        ExecutionStateSpec, WorkloadState, WorkloadStateSpec, WorkloadStatesMap,
        WorkloadStatesMapSpec,
    };
    use crate::test_utils::{fixtures, generate_test_workload_named_with_params};
    use crate::test_utils::{
        generate_test_workload_state_with_agent,
        generate_test_workload_states_map_from_workload_states,
    };
    use std::vec;

    const ADDITIONAL_INFO: &str = "additional_info";

    fn create_test_setup() -> WorkloadStatesMapSpec {
        generate_test_workload_states_map_from_workload_states(vec![
            generate_test_workload_state_with_agent(
                fixtures::WORKLOAD_NAMES[0],
                fixtures::AGENT_NAMES[0],
                ExecutionStateSpec::succeeded(),
            ),
            generate_test_workload_state_with_agent(
                fixtures::WORKLOAD_NAMES[1],
                fixtures::AGENT_NAMES[0],
                ExecutionStateSpec::starting(ADDITIONAL_INFO),
            ),
            generate_test_workload_state_with_agent(
                fixtures::WORKLOAD_NAMES[2],
                fixtures::AGENT_NAMES[1],
                ExecutionStateSpec::running(),
            ),
        ])
    }

    #[test]
    fn utest_workload_states_map_into_vec_of_workload_states() {
        let wls_db: WorkloadStatesMap = create_test_setup().into();

        let mut wls_res: Vec<WorkloadStateSpec> = Into::<Vec<WorkloadState>>::into(wls_db)
            .into_iter()
            .map(|state| state.try_into().unwrap())
            .collect();
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res,
            vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[0],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[1],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::starting(ADDITIONAL_INFO),
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[2],
                    fixtures::AGENT_NAMES[1],
                    ExecutionStateSpec::running()
                ),
            ]
        )
    }

    #[test]
    fn utest_workload_states_store_new() {
        let mut wls_db = create_test_setup();

        let wl_state_new = generate_test_workload_state_with_agent(
            "new_workload",
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::starting(ADDITIONAL_INFO),
        );

        wls_db.process_new_states(vec![wl_state_new.clone()]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[0],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[1],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::starting(ADDITIONAL_INFO),
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[2],
                    fixtures::AGENT_NAMES[1],
                    ExecutionStateSpec::running()
                ),
                wl_state_new
            ])
        )
    }

    #[test]
    fn utest_workload_states_store_update() {
        let mut wls_db = create_test_setup();

        let wl_state_2_update = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );

        wls_db.process_new_states(vec![wl_state_2_update.clone()]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[0],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::succeeded()
                ),
                wl_state_2_update,
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[2],
                    fixtures::AGENT_NAMES[1],
                    ExecutionStateSpec::running()
                )
            ])
        )
    }

    #[test]
    fn utest_get_workload_states_excluding_agent_returns_correct() {
        let wls_db = create_test_setup();

        let mut wls_res = wls_db.get_workload_state_excluding_agent(fixtures::AGENT_NAMES[1]);
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res,
            vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[0],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[1],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::starting(ADDITIONAL_INFO),
                ),
            ]
        )
    }

    #[test]
    fn utest_mark_all_workload_state_for_agent_disconnected() {
        let mut wls_db = create_test_setup();

        wls_db.agent_disconnected(fixtures::AGENT_NAMES[0]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[0],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::agent_disconnected()
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[1],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::agent_disconnected()
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[2],
                    fixtures::AGENT_NAMES[1],
                    ExecutionStateSpec::running()
                ),
            ])
        )
    }

    #[test]
    fn utest_get_workload_state_for_agent_returns_workload_state_of_existing_agent_name() {
        let wls_db = create_test_setup();

        let mut wls_res = wls_db.get_workload_state_for_agent(fixtures::AGENT_NAMES[0]);
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res,
            vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[0],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::succeeded()
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[1],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::starting(ADDITIONAL_INFO),
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
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::removed(),
        );

        let wl_state_3 = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[1],
            ExecutionStateSpec::removed(),
        );

        wls_db.process_new_states(vec![wl_state_1, wl_state_3]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[1],
                    fixtures::AGENT_NAMES[0],
                    ExecutionStateSpec::starting(ADDITIONAL_INFO),
                )
            ])
        )
    }

    #[test]
    fn utest_workload_states_initial_state() {
        let mut wls_db = WorkloadStatesMapSpec::new();

        let wl_state_1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            "",
            fixtures::RUNTIME_NAMES[0],
        );
        let wl_state_3 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
        );

        wls_db.initial_state(&vec![wl_state_1, wl_state_3]);

        assert_eq!(
            wls_db,
            generate_test_workload_states_map_from_workload_states(vec![
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[0],
                    "",
                    ExecutionStateSpec::not_scheduled(),
                ),
                generate_test_workload_state_with_agent(
                    fixtures::WORKLOAD_NAMES[2],
                    fixtures::AGENT_NAMES[1],
                    ExecutionStateSpec::initial(),
                )
            ])
        )
    }

    #[test]
    fn utest_get_workload_state_for_workload_existing_workload() {
        let wls_db = create_test_setup();

        let wl_state = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::succeeded(),
        );

        assert_eq!(
            wls_db.get_workload_state_for_workload(&wl_state.instance_name),
            Some(&ExecutionStateSpec::succeeded())
        )
    }

    #[test]
    fn utest_get_workload_state_for_workload_not_existing_workload() {
        let wls_db = create_test_setup();

        let wl_state = generate_test_workload_state_with_agent(
            "not_existing_workload",
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::running(),
        );

        assert!(
            wls_db
                .get_workload_state_for_workload(&wl_state.instance_name)
                .is_none()
        )
    }

    #[test]
    fn utest_get_workload_states_for_workload_name() {
        let mut wls_db = create_test_setup();

        let wl_state_2_update = generate_test_workload_state_with_agent(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[0],
            ExecutionStateSpec::stopping_requested(),
        );

        wls_db.process_new_states(vec![wl_state_2_update.clone()]);

        let wls_res = wls_db.get_states_for_workload_name(fixtures::WORKLOAD_NAMES[2]);

        assert_eq!(wls_res.len(), 2);

        assert_eq!(
            wls_res
                .iter()
                .find(|state| {
                    state.instance_name.workload_name() == fixtures::WORKLOAD_NAMES[2]
                        && state.instance_name.agent_name() == fixtures::AGENT_NAMES[1]
                })
                .unwrap()
                .execution_state,
            ExecutionStateSpec::running()
        );

        assert_eq!(
            wls_res
                .iter()
                .find(|state| {
                    state.instance_name.workload_name() == fixtures::WORKLOAD_NAMES[2]
                        && state.instance_name.agent_name() == fixtures::AGENT_NAMES[0]
                })
                .unwrap()
                .execution_state,
            ExecutionStateSpec::stopping_requested()
        );
    }
}

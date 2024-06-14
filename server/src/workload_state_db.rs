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

use common::objects::{ExecutionState, WorkloadInstanceName, WorkloadSpec, WorkloadState};
use std::collections::HashMap;

type AgentName = String;

type WorkloadIdStatesMap = HashMap<String, ExecutionState>;
type WorkloadNameStatesMap = HashMap<String, WorkloadIdStatesMap>;
type AgentWorkloadStates = HashMap<AgentName, WorkloadNameStatesMap>;

pub struct WorkloadStateDB {
    stored_states: AgentWorkloadStates,
}

impl WorkloadStateDB {
    pub fn new() -> Self {
        Self {
            stored_states: HashMap::new(),
        }
    }

    // [impl->swdd~server-provides-interface-get-complete-state~1]
    pub fn get_all_workload_states(&self) -> Vec<WorkloadState> {
        self.stored_states
            .iter()
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

    // [impl->swdd~server-distribute-workload-state-on-disconnect~1]
    pub fn get_workload_state_for_agent(&self, agent_name: &str) -> Vec<WorkloadState> {
        self.stored_states
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

    // [impl->swdd~server-informs-a-newly-connected-agent-workload-states~1]
    pub fn get_workload_state_excluding_agent(
        &self,
        excluding_agent_name: &str,
    ) -> Vec<WorkloadState> {
        self.stored_states
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

    // [impl->swdd~server-set-workload-state-on-disconnect~1]
    pub fn agent_disconnected(&mut self, agent_name: &str) {
        if let Some(agent_states) = self.stored_states.get_mut(agent_name) {
            agent_states.iter_mut().for_each(|(_, name_map)| {
                name_map
                    .iter_mut()
                    .for_each(|(_, exec_state)| *exec_state = ExecutionState::agent_disconnected())
            })
        }
    }

    // [impl->swdd~server-sets-state-of-new-workloads-to-pending~1]
    pub fn initial_state(&mut self, workload_specs: &Vec<WorkloadSpec>) {
        for spec in workload_specs {
            self.stored_states
                .entry(spec.instance_name.agent_name().to_owned())
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

    // [impl->swdd~server-deletes-removed-workload-state~1]
    pub fn remove(&mut self, instance_name: &WorkloadInstanceName) {
        if let Some(agent_states) = self.stored_states.get_mut(instance_name.agent_name()) {
            if let Some(workload_states) = agent_states.get_mut(instance_name.workload_name()) {
                workload_states.remove(instance_name.id());
            }
        }
    }

    // [impl->swdd~server-stores-workload-state~1]
    pub fn process_new_states(&mut self, workload_states: Vec<WorkloadState>) {
        workload_states.into_iter().for_each(|workload_state| {
            if workload_state.execution_state.is_removed() {
                self.remove(&workload_state.instance_name);
            } else {
                self.stored_states
                    .entry(workload_state.instance_name.agent_name().to_owned())
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

impl Default for WorkloadStateDB {
    fn default() -> Self {
        Self::new()
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use common::objects::{
        generate_test_workload_spec_with_runtime_config, generate_test_workload_state_with_agent,
        ExecutionState,
    };

    use super::WorkloadStateDB;

    const AGENT_A: &str = "agent_A";
    const AGENT_B: &str = "agent_B";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const WORKLOAD_NAME_4: &str = "workload_4";

    fn create_test_setup() -> WorkloadStateDB {
        let mut wls_db = WorkloadStateDB::new();

        let wl_1_state = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_1,
            AGENT_A,
            ExecutionState::succeeded(),
        );
        let wl_2_state = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_2,
            AGENT_A,
            ExecutionState::starting("additional_info"),
        );
        let wl_3_state = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_3,
            AGENT_B,
            ExecutionState::running(),
        );

        let mut wls1_id_1 = HashMap::new();
        let mut wls2_id_2 = HashMap::new();

        wls1_id_1.insert(
            wl_1_state.instance_name.id().to_owned(),
            wl_1_state.execution_state,
        );
        wls2_id_2.insert(
            wl_2_state.instance_name.id().to_owned(),
            wl_2_state.execution_state,
        );

        let mut wls_agent_a = HashMap::new();
        wls_agent_a.insert(
            wl_1_state.instance_name.workload_name().to_owned(),
            wls1_id_1,
        );
        wls_agent_a.insert(
            wl_2_state.instance_name.workload_name().to_owned(),
            wls2_id_2,
        );

        wls_db
            .stored_states
            .insert(AGENT_A.to_string(), wls_agent_a);

        let mut wls3_id_3 = HashMap::new();
        wls3_id_3.insert(
            wl_3_state.instance_name.id().to_owned(),
            wl_3_state.execution_state,
        );
        let mut wls_agent_b = HashMap::new();
        wls_agent_b.insert(
            wl_3_state.instance_name.workload_name().to_owned(),
            wls3_id_3,
        );
        wls_db
            .stored_states
            .insert(AGENT_B.to_string(), wls_agent_b);

        wls_db
    }

    // [utest->swdd~server-provides-interface-get-complete-state~1]
    #[test]
    fn utest_get_all_workload_states_returns_correct() {
        let wls_db = create_test_setup();

        let mut wls_res = wls_db.get_all_workload_states();
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

    // [utest->swdd~server-stores-workload-state~1]
    #[test]
    fn utest_workload_states_store_new() {
        let mut wls_db = create_test_setup();

        let wl_state_4 = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_4,
            AGENT_A,
            ExecutionState::starting("test info"),
        );

        wls_db.process_new_states(vec![wl_state_4.clone()]);

        let mut wls_res = wls_db.get_all_workload_states();
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
                wl_state_4
            ]
        )
    }

    // [utest->swdd~server-stores-workload-state~1]
    #[test]
    fn utest_workload_states_store_update() {
        let mut wls_db = create_test_setup();

        let wl_state_2_update = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_2,
            AGENT_A,
            ExecutionState::running(),
        );

        wls_db.process_new_states(vec![wl_state_2_update.clone()]);

        let mut wls_res = wls_db.get_all_workload_states();
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
                wl_state_2_update,
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_3,
                    AGENT_B,
                    ExecutionState::running()
                )
            ]
        )
    }

    // [utest->swdd~server-informs-a-newly-connected-agent-workload-states~1]
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

    // [utest->swdd~server-set-workload-state-on-disconnect~1]
    #[test]
    fn utest_mark_all_workload_state_for_agent_disconnected() {
        let mut wls_db = create_test_setup();

        let mut wls_res = wls_db.get_all_workload_states();
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        wls_db.agent_disconnected(AGENT_A);
        let mut wls_res_marked = wls_db.get_all_workload_states();
        wls_res_marked.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res_marked,
            vec![
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
            ]
        )
    }

    // [utest->swdd~server-distribute-workload-state-on-disconnect~1]
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

    // [utest->swdd~server-distribute-workload-state-on-disconnect~1]
    #[test]
    fn utest_get_workload_state_for_agent_returns_empty_list_of_non_existing_agent_name() {
        let wls_db = create_test_setup();
        assert_eq!(
            wls_db.get_workload_state_for_agent("non_existing_agent"),
            vec![]
        );
    }

    // [utest->swdd~server-deletes-removed-workload-state~1]
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

        let mut wls_res = wls_db.get_all_workload_states();
        wls_res.sort_by(|a, b| {
            a.instance_name
                .workload_name()
                .cmp(b.instance_name.workload_name())
        });

        assert_eq!(
            wls_res,
            vec![generate_test_workload_state_with_agent(
                WORKLOAD_NAME_2,
                AGENT_A,
                ExecutionState::starting("additional_info"),
            )]
        )
    }

    // [utest->swdd~server-sets-state-of-new-workloads-to-pending~1]
    #[test]
    fn utest_workload_states_initial_state() {
        let mut wls_db = WorkloadStateDB::new();

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

        let mut wls_res = wls_db.get_all_workload_states();
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
                    "",
                    ExecutionState::not_scheduled(),
                ),
                generate_test_workload_state_with_agent(
                    WORKLOAD_NAME_3,
                    AGENT_B,
                    ExecutionState::initial(),
                )
            ]
        )
    }
}

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

use common::objects::{ExecutionState, WorkloadState};
use std::collections::HashMap;

type WorkloadName = String;
type AgentName = String;

type WorkloadStatesMap = HashMap<WorkloadName, Vec<WorkloadState>>;
type AgentWorkloadStates = HashMap<AgentName, WorkloadStatesMap>;

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
            .flat_map(|(_, workload_states)| {
                workload_states
                    .iter()
                    .flat_map(|(_, workload_state)| workload_state.to_owned())
            })
            .collect()
    }

    // [impl->swdd~server-distribute-workload-state-on-disconnect~1]
    pub fn get_workload_state_for_agent(&self, agent_name: &str) -> Vec<WorkloadState> {
        self.stored_states
            .get(agent_name)
            .map(|x| x.iter().flat_map(|(_, v)| v.to_owned()).collect())
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
            .flat_map(|(_, workload_states)| {
                workload_states
                    .iter()
                    .flat_map(|(_, workload_state)| workload_state.to_owned())
            })
            .collect()
    }

    // [impl->swdd~server-set-workload-state-on-disconnect~1]
    pub fn agent_disconnected(&mut self, agent_name: &str) {
        if let Some(agent_states) = self.stored_states.get_mut(agent_name) {
            agent_states.iter_mut().for_each(|(_, states)| {
                states.iter_mut().for_each(|wl_state| {
                    wl_state.execution_state = ExecutionState::agent_disconnected()
                })
            })
        }
    }

    // [impl->swdd~server-deletes-removed-workload-state~1]
    pub fn remove(&mut self, state_to_remove: WorkloadState) {
        if let Some(agent_states) = self
            .stored_states
            .get_mut(state_to_remove.instance_name.agent_name())
        {
            if state_to_remove.workload_id.is_empty() {
                agent_states.remove(state_to_remove.instance_name.workload_name());
            } else if let Some(wl_states) =
                agent_states.get_mut(state_to_remove.instance_name.workload_name())
            {
                wl_states.retain(|wl_state| wl_state.workload_id != state_to_remove.workload_id)
            }
        }
    }

    // [impl->swdd~server-stores-workload-state~1]
    pub fn proccess_new_states(&mut self, workload_states: Vec<WorkloadState>) {
        workload_states.into_iter().for_each(|workload_state| {
            if workload_state.execution_state.is_removed() {
                self.remove(workload_state);
            } else {
                self.stored_states
                    .entry(workload_state.instance_name.agent_name().to_owned())
                    .or_default()
                    .entry(workload_state.instance_name.workload_name().to_owned())
                    .or_default()
                    .push(workload_state);
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

    use common::objects::{generate_test_workload_state_with_agent, ExecutionState};

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

        let mut wls = HashMap::new();
        wls.insert(
            wl_1_state.instance_name.workload_name().to_owned(),
            vec![wl_1_state],
        );
        wls.insert(
            wl_2_state.instance_name.workload_name().to_owned(),
            vec![wl_2_state],
        );
        wls_db.stored_states.insert(AGENT_A.to_string(), wls);

        let mut wls_2 = HashMap::new();
        wls_2.insert(
            wl_3_state.instance_name.workload_name().to_owned(),
            vec![wl_3_state],
        );
        wls_db.stored_states.insert(AGENT_B.to_string(), wls_2);

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

        wls_db.proccess_new_states(vec![wl_state_4.clone()]);

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

        let mut wl_state_1 = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_1,
            AGENT_A,
            ExecutionState::removed(),
        );
        wl_state_1.workload_id = "".to_string();

        let wl_state_3 = generate_test_workload_state_with_agent(
            WORKLOAD_NAME_3,
            AGENT_B,
            ExecutionState::removed(),
        );

        wls_db.proccess_new_states(vec![wl_state_1, wl_state_3]);

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
}

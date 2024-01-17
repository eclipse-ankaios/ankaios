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

type WorkloadStatesMap = HashMap<String, common::objects::ExecutionState>;
// type WorkloadStates = HashMap<String, bool>;
type AgentWorkloadStates = HashMap<String, WorkloadStatesMap>;

pub struct WorkloadStateDB {
    stored_states: AgentWorkloadStates,
}

impl WorkloadStateDB {
    pub fn new() -> Self {
        Self {
            stored_states: HashMap::new(),
        }
    }

    pub fn get_all_workload_states(&self) -> Vec<WorkloadState> {
        let mut all_workload_states: Vec<WorkloadState> = vec![];
        for (agent, workload_states) in &self.stored_states {
            let mut x: Vec<WorkloadState> = workload_states
                .iter()
                .map(|(workload_name, state)| WorkloadState {
                    workload_name: workload_name.clone(),
                    agent_name: agent.clone(),
                    execution_state: state.clone(),
                })
                .collect();
            all_workload_states.append(&mut x);
        }

        all_workload_states
    }

    pub fn get_workload_state_for_agent(&self, agent_name: &str) -> Vec<WorkloadState> {
        if let Some(workload_states) = self.stored_states.get(agent_name) {
            return workload_states
                .clone()
                .into_iter()
                .map(|(workload_name, execution_state)| WorkloadState {
                    workload_name,
                    agent_name: agent_name.to_owned(),
                    execution_state,
                })
                .collect();
        }

        Vec::new()
    }

    pub fn get_workload_state_excluding_agent(
        &self,
        excluding_agent_name: &str,
    ) -> Vec<WorkloadState> {
        let mut output_workload_states = Vec::new();
        for (agent_name, workload_states) in self.stored_states.iter() {
            if agent_name != excluding_agent_name {
                for (workload_name, execution_state) in workload_states.iter() {
                    output_workload_states.push(WorkloadState {
                        workload_name: workload_name.clone(),
                        agent_name: agent_name.clone(),
                        execution_state: execution_state.clone(),
                    });
                }
            }
        }

        output_workload_states
    }

    pub fn mark_all_workload_state_for_agent_unknown(&mut self, agent_name: &str) {
        if let Some(workload_states) = self.stored_states.get_mut(agent_name) {
            workload_states
                .iter_mut()
                .for_each(|(_, execution_state)| *execution_state = ExecutionState::ExecUnknown);
        }
    }

    pub fn insert(&mut self, workload_states: Vec<WorkloadState>) {
        for workload_state in workload_states {
            if let Some(current_states) = self
                .stored_states
                .get_mut(workload_state.agent_name.as_str())
            {
                if let Some(old_exec_state) = current_states
                    .insert(workload_state.workload_name, workload_state.execution_state)
                {
                    log::debug!("Replaced old execution state: '{old_exec_state:?}'");
                }
            } else {
                let mut new_current_states = HashMap::new();
                new_current_states
                    .insert(workload_state.workload_name, workload_state.execution_state);
                self.stored_states
                    .insert(workload_state.agent_name, new_current_states);
            }
        }
    }

    pub fn remove(&mut self, workload_state: &WorkloadState) {
        if let Some(current_states_agent) = self
            .stored_states
            .get_mut(workload_state.agent_name.as_str())
        {
            current_states_agent.remove(&workload_state.workload_name);
        } else {
            log::warn!(
                "Failed to remove the workload state {:?}. The agent is not in the map.",
                workload_state
            );
        }
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

    use common::objects::{ExecutionState, WorkloadState};

    use super::WorkloadStateDB;

    fn create_test_setup_1(agent_name: &str) -> WorkloadStateDB {
        let mut wls_db = WorkloadStateDB::new();
        let mut wls = HashMap::new();
        wls.insert("workload1".to_owned(), ExecutionState::ExecSucceeded);
        wls.insert("workload2".to_owned(), ExecutionState::ExecStarting);
        wls_db.stored_states.insert(agent_name.to_string(), wls);
        wls_db
    }

    fn create_test_setup_2(agent_name_1: &str, agent_name_2: &str) -> WorkloadStateDB {
        let mut wls_db = WorkloadStateDB::new();

        let mut wls = HashMap::new();
        wls.insert("workload1".to_owned(), ExecutionState::ExecSucceeded);
        wls.insert("workload2".to_owned(), ExecutionState::ExecStarting);
        wls_db.stored_states.insert(agent_name_1.to_string(), wls);

        let mut wls_2 = HashMap::new();
        wls_2.insert("workload3".to_owned(), ExecutionState::ExecRunning);
        wls_db.stored_states.insert(agent_name_2.to_string(), wls_2);

        wls_db
    }

    #[test]
    fn utest_get_all_workload_states_returns_correct() {
        let agent_name_1 = "test_agent_1";
        let agent_name_2 = "test_agent_2";
        let wls_db = create_test_setup_2(agent_name_1, agent_name_2);

        let mut wls_res = wls_db.get_all_workload_states();
        wls_res.sort_by(|a, b| a.workload_name.cmp(&b.workload_name));

        assert_eq!(
            wls_res,
            vec![
                WorkloadState {
                    agent_name: agent_name_1.to_string(),
                    workload_name: "workload1".to_string(),
                    execution_state: ExecutionState::ExecSucceeded
                },
                WorkloadState {
                    agent_name: agent_name_1.to_string(),
                    workload_name: "workload2".to_string(),
                    execution_state: ExecutionState::ExecStarting
                },
                WorkloadState {
                    agent_name: agent_name_2.to_string(),
                    workload_name: "workload3".to_string(),
                    execution_state: ExecutionState::ExecRunning
                }
            ]
        )
    }

    #[test]
    fn utest_mark_all_workload_state_for_agent_unknown() {
        let agent_name_1 = "test_agent_1";
        let agent_name_2 = "test_agent_2";
        let mut wls_db = create_test_setup_2(agent_name_1, agent_name_2);

        let mut wls_res = wls_db.get_all_workload_states();
        wls_res.sort_by(|a, b| a.workload_name.cmp(&b.workload_name));

        assert_eq!(
            wls_res,
            vec![
                WorkloadState {
                    agent_name: agent_name_1.to_string(),
                    workload_name: "workload1".to_string(),
                    execution_state: ExecutionState::ExecSucceeded
                },
                WorkloadState {
                    agent_name: agent_name_1.to_string(),
                    workload_name: "workload2".to_string(),
                    execution_state: ExecutionState::ExecStarting
                },
                WorkloadState {
                    agent_name: agent_name_2.to_string(),
                    workload_name: "workload3".to_string(),
                    execution_state: ExecutionState::ExecRunning
                }
            ]
        );

        wls_db.mark_all_workload_state_for_agent_unknown(agent_name_1);
        let mut wls_res_marked = wls_db.get_all_workload_states();
        wls_res_marked.sort_by(|a, b| a.workload_name.cmp(&b.workload_name));

        assert_eq!(
            wls_res_marked,
            vec![
                WorkloadState {
                    agent_name: agent_name_1.to_string(),
                    workload_name: "workload1".to_string(),
                    execution_state: ExecutionState::ExecUnknown
                },
                WorkloadState {
                    agent_name: agent_name_1.to_string(),
                    workload_name: "workload2".to_string(),
                    execution_state: ExecutionState::ExecUnknown
                },
                WorkloadState {
                    agent_name: agent_name_2.to_string(),
                    workload_name: "workload3".to_string(),
                    execution_state: ExecutionState::ExecRunning
                }
            ]
        )
    }

    #[test]
    fn utest_get_workload_state_for_agent_returns_workload_state_of_existing_agent_name() {
        let agent_name = "test_agent";
        let wls_db = create_test_setup_1(agent_name);

        let mut wls_res = wls_db.get_workload_state_for_agent(agent_name);
        wls_res.sort_by(|a, b| a.workload_name.cmp(&b.workload_name));

        assert_eq!(
            wls_res,
            vec![
                WorkloadState {
                    agent_name: agent_name.to_string(),
                    workload_name: "workload1".to_string(),
                    execution_state: ExecutionState::ExecSucceeded
                },
                WorkloadState {
                    agent_name: agent_name.to_string(),
                    workload_name: "workload2".to_string(),
                    execution_state: ExecutionState::ExecStarting
                }
            ]
        )
    }

    #[test]
    fn utest_get_workload_state_for_agent_returns_empty_list_of_non_existing_agent_name() {
        let wls_db = create_test_setup_1("test_agent");
        assert_eq!(
            wls_db.get_workload_state_for_agent("non_existing_agent"),
            vec![]
        );
    }
}

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

use common::{
    commands::CompleteState,
    objects::{DeleteCondition, State, WorkloadSpec},
};
use std::collections::HashMap;

type DeleteGraph = HashMap<String, HashMap<String, DeleteCondition>>;

pub struct ServerState {
    state: CompleteState,
    delete_conditions: DeleteGraph,
}

fn find_cycles(
    cycles: &mut Vec<Vec<String>>,
    recursion_stack: &mut Vec<String>,
    visited: &mut Vec<String>,
    state: &State,
    workload_spec: &WorkloadSpec,
) -> bool {
    if !visited.contains(&workload_spec.name) {
        visited.push(workload_spec.name.clone());
        recursion_stack.push(workload_spec.name.clone());

        for (workload_name, _) in workload_spec.dependencies.iter() {
            if !visited.contains(workload_name) {
                if let Some(next_workload) = state.workloads.get(workload_name) {
                    if find_cycles(cycles, recursion_stack, visited, state, next_workload) {
                        return true;
                    }
                }
            } else if recursion_stack.contains(workload_name) {
                let cycle_start = recursion_stack
                    .iter()
                    .position(|n| n == workload_name)
                    .unwrap_or(0);
                let mut cycle = recursion_stack[cycle_start..].to_owned();
                cycle.push(workload_name.clone());
                cycles.push(cycle);
                return true;
            }
        }
    }

    if let Some(index) = recursion_stack
        .iter()
        .position(|n| n == &workload_spec.name)
    {
        recursion_stack.remove(index);
    }
    false
}

impl ServerState {
    fn new(state: CompleteState, delete_conditions: DeleteGraph) -> Self {
        ServerState {
            state,
            delete_conditions,
        }
    }

    fn cyclic_dependencies(&self) -> Result<(), String> {
        let mut cycles = Vec::new();
        let mut visited = Vec::new();
        for (workload_name, workload_spec) in self.state.current_state.workloads.iter() {
            if !visited.contains(workload_name) {
                let mut recursion_stack = Vec::new();
                if find_cycles(
                    &mut cycles,
                    &mut recursion_stack,
                    &mut visited,
                    &self.state.current_state,
                    workload_spec,
                ) {
                    log::info!("{:?}", cycles);
                    return Err(format!(
                        "dependencies are conatining a cycle: '{:?}'",
                        cycles,
                    ));
                }
            }
        }

        Ok(())
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
    use super::*;
    use common::{
        objects::AddCondition,
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };

    const AGENT_NAME: &str = "agent_A";
    const RUNTIME: &str = "runtime X";
    const REQUEST_ID: &str = "request@id";

    #[test]
    fn utest_detect_cycle_in_dependencies() {
        /*
            Graph:
            A -> B -> C
            C -> A
            B -> D
            Cycle:
            A -> B -> C -> A
        */
        let _ = env_logger::builder().is_test(true).try_init();

        let mut workload_a = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "A".to_string(),
            RUNTIME.to_string(),
        );

        workload_a.dependencies = HashMap::from([("B".into(), AddCondition::AddCondRunning)]);

        let mut workload_b = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "B".to_string(),
            RUNTIME.to_string(),
        );

        workload_b.dependencies = HashMap::from([
            ("C".into(), AddCondition::AddCondSucceeded),
            // ("D".into(), AddCondition::AddCondSucceeded),
        ]);

        let mut workload_c = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "C".to_string(),
            RUNTIME.to_string(),
        );

        workload_c.dependencies = HashMap::from([
            ("D".into(), AddCondition::AddCondRunning),
            ("A".into(), AddCondition::AddCondRunning),
        ]);

        let mut workload_d = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "D".to_string(),
            RUNTIME.to_string(),
        );

        // workload_d.dependencies = HashMap::from([
        //     ("C".into(), AddCondition::AddCondRunning),
        //     ("B".into(), AddCondition::AddCondRunning),
        // ]);
        workload_d.dependencies.clear();

        let mut complete_state = generate_test_complete_state(
            REQUEST_ID.to_string(),
            vec![workload_a, workload_b, workload_c, workload_d],
        );
        complete_state.workload_states.clear();

        log::info!("{:#?}", complete_state);
        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.cyclic_dependencies();
        assert!(result.is_err());
    }

    #[test]
    fn utest_detect_cycle_in_dependencies_not_detectable() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut workload_a = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "A".to_string(),
            RUNTIME.to_string(),
        );

        workload_a.dependencies = HashMap::from([("B".into(), AddCondition::AddCondRunning)]);

        let mut workload_b = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "B".to_string(),
            RUNTIME.to_string(),
        );

        workload_b.dependencies = HashMap::from([("C".into(), AddCondition::AddCondSucceeded)]);

        let mut workload_c = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "C".to_string(),
            RUNTIME.to_string(),
        );

        workload_c.dependencies = HashMap::from([("F".into(), AddCondition::AddCondRunning)]);

        let mut workload_f = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "F".to_string(),
            RUNTIME.to_string(),
        );

        workload_f.dependencies = HashMap::from([("E".into(), AddCondition::AddCondRunning)]);

        let mut workload_e = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "E".to_string(),
            RUNTIME.to_string(),
        );

        workload_e.dependencies = HashMap::from([("D".into(), AddCondition::AddCondRunning)]);

        let mut workload_d = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "D".to_string(),
            RUNTIME.to_string(),
        );

        workload_d.dependencies = HashMap::from([("A".into(), AddCondition::AddCondRunning)]);

        let mut complete_state = generate_test_complete_state(
            REQUEST_ID.to_string(),
            vec![
                workload_a, workload_b, workload_c, workload_f, workload_e, workload_d,
            ],
        );
        complete_state.workload_states.clear();

        log::info!("{:#?}", complete_state);
        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.cyclic_dependencies();
        assert!(result.is_err());
    }
}

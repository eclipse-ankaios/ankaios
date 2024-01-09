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
use std::collections::{HashMap, HashSet};

type DeleteGraph = HashMap<String, HashMap<String, DeleteCondition>>;

pub struct ServerState {
    state: CompleteState,
    delete_conditions: DeleteGraph,
}

enum Visited {
    Not,
    Partially,
    Full,
}

fn dfs(
    recursion_stack_counter: &mut usize,
    recursion_stack: &mut HashMap<String, usize>,
    visited: &mut HashSet<String>,
    state: &State,
    workload_spec: &WorkloadSpec,
) -> Option<Vec<String>> {
    visited.insert(workload_spec.name.clone());
    *recursion_stack_counter += 1;
    recursion_stack.insert(workload_spec.name.clone(), *recursion_stack_counter);
    log::info!("Find cycles for workload = '{}'", workload_spec.name);
    for (workload_name, _) in workload_spec.dependencies.iter() {
        if !visited.contains(workload_name) {
            log::info!("'{}' not visited", workload_name);
            if let Some(next_workload) = state.workloads.get(workload_name) {
                log::info!("get next workload spec of dependency = '{}'", workload_name);
                if let Some(cycle) = dfs(
                    recursion_stack_counter,
                    recursion_stack,
                    visited,
                    state,
                    next_workload,
                ) {
                    return Some(cycle);
                }
            }
        } else if recursion_stack.contains_key(workload_name) {
            log::info!("'{}' is in recursion stack => cylce!", workload_name);
            let mut rec_stack: Vec<(&String, &usize)> = recursion_stack.iter().collect();
            rec_stack
                .sort_by(|(_, left_counter), (_, right_counter)| left_counter.cmp(right_counter));
            let cycle_start = rec_stack
                .iter()
                .position(|(name, _)| name == &workload_name)
                .unwrap_or(0);
            let mut cycle: Vec<String> = rec_stack[cycle_start..]
                .iter()
                .map(|(n, _)| n.to_owned().clone())
                .collect();
            cycle.push(workload_name.clone());
            return Some(cycle);
        }
    }
    recursion_stack.remove(&workload_spec.name);

    None
}

impl ServerState {
    fn new(state: CompleteState, delete_conditions: DeleteGraph) -> Self {
        ServerState {
            state,
            delete_conditions,
        }
    }

    fn has_cyclic_dependencies(&self) -> Result<(), String> {
        let mut visited = HashSet::new();
        for (workload_name, workload_spec) in self.state.current_state.workloads.iter() {
            log::info!("searching for workload = '{}'", workload_name);
            if !visited.contains(workload_name) {
                let mut recursion_stack_id: usize = 0;
                let mut recursion_stack: HashMap<String, usize> = HashMap::new();
                if let Some(cycle) = dfs(
                    &mut recursion_stack_id,
                    &mut recursion_stack,
                    &mut visited,
                    &self.state.current_state,
                    workload_spec,
                ) {
                    log::info!("{:?}", cycle);
                    return Err(format!(
                        "dependencies are conatining a cycle: '{:?}'",
                        cycle,
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
    fn utest_detect_cycle_in_dependencies_1() {
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

        workload_c.dependencies = HashMap::from([
            ("D".into(), AddCondition::AddCondRunning),
            ("A".into(), AddCondition::AddCondRunning),
        ]);

        let mut workload_d = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "D".to_string(),
            RUNTIME.to_string(),
        );

        workload_d.dependencies.clear();

        let mut complete_state = generate_test_complete_state(
            REQUEST_ID.to_string(),
            vec![workload_a, workload_b, workload_c, workload_d],
        );
        complete_state.workload_states.clear();

        log::info!("{:#?}", complete_state);
        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
    }

    #[test]
    fn utest_detect_cycle_in_dependencies_2() {
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
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
    }

    #[test]
    fn utest_detect_cycle_in_dependencies_3() {
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
            ("A".into(), AddCondition::AddCondSucceeded),
        ]);

        let mut workload_c = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            "C".to_string(),
            RUNTIME.to_string(),
        );

        workload_c.dependencies.clear();

        let mut complete_state = generate_test_complete_state(
            REQUEST_ID.to_string(),
            vec![workload_a, workload_b, workload_c],
        );
        complete_state.workload_states.clear();

        log::info!("{:#?}", complete_state);
        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
    }
}

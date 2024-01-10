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
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::Display,
};

pub type DeleteGraph = HashMap<String, HashMap<String, DeleteCondition>>;

pub struct ServerState {
    state: CompleteState,
    delete_conditions: DeleteGraph,
}

struct BackEdge<T> {
    from: T,
    to: T,
}

impl<T> BackEdge<T> {
    fn new(from: T, to: T) -> Self {
        BackEdge { from, to }
    }
}

impl<T> std::fmt::Display for BackEdge<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}' -> '{}' (back edge)", self.from, self.to)
    }
}

fn dfs(
    recursion_stack: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    state: &State,
    workload_spec: &WorkloadSpec,
) -> Option<BackEdge<String>> {
    visited.insert(workload_spec.name.clone());
    recursion_stack.insert(workload_spec.name.clone());
    let last_recursion_stack_element = &workload_spec.name;
    for (workload_name, _) in workload_spec.dependencies.iter() {
        if !visited.contains(workload_name) {
            log::debug!("'{}' not visited", workload_name);
            if let Some(next_workload) = state.workloads.get(workload_name) {
                log::debug!(
                    "get next workload spec of dependency = '{}', path = '{:?}'",
                    workload_name,
                    recursion_stack
                );
                if let Some(cycle) = dfs(recursion_stack, visited, state, next_workload) {
                    return Some(cycle);
                }
            }
        } else if recursion_stack.contains(workload_name) {
            log::debug!(
                "cycle from '{}' -> ... -> {} -> {}",
                workload_name,
                last_recursion_stack_element,
                workload_name
            );
            return Some(BackEdge::new(
                last_recursion_stack_element.clone(),
                workload_name.to_string(),
            ));
        }
    }
    recursion_stack.remove(&workload_spec.name);
    log::debug!("remove '{}' from path.", workload_spec.name);
    None
}

impl ServerState {
    pub fn new(state: CompleteState, delete_conditions: DeleteGraph) -> Self {
        ServerState {
            state,
            delete_conditions,
        }
    }

    pub fn has_cyclic_dependencies(&self) -> Result<(), String> {
        let mut visited = HashSet::new();
        for (workload_name, workload_spec) in self.state.current_state.workloads.iter() {
            if !visited.contains(workload_name) {
                log::debug!("searching for workload = '{}'", workload_name);
                let mut recursion_stack = HashSet::new();
                if let Some(back_edge) = dfs(
                    &mut recursion_stack,
                    &mut visited,
                    &self.state.current_state,
                    workload_spec,
                ) {
                    // log::debug!("cycle from '{}' -> ... -> {}", back_edge.to, back_edge,);
                    return Err(format!(
                        "cycle from '{}' -> ... -> {}",
                        back_edge.to, back_edge,
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn has_cyclic_dependencies_iterative(&self) -> Result<(), String> {
        let mut stack: VecDeque<&String> = VecDeque::new();
        let mut visited: HashSet<&String> = HashSet::new();
        let mut path: VecDeque<&String> = VecDeque::new();
        let mut data: Vec<&String> = self.state.current_state.workloads.keys().collect();
        data.sort();

        for workload_name in data {
            if visited.contains(workload_name) {
                continue;
            }

            log::debug!("searching for workload = '{}'", workload_name);
            stack.push_front(workload_name);
            while let Some(head) = stack.front() {
                let dependencies = self
                    .state
                    .current_state
                    .workloads
                    .get(*head)
                    .ok_or_else(|| format!("workload '{head}' not found."))?;

                if !visited.contains(head) {
                    log::debug!("visit '{}'", head);
                    visited.insert(head);
                    path.push_back(head);
                } else {
                    log::debug!("remove '{}' from path", head);
                    path.pop_back();
                    stack.pop_front();
                }

                let mut dependencies: Vec<&String> = dependencies.dependencies.keys().collect();
                dependencies.sort();

                for dependency in dependencies {
                    if !visited.contains(dependency) {
                        stack.push_front(dependency);
                    } else if path.contains(&dependency) {
                        let error_msg = format!(
                            "cycle found '{}' -> ... -> {} -> {}",
                            dependency,
                            path.pop_back().unwrap(),
                            dependency
                        );
                        log::debug!("iterative {}", error_msg);
                        return Err("cycle found.".to_string());
                    }
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

        let complete_state = CompleteStateBuilder::default()
            .workload_spec("A")
            .workload_spec("B")
            .workload_spec("C")
            .workload_spec("D")
            .dependency_for_workload("A", "B", AddCondition::AddCondRunning)
            .dependency_for_workload("B", "C", AddCondition::AddCondRunning)
            .dependency_for_workload("C", "D", AddCondition::AddCondRunning)
            .dependency_for_workload("C", "A", AddCondition::AddCondRunning)
            .build();

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies_iterative();
        assert!(result.is_err());
        log::info!("--------------- recursive ---------------");
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
    }

    #[test]
    fn utest_detect_cycle_in_dependencies_2() {
        let _ = env_logger::builder().is_test(true).try_init();

        let complete_state = CompleteStateBuilder::default()
            .workload_spec("A")
            .workload_spec("B")
            .workload_spec("C")
            .workload_spec("D")
            .workload_spec("E")
            .workload_spec("F")
            .dependency_for_workload("A", "B", AddCondition::AddCondRunning)
            .dependency_for_workload("B", "C", AddCondition::AddCondRunning)
            .dependency_for_workload("C", "F", AddCondition::AddCondRunning)
            .dependency_for_workload("F", "E", AddCondition::AddCondRunning)
            .dependency_for_workload("E", "D", AddCondition::AddCondRunning)
            .dependency_for_workload("D", "A", AddCondition::AddCondRunning)
            .build();

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies_iterative();
        assert!(result.is_err());
        log::info!("{}", result.err().unwrap());
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
        log::info!("{}", result.err().unwrap());
    }

    #[test]
    fn utest_detect_cycle_in_dependencies_3() {
        let _ = env_logger::builder().is_test(true).try_init();

        let complete_state = CompleteStateBuilder::default()
            .workload_spec("A")
            .workload_spec("B")
            .workload_spec("C")
            .dependency_for_workload("A", "B", AddCondition::AddCondRunning)
            .dependency_for_workload("B", "C", AddCondition::AddCondSucceeded)
            .dependency_for_workload("B", "A", AddCondition::AddCondSucceeded)
            .build();

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
        let result = server_state.has_cyclic_dependencies_iterative();
        assert!(result.is_err());
    }

    #[test]
    fn utest_detect_self_cycle_in_dependencies() {
        let _ = env_logger::builder().is_test(true).try_init();

        let complete_state = CompleteStateBuilder::default()
            .workload_spec("A")
            .dependency_for_workload("A", "A", AddCondition::AddCondRunning)
            .build();

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
        let result = server_state.has_cyclic_dependencies_iterative();
        assert!(result.is_err());

        let complete_state = CompleteStateBuilder::default()
            .workload_spec("A")
            .workload_spec("B")
            .dependency_for_workload("A", "B", AddCondition::AddCondRunning)
            .dependency_for_workload("B", "B", AddCondition::AddCondRunning)
            .build();

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        let result = server_state.has_cyclic_dependencies();
        assert!(result.is_err());
        let result = server_state.has_cyclic_dependencies_iterative();
        assert!(result.is_err());
    }

    #[test]
    fn utest_detect_cycle_in_dependencies_performance_1000_nodes() {
        let _ = env_logger::builder().is_test(true).try_init();
        const AMOUNT_OF_WORKLOADS: usize = 1000;
        use rand::{thread_rng, Rng};
        let root_name: String = thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(thread_rng().gen_range(10..30))
            .map(|x| x as char)
            .collect();

        let mut workload_root = generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            root_name.clone(),
            RUNTIME.to_string(),
        );
        workload_root.dependencies.clear();

        let mut dependencies = vec![workload_root];
        for i in 1..AMOUNT_OF_WORKLOADS {
            let random_workload_name: String = thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(thread_rng().gen_range(10..30))
                .map(|x| x as char)
                .collect();
            let workload_name = format!("{}{}", random_workload_name, i); // concatenate with index to ensure unique name in collection
            let workload_i = generate_test_workload_spec_with_param(
                AGENT_NAME.to_string(),
                workload_name,
                RUNTIME.to_string(),
            );

            dependencies.last_mut().unwrap().dependencies =
                HashMap::from([(workload_i.name.clone(), AddCondition::AddCondRunning)]);
            dependencies.push(workload_i);
        }

        dependencies.last_mut().unwrap().dependencies =
            HashMap::from([(root_name, AddCondition::AddCondRunning)]);

        let mut complete_state = generate_test_complete_state(REQUEST_ID.to_string(), dependencies);
        complete_state.workload_states.clear();
        assert_eq!(
            complete_state.current_state.workloads.len(),
            AMOUNT_OF_WORKLOADS
        );

        let server_state = ServerState::new(complete_state, DeleteGraph::new());
        use std::time::Instant;
        let start = Instant::now();
        let result = server_state.has_cyclic_dependencies();
        let duration = start.elapsed();
        assert!(result.is_err());
        log::info!("{}", result.err().unwrap());
        log::info!("time recursive cyclic dependency check: '{:?}'", duration);

        let start = Instant::now();
        let result = server_state.has_cyclic_dependencies_iterative();
        let duration = start.elapsed();
        assert!(result.is_err());
        log::info!("{}", result.err().unwrap());
        log::info!("time iterative cyclic dependency check: '{:?}'", duration);
    }

    struct CompleteStateBuilder(CompleteState);
    impl CompleteStateBuilder {
        fn default() -> Self {
            let mut complete_state =
                generate_test_complete_state(REQUEST_ID.to_string(), Vec::new());
            complete_state.workload_states.clear();
            CompleteStateBuilder(complete_state)
        }

        fn workload_spec(mut self, workload_name: &str) -> Self {
            let mut test_workload_spec = generate_test_workload_spec_with_param(
                AGENT_NAME.into(),
                workload_name.into(),
                RUNTIME.into(),
            );
            test_workload_spec.dependencies.clear();
            self.0
                .current_state
                .workloads
                .insert(workload_name.into(), test_workload_spec);
            self
        }

        fn dependency_for_workload(
            mut self,
            workload_name: &str,
            dependency_name: &str,
            add_condition: AddCondition,
        ) -> Self {
            self.0
                .current_state
                .workloads
                .get_mut(workload_name)
                .and_then(|w_spec| {
                    w_spec
                        .dependencies
                        .insert(dependency_name.into(), add_condition)
                });
            self
        }

        fn build(self) -> CompleteState {
            self.0
        }
    }
}

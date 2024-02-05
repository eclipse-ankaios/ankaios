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
use common::objects::State;
use std::collections::{HashSet, VecDeque};

/// Returns an Option containing the workload dependency that is part of a cycle
/// or [`None`] if no cycles are detected
///
/// The method implements the iterative depth search first (dfs) algorithm to
/// detect a cycle in the directed graph for inter workload dependencies within a state.
///
/// # Arguments
///
/// * `state` - The State with workloads representing the directed graph to check for a cycle
/// * `start_nodes` - Start visiting the graph only for the passed workloads
///                   if [`None`] the search is started from all workloads of the state
///
pub fn dfs(state: &State, start_nodes: Option<Vec<&String>>) -> Option<String> {
    log::trace!(
        "Execute cyclic dependency check with start_nodes = {:?}",
        start_nodes
    );

    /* The stack is used to push the neighbors of a workload inside the dependency graph
    that needs to be visited next and to terminate the search. If a workload is not already visited,
    all neighbor workloads of that workload are pushed on the stack and next round a workload is popped
    from the stack and the procedure is repeated until a cycle is detected or all workloads are visited once.
    With pushing and popping to the stack the search is done in the depth inside the dependency graph.
    The stack simulates what the recursion stack represents in the recursive dfs algorithm. */
    let mut stack: VecDeque<&String> = VecDeque::new();

    // used to prevent visiting nodes repeatedly
    let mut visited: HashSet<&String> = HashSet::with_capacity(state.workloads.len());

    /* although the path container is used for lookups,
    measurements have shown that it is faster than associative data structure within this code path */
    let mut path: VecDeque<&String> = VecDeque::with_capacity(state.workloads.len());

    // start visiting workloads in the graph only for a subset of workloads (e.g. in case of a an update) or for all
    let mut workloads_to_visit: Vec<&String> = if let Some(nodes) = start_nodes {
        nodes
    } else {
        state.workloads.keys().collect()
    };
    /* sort the keys of the map to have an constant equal outcome
    because the current data structure is randomly ordered because of HashMap's random seed */
    workloads_to_visit.sort();

    // iterate through all the nodes if they are not already visited
    for workload_name in workloads_to_visit {
        if visited.contains(workload_name) {
            continue;
        }

        log::trace!("searching for workload = '{}'", workload_name);
        stack.push_front(workload_name);
        while let Some(head) = stack.front() {
            if let Some(workload_spec) = state.workloads.get(*head) {
                if !visited.contains(head) {
                    log::trace!("visit '{}'", head);
                    visited.insert(head);
                    path.push_back(head);
                } else {
                    log::trace!("remove '{}' from path", head);
                    path.pop_back();
                    stack.pop_front();
                }

                // sort the map to have an constant equal outcome
                let mut dependencies: Vec<&String> = workload_spec.dependencies.keys().collect();
                dependencies.sort();

                for dependency in dependencies {
                    if !visited.contains(dependency) {
                        stack.push_front(dependency);
                    } else if path.contains(&dependency) {
                        // [impl->swdd~cycle-detection-stops-on-the-first-cycle~1]
                        log::debug!("workload '{dependency}' is part of a cycle.");
                        return Some(dependency.to_string());
                    }
                }
            } else {
                // [impl->swdd~cycle-detection-ignores-non-existing-workloads~1]
                log::trace!(
                    "workload '{}' is skipped because it is not part of the state.",
                    head
                );
                stack.pop_front();
            }
        }
    }
    None
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
    use std::{collections::HashSet, ops::Deref};

    const AGENT_NAME: &str = "agent_A";
    const RUNTIME: &str = "runtime X";

    fn fn_assert_cycle(
        state_builder: StateBuilder,
        workloads: &[&str],
        expected_nodes_part_of_a_cycle: &[&str],
    ) {
        let mut actual = HashSet::new();
        for start_node in workloads {
            let builder = state_builder.clone();
            // marking `start_node` as first node to visit by adding prefix "1_" to the workload name
            let state = builder.set_start_node(start_node).build();

            let result = dfs(&state, None);

            // matches! not used, because of better assertion output if a test fails
            assert!(result.is_some(), "expected cycle, but no cycle detected");
            let workload_part_of_cycle = result.unwrap().replace("1_", ""); // remove prefix "1_"
            assert!(
                expected_nodes_part_of_a_cycle.contains(&workload_part_of_cycle.deref()),
                "{}",
                format!(
                    "expected workload '{}' is not part of a cycle",
                    workload_part_of_cycle
                )
            );

            actual.insert(workload_part_of_cycle);
        }

        assert_eq!(
            actual,
            HashSet::from_iter(
                expected_nodes_part_of_a_cycle
                    .iter()
                    .map(|item| item.to_string())
            ),
            "fewer workloads than expected are part of the cycle"
        );
    }

    fn fn_assert_no_cycle(state_builder: StateBuilder, workloads: &[&str]) {
        for start_node in workloads {
            let builder = state_builder.clone();
            // marking `start_node` as first node to visit by adding prefix "1_" to the workload name
            let state = builder.set_start_node(start_node).build();
            let result = dfs(&state, None);
            assert!(
                result.is_none(),
                "{}",
                format!(
                    "expected no cycle, but cycle detected, workload '{:?}' is part of cycle.",
                    result
                )
            );
        }
    }

    /// Asserts that a cycle is detected within the inter workload dependencies.
    /// To accomplish that the cycle is found regardless of a certain start node,
    /// the cycle check is repeated for each workload, so that each workload
    /// is one times the start node of the cycle search.
    ///
    /// This will invoke the [`panic!`] macro if the provided expression cannot be
    /// evaluated to `true` at runtime.
    ///
    /// # Description
    ///
    /// The State structure contains the dependency graph within its members.
    /// The dependency graph is represented as adjacency list based on HashMaps.
    /// Due to HashMap's seed, the order is not preserved between different runs and
    /// the cycle check method sorts the input to avoid different output.
    /// To mark each workload one times as a start node, we must prefix the workload name
    /// with "1_", so that it moves to the first position due to the sorting.
    ///
    /// Example:
    /// Workloads: A, B, C
    /// Make B as first start node: 1_B, A, C
    ///
    /// # Arguments
    ///
    /// * `state_builder` - The State builder for creating a State
    /// * `workloads` - The workloads array representing the vertices of the dependency graph
    /// * `expected_workloads_part_of_cycle` - The expected workloads that shall be part of a cycle
    ///
    #[macro_export]
    macro_rules! assert_cycle {
        ( $builder:expr, $workloads:expr, $expected:expr ) => {
            fn_assert_cycle($builder, $workloads, $expected)
        };
    }

    /// Asserts that no cycle is detected within the inter workload dependencies.
    /// To accomplish that no cycle is found regardless of a certain start node,
    /// the cycle check is repeated for each workload, so that each workload
    /// is one times the start node of the cycle search.
    ///
    /// This will invoke the [`panic!`] macro if the provided expression cannot be
    /// evaluated to `true` at runtime.
    ///
    /// # Description
    ///
    /// The State structure contains the dependency graph within its members.
    /// The dependency graph is represented as adjacency list based on HashMaps.
    /// Due to HashMap's seed, the order is not preserved between different runs and
    /// the cycle check method sorts the input to avoid different output.
    /// To mark each workload one times as a start node, we must prefix the workload name
    /// with "1_", so that it moves to the first position due to the sorting.
    ///
    /// Example:
    /// Workloads: A, B, C
    /// Make B as first start node: 1_B, A, C
    ///
    /// # Arguments
    ///
    /// * `state_builder` - The State builder for creating a State
    /// * `workloads` - The workloads array representing the vertices of the dependency graph
    ///
    #[macro_export]
    macro_rules! assert_no_cycle {
        ( $builder:expr, $workloads:expr ) => {
            fn_assert_no_cycle($builder, $workloads)
        };
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%20%20%20%20C%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondRunning)
            .workload_dependency("C", "D", AddCondition::AddCondRunning)
            .workload_dependency("C", "A", AddCondition::AddCondRunning);

        let expected_nodes_part_of_a_cycle = ["A", "B", "C"];

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20C%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20E%20-%3E%20D%3B%0A%20%20%20%20D%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_2() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondRunning)
            .workload_dependency("C", "F", AddCondition::AddCondRunning)
            .workload_dependency("F", "E", AddCondition::AddCondRunning)
            .workload_dependency("E", "D", AddCondition::AddCondRunning)
            .workload_dependency("D", "A", AddCondition::AddCondRunning);

        let expected_nodes_part_of_a_cycle = ["A", "B", "C", "D", "E", "F"];

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_3() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "A", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["A", "B"];

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_4() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "D", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["C", "H", "G", "F", "D"];

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    // Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20A%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_5() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "A", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["A", "B", "D", "C", "H", "G", "F"];

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20H%3B%0A%20%20%20%20A%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20A%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20C%20-%3E%20B%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20C%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20D%3B%0A%20%20%20%20D%20-%3E%20H%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%7D
    #[test]
    fn utest_detect_cycle_in_dependencies_star_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "H", AddCondition::AddCondRunning)
            .workload_dependency("A", "D", AddCondition::AddCondRunning)
            .workload_dependency("B", "A", AddCondition::AddCondRunning)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["G", "D", "H", "D"];

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20H%3B%0A%20%20%20%20A%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20A%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20C%20-%3E%20B%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20D%3B%0A%20%20%20%20E%20-%3E%20C%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20D%3B%0A%20%20%20%20H%20-%3E%20D%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_star_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "H", AddCondition::AddCondRunning)
            .workload_dependency("A", "D", AddCondition::AddCondRunning)
            .workload_dependency("B", "A", AddCondition::AddCondRunning)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("E", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded);

        assert_no_cycle!(builder, &workloads);
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20A%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20B%3B%0A%20%20%20%20G%20-%3E%20H%3B%0A%20%20%20%20H%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20F%3B%0A%7D
    #[test]
    fn utest_detect_self_cycle_in_dependencies_separated_graphs() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "A", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "F", AddCondition::AddCondSucceeded);

        let expected_nodes_part_of_a_cycle = ["F"]; // self cycle in one of the two graphs

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-stops-on-the-first-cycle~1]
    /// Graph visualized: 1) A -> A and 2) A -> B -> B
    #[test]
    fn utest_detect_self_cycle_in_dependencies() {
        let _ = env_logger::builder().is_test(true).try_init();

        // 1)
        let state = StateBuilder::default()
            .with_workloads(&["A"])
            .workload_dependency("A", "A", AddCondition::AddCondRunning)
            .build();

        let result = dfs(&state, None);
        assert_eq!(result, Some("A".to_string()));

        // 2)
        let workloads = ["A", "B"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "B", AddCondition::AddCondRunning);

        let expected_nodes_part_of_a_cycle = ["B"]; // self cycle in one of the two graphs

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-ignores-non-existing-workloads~1]
    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20E%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%20%20%20%20F%20-%3E%20C%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%7D
    /// The graph configuration below contains an additional edge to a dependency that is not part of the state config.
    #[test]
    fn utest_detect_continue_on_non_existing_workload_in_dependencies_and_find_cycles() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondRunning)
            .workload_dependency("B", "E", AddCondition::AddCondRunning)
            .workload_dependency("E", "F", AddCondition::AddCondRunning)
            .workload_dependency("F", "C", AddCondition::AddCondRunning)
            .workload_dependency("F", "D", AddCondition::AddCondRunning)
            .workload_dependency("F", "G", AddCondition::AddCondRunning) // G does not exist in the state
            .workload_dependency("C", "G", AddCondition::AddCondRunning)
            .workload_dependency("C", "D", AddCondition::AddCondRunning)
            .workload_dependency("C", "E", AddCondition::AddCondRunning);

        let expected_nodes_part_of_a_cycle = ["E", "F", "C"];

        assert_cycle!(builder, &workloads, &expected_nodes_part_of_a_cycle);
    }

    // [utest->swdd~cycle-detection-ignores-non-existing-workloads~1]
    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20E%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20D%3B%0A%20%20%20%20F%20-%3E%20C%3B%0A%20%20%20%20C%20-%3E%20D%3B%0A%7D
    /// The graph configuration below contains an additional edge to a dependency that is not part of the state config.
    #[test]
    fn utest_detect_continue_on_non_existing_workload_in_dependencies() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondRunning)
            .workload_dependency("B", "E", AddCondition::AddCondRunning)
            .workload_dependency("E", "F", AddCondition::AddCondRunning)
            .workload_dependency("F", "C", AddCondition::AddCondRunning)
            .workload_dependency("F", "D", AddCondition::AddCondRunning)
            .workload_dependency("F", "G", AddCondition::AddCondRunning) // G does not exist in the state
            .workload_dependency("C", "G", AddCondition::AddCondRunning) // G does not exist in the state
            .workload_dependency("C", "D", AddCondition::AddCondRunning);

        assert_no_cycle!(builder, &workloads);
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20D%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20F%3B%0A%20%20%20%20D%20-%3E%20G%3B%0A%20%20%20%20D%20-%3E%20H%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "D", AddCondition::AddCondRunning)
            .workload_dependency("B", "D", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "F", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "H", AddCondition::AddCondSucceeded);

        assert_no_cycle!(builder, &workloads);
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20B%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20E%3B%0A%20%20%20%20C%20-%3E%20H%3B%0A%20%20%20%20D%20-%3E%20B%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20E%3B%0A%20%20%20%20F%20-%3E%20E%3B%0A%20%20%20%20H%20-%3E%20G%3B%0A%20%20%20%20G%20-%3E%20F%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_2() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];

        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("B", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("C", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("F", "E", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "G", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "F", AddCondition::AddCondSucceeded);

        assert_no_cycle!(builder, &workloads);
    }

    /// Graph visualized: https://dreampuf.github.io/GraphvizOnline/#digraph%20%7B%0A%20%20%20%20A%20-%3E%20B%3B%0A%20%20%20%20B%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20A%3B%0A%20%20%20%20D%20-%3E%20C%3B%0A%20%20%20%20D%20-%3E%20B%3B%0A%20%20%20%20G%20-%3E%20H%3B%0A%20%20%20%20H%20-%3E%20F%3B%0A%20%20%20%20F%20-%3E%20F%3B%0A%7D
    #[test]
    fn utest_detect_no_cycle_in_dependencies_separated_graphs_1() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workloads = ["A", "B", "C", "D", "E", "F", "G", "H"];
        let builder = StateBuilder::default()
            .with_workloads(&workloads)
            .workload_dependency("A", "B", AddCondition::AddCondRunning)
            .workload_dependency("B", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "A", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "C", AddCondition::AddCondSucceeded)
            .workload_dependency("D", "B", AddCondition::AddCondSucceeded)
            .workload_dependency("G", "H", AddCondition::AddCondSucceeded)
            .workload_dependency("H", "F", AddCondition::AddCondSucceeded);

        assert_no_cycle!(builder, &workloads);
    }

    #[derive(Clone)]
    struct StateBuilder(State);
    impl StateBuilder {
        fn default() -> Self {
            let state = generate_test_complete_state(Vec::new()).desired_state;
            StateBuilder(state)
        }

        fn with_workloads(mut self, workloads: &[&str]) -> Self {
            for w in workloads {
                let mut test_workload_spec = generate_test_workload_spec_with_param(
                    AGENT_NAME.into(),
                    w.to_string(),
                    RUNTIME.into(),
                );
                test_workload_spec.dependencies.clear();
                self.0.workloads.insert(w.to_string(), test_workload_spec);
            }
            self
        }

        fn workload_dependency(
            mut self,
            workload: &str,
            depend_on: &str,
            add_condition: AddCondition,
        ) -> Self {
            self.0
                .workloads
                .get_mut(workload)
                .and_then(|w_spec| w_spec.dependencies.insert(depend_on.into(), add_condition));
            self
        }

        fn set_start_node(mut self, start_node: &str) -> Self {
            let new_name = format!("1_{start_node}");
            let entry = self.0.workloads.remove(start_node).unwrap();
            self.0.workloads.insert(new_name.clone(), entry);

            for workload_spec in self.0.workloads.values_mut() {
                if let Some(dep_condition) = workload_spec.dependencies.remove(start_node) {
                    workload_spec
                        .dependencies
                        .insert(new_name.clone(), dep_condition);
                }
            }
            self
        }

        fn build(self) -> State {
            self.0
        }
    }
}

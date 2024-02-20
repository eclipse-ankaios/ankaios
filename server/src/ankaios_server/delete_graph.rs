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
use common::objects::{AddCondition, DeleteCondition, DeletedWorkload, WorkloadSpec};
use std::collections::HashMap;

#[cfg(test)]
use mockall::automock;

#[derive(Default)]
pub struct DeleteGraph {
    delete_graph: HashMap<String, HashMap<String, DeleteCondition>>,
}

#[cfg_attr(test, automock)]
impl DeleteGraph {
    // [impl->swdd~server-state-stores-delete-condition~1]
    pub fn insert(&mut self, new_workloads: &[WorkloadSpec]) {
        for workload_spec in new_workloads {
            for (dependency_name, add_condition) in workload_spec.dependencies.iter() {
                /* currently for other add conditions besides AddCondRunning
                the workload can be deleted immediately and does not need a delete condition */
                if add_condition == &AddCondition::AddCondRunning {
                    let workload_name = workload_spec.instance_name.workload_name().to_owned();
                    self.delete_graph
                        .entry(dependency_name.clone())
                        .or_default()
                        .insert(workload_name, DeleteCondition::DelCondNotPendingNorRunning);
                }
            }
        }
    }

    // [impl->swdd~server-state-adds-delete-conditions-to-deleted-workload~1]
    pub fn apply_delete_conditions_to(&self, deleted_workloads: &mut [DeletedWorkload]) {
        for workload in deleted_workloads.iter_mut() {
            if let Some(delete_conditions) = self
                .delete_graph
                .get(workload.instance_name.workload_name())
            {
                workload.dependencies = delete_conditions.clone();
            }
        }
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
    use super::{AddCondition, DeleteCondition, DeleteGraph};
    use common::{objects::DeletedWorkload, test_utils::generate_test_workload_spec_with_param};
    use std::collections::HashMap;

    const AGENT_A: &str = "agent_A";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const WORKLOAD_NAME_4: &str = "workload_4";
    const WORKLOAD_NAME_5: &str = "workload_5";
    const WORKLOAD_NAME_6: &str = "workload_6";
    const RUNTIME: &str = "runtime";

    // [utest->swdd~server-state-stores-delete-condition~1]
    #[test]
    fn utest_delete_graph_insert() {
        /*
            Dependency graph as input           Expected delete graph

            R = ADD_COND_RUNNING
            S = ADD_COND_SUCCEEDED
            F = ADD_COND_FAILED

                                          =>    2 --> 1 (DelCondNotPendingNorRunning)
            4 --> 1 --> 2                       5 --> 3 (DelCondNotPendingNorRunning)
               F     R
            3 --> 5
               R
            6 (workload without dependencies)
        */
        let _ = env_logger::builder().is_test(true).try_init();

        let mut workload_1 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        let mut workload_2 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_2.to_string(),
            RUNTIME.to_string(),
        );

        let mut workload_3 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_3.to_string(),
            RUNTIME.to_string(),
        );

        let mut workload_4 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_4.to_string(),
            RUNTIME.to_string(),
        );

        let mut workload_5 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_5.to_string(),
            RUNTIME.to_string(),
        );

        let mut workload_6 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_6.to_string(),
            RUNTIME.to_string(),
        );

        workload_1.dependencies =
            HashMap::from([(workload_2.name.clone(), AddCondition::AddCondRunning)]);

        workload_2.dependencies.clear();

        workload_3.dependencies =
            HashMap::from([(workload_5.name.clone(), AddCondition::AddCondRunning)]);

        workload_4.dependencies =
            HashMap::from([(workload_1.name.clone(), AddCondition::AddCondFailed)]);

        workload_5.dependencies.clear();
        workload_6.dependencies.clear();

        let mut delete_graph = DeleteGraph::default();
        delete_graph.insert(&vec![
            workload_1.clone(),
            workload_2.clone(),
            workload_3.clone(),
            workload_4.clone(),
            workload_5.clone(),
            workload_6.clone(),
        ]);

        let expected_delete_graph = HashMap::from([
            (
                workload_2.name.clone(),
                HashMap::from([(
                    workload_1.name.clone(),
                    DeleteCondition::DelCondNotPendingNorRunning,
                )]),
            ),
            (
                workload_5.name.clone(),
                HashMap::from([(
                    workload_3.name.clone(),
                    DeleteCondition::DelCondNotPendingNorRunning,
                )]),
            ),
        ]);

        assert_eq!(expected_delete_graph, delete_graph.delete_graph);
    }

    // [utest->swdd~server-state-stores-delete-condition~1]
    // [utest->swdd~server-state-adds-delete-conditions-to-deleted-workload~1]
    #[test]
    fn utest_delete_graph_apply_delete_conditions() {
        /*
            2 --> 1 (DelCondNotPendingNorRunning)
            5 --> 3 (DelCondNotPendingNorRunning)

            Expectation:
            The DeletedWorkloads of workload 2 and 5 shall be filled with the
            content of the delete graph above,
            and the DeletedWorkload of workload 4 shall contain an empty
            DeleteDependencies map.
        */
        let _ = env_logger::builder().is_test(true).try_init();

        let delete_graph = DeleteGraph {
            delete_graph: HashMap::from([
                (
                    WORKLOAD_NAME_2.to_string(),
                    HashMap::from([(
                        WORKLOAD_NAME_1.to_string(),
                        DeleteCondition::DelCondNotPendingNorRunning,
                    )]),
                ),
                (
                    WORKLOAD_NAME_5.to_string(),
                    HashMap::from([(
                        WORKLOAD_NAME_3.to_string(),
                        DeleteCondition::DelCondNotPendingNorRunning,
                    )]),
                ),
            ]),
        };

        let mut deleted_workloads = vec![
            DeletedWorkload {
                name: WORKLOAD_NAME_2.to_string(),
                agent: AGENT_A.to_string(),
                ..Default::default()
            },
            DeletedWorkload {
                name: WORKLOAD_NAME_4.to_string(),
                agent: AGENT_A.to_string(),
                ..Default::default()
            },
            DeletedWorkload {
                name: WORKLOAD_NAME_5.to_string(),
                agent: AGENT_A.to_string(),
                ..Default::default()
            },
        ];

        delete_graph.apply_delete_conditions_to(&mut deleted_workloads);

        assert_eq!(
            DeletedWorkload {
                name: WORKLOAD_NAME_2.to_string(),
                agent: AGENT_A.to_string(),
                dependencies: HashMap::from([(
                    WORKLOAD_NAME_1.to_string(),
                    DeleteCondition::DelCondNotPendingNorRunning
                )])
            },
            deleted_workloads[0]
        );

        assert_eq!(
            DeletedWorkload {
                name: WORKLOAD_NAME_5.to_string(),
                agent: AGENT_A.to_string(),
                dependencies: HashMap::from([(
                    WORKLOAD_NAME_3.to_string(),
                    DeleteCondition::DelCondNotPendingNorRunning
                )])
            },
            deleted_workloads[2]
        );

        assert_eq!(
            DeletedWorkload {
                name: WORKLOAD_NAME_4.to_string(),
                agent: AGENT_A.to_string(),
                dependencies: HashMap::new()
            },
            deleted_workloads[1]
        );
    }
}

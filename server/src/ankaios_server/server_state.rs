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

use super::cycle_check;
use crate::state_manipulation::{Object, Path};
use crate::workload_state_db::WorkloadStateDB;
use common::std_extensions::IllegalStateResult;
use common::{
    commands::{CompleteState, RequestCompleteState},
    objects::{DeletedWorkload, State, WorkloadSpec},
};
use std::collections::HashMap;
use std::fmt::Display;

#[cfg(test)]
use mockall::automock;

fn update_state(
    current_state: &CompleteState,
    updated_state: CompleteState,
    update_mask: Vec<String>,
) -> Result<CompleteState, UpdateStateError> {
    // [impl->swdd~update-current-state-empty-update-mask~1]
    if update_mask.is_empty() {
        return Ok(updated_state);
    }

    // [impl->swdd~update-current-state-with-update-mask~1]
    let mut new_state: Object = current_state.try_into().map_err(|err| {
        UpdateStateError::ResultInvalid(format!("Failed to parse current state, '{}'", err))
    })?;
    let state_from_update: Object = updated_state.try_into().map_err(|err| {
        UpdateStateError::ResultInvalid(format!("Failed to parse new state, '{}'", err))
    })?;

    for field in update_mask {
        let field: Path = field.into();
        if let Some(field_from_update) = state_from_update.get(&field) {
            if new_state.set(&field, field_from_update.to_owned()).is_err() {
                return Err(UpdateStateError::FieldNotFound(field.into()));
            }
        } else if new_state.remove(&field).is_err() {
            return Err(UpdateStateError::FieldNotFound(field.into()));
        }
    }

    if let Ok(new_state) = new_state.try_into() {
        Ok(new_state)
    } else {
        Err(UpdateStateError::ResultInvalid(
            "Could not parse into CompleteState.".to_string(),
        ))
    }
}

fn extract_added_and_deleted_workloads(
    current_state: &State,
    new_state: &State,
) -> Option<(Vec<WorkloadSpec>, Vec<DeletedWorkload>)> {
    let mut added_workloads: Vec<WorkloadSpec> = Vec::new();
    let mut deleted_workloads: Vec<DeletedWorkload> = Vec::new();

    // find updated or deleted workloads
    current_state.workloads.iter().for_each(|(wl_name, wls)| {
        if let Some(new_wls) = new_state.workloads.get(wl_name) {
            // The new workload is identical with existing or updated. Lets check if it is an update.
            if wls != new_wls {
                // [impl->swdd~server-detects-changed-workload~1]
                added_workloads.push(new_wls.clone());
                deleted_workloads.push(DeletedWorkload {
                    agent: wls.agent.clone(),
                    name: wl_name.clone(),
                    dependencies: HashMap::new(),
                });
            }
        } else {
            // [impl->swdd~server-detects-deleted-workload~1]
            deleted_workloads.push(DeletedWorkload {
                agent: wls.agent.clone(),
                name: wl_name.clone(),
                dependencies: HashMap::new(),
            });
        }
    });

    // find new workloads
    // [impl->swdd~server-detects-new-workload~1]
    new_state
        .workloads
        .iter()
        .for_each(|(new_wl_name, new_wls)| {
            if !current_state.workloads.contains_key(new_wl_name) {
                added_workloads.push(new_wls.clone());
            }
        });

    if added_workloads.is_empty() && deleted_workloads.is_empty() {
        return None;
    }

    Some((added_workloads, deleted_workloads))
}

fn update_delete_graph(delete_graph: &mut DeleteGraph, state: &State) {
    for workload in state.workloads.values() {
        for (dependency_name, add_condition) in workload.dependencies.iter() {
            /* for other add conditions besides AddCondRunning
            the workload can be deleted immediately and does not need a delete condition */
            if add_condition == &AddCondition::AddCondRunning {
                let workload_name = workload.name.clone();
                delete_graph
                    .entry(dependency_name.clone())
                    .and_modify(|e| {
                        e.insert(
                            workload_name.clone(),
                            DeleteCondition::DelCondNotPendingNorRunning,
                        );
                    })
                    .or_insert_with(|| {
                        HashMap::from([(
                            workload_name,
                            DeleteCondition::DelCondNotPendingNorRunning,
                        )])
                    });
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStateError {
    FieldNotFound(String),
    ResultInvalid(String),
    CycleInDependencies(String),
}

impl Display for UpdateStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateStateError::FieldNotFound(field) => {
                write!(f, "Could not find field {}", field)
            }
            UpdateStateError::ResultInvalid(reason) => {
                write!(f, "Resulting State is invalid, reason: '{}'", reason)
            }
            UpdateStateError::CycleInDependencies(workload_part_of_cycle) => {
                write!(
                    f,
                    "workload dependency '{}' is part of a cycle.",
                    workload_part_of_cycle
                )
            }
        }
    }
}

use common::objects::{AddCondition, DeleteCondition};
pub type DeleteGraph = HashMap<String, HashMap<String, DeleteCondition>>;

#[derive(Default)]
pub struct ServerState {
    state: CompleteState,
    delete_graph: DeleteGraph,
}

pub type AddedDeletedWorkloads = Option<(Vec<WorkloadSpec>, Vec<DeletedWorkload>)>;

#[cfg_attr(test, automock)]
impl ServerState {
    pub fn get_complete_state_by_field_mask(
        &self,
        request_complete_state: &RequestCompleteState,
        workload_state_db: &WorkloadStateDB,
    ) -> Result<CompleteState, String> {
        let current_complete_state = CompleteState {
            request_id: request_complete_state.request_id.to_owned(),
            current_state: self.state.current_state.clone(),
            startup_state: self.state.startup_state.clone(),
            workload_states: workload_state_db.get_all_workload_states(),
        };

        // [impl->swdd~server-filters-get-complete-state-result~1]
        if !request_complete_state.field_mask.is_empty() {
            let current_complete_state: Object =
                current_complete_state.try_into().unwrap_or_illegal_state();
            let mut return_state = Object::default();

            return_state.set(
                &"requestId".into(),
                request_complete_state.request_id.to_owned().into(),
            )?;

            for field in &request_complete_state.field_mask {
                if let Some(value) = current_complete_state.get(&field.into()) {
                    return_state.set(&field.into(), value.to_owned())?;
                } else {
                    log::debug!(
                        concat!(
                        "Result for CompleteState incomplete, as requested field does not exist:\n",
                        "   request_id: {:?}\n",
                        "   field: {}"),
                        request_complete_state.request_id,
                        field
                    );
                    continue;
                };
            }

            return_state.try_into().map_err(|err: serde_yaml::Error| {
                format!("The result for CompleteState is invalid: '{}'", err)
            })
        } else {
            Ok(current_complete_state)
        }
    }

    pub fn get_workloads_for_agent(&self, agent_name: &String) -> Vec<WorkloadSpec> {
        self.state
            .current_state
            .workloads
            .clone()
            .into_values()
            // [impl->swdd~agent-from-agent-field~1]
            .filter(|workload_spec| workload_spec.agent.eq(agent_name))
            .collect()
    }

    pub fn update(
        &mut self,
        new_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<AddedDeletedWorkloads, UpdateStateError> {
        match update_state(&self.state, new_state, update_mask) {
            Ok(new_state) => {
                let cmd = extract_added_and_deleted_workloads(
                    &self.state.current_state,
                    &new_state.current_state,
                );

                if let Some((added_workloads, deleted_workloads)) = cmd {
                    let start_nodes: Vec<&String> = added_workloads
                        .iter()
                        .filter_map(|w| {
                            if !w.dependencies.is_empty() {
                                Some(&w.name)
                            } else {
                                None
                            }
                        })
                        .collect();

                    // [impl->swdd~server-state-rejects-state-with-cyclic-dependencies~1]
                    log::debug!(
                        "Execute cyclic dependency check with start_nodes = {:?}",
                        start_nodes
                    );
                    if let Some(workload_part_of_cycle) =
                        cycle_check::dfs(&new_state.current_state, Some(start_nodes))
                    {
                        return Err(UpdateStateError::CycleInDependencies(
                            workload_part_of_cycle,
                        ));
                    }

                    self::update_delete_graph(&mut self.delete_graph, &new_state.current_state);

                    self.state = new_state;
                    Ok(Some((added_workloads, deleted_workloads)))
                } else {
                    Ok(None)
                }
            }
            Err(error) => Err(error),
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
    use std::collections::HashMap;

    use common::{
        commands::{CompleteState, RequestCompleteState},
        objects::{AddCondition, DeleteCondition, DeletedWorkload, State, WorkloadSpec},
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };

    use crate::{
        ankaios_server::server_state::UpdateStateError, workload_state_db::WorkloadStateDB,
    };

    use super::ServerState;
    const AGENT_A: &str = "agent_A";
    const AGENT_B: &str = "agent_B";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const RUNTIME: &str = "runtime";

    #[test]
    fn utest_server_state_get_complete_state_by_field_mask_empty_mask() {
        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        let w2 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_2.to_string(),
            RUNTIME.to_string(),
        );

        let w3 = generate_test_workload_spec_with_param(
            AGENT_B.to_string(),
            WORKLOAD_NAME_3.to_string(),
            RUNTIME.to_string(),
        );

        let server_state = ServerState {
            state: generate_test_complete_state(
                "".to_string(),
                vec![w1.clone(), w2.clone(), w3.clone()],
            ),
            ..Default::default()
        };

        let request_id = "cli@request_id".to_string();
        let request_complete_state = RequestCompleteState {
            request_id: request_id.clone(),
            field_mask: vec![],
        };

        let mut workload_state_db = WorkloadStateDB::default();
        workload_state_db.insert(server_state.state.workload_states.clone());

        let mut complete_state = server_state
            .get_complete_state_by_field_mask(&request_complete_state, &workload_state_db)
            .unwrap();

        // result must be sorted because inside WorkloadStateDB the order of workload states is not preserved
        complete_state
            .workload_states
            .sort_by(|left, right| left.workload_name.cmp(&right.workload_name));

        let mut expected_complete_state = server_state.state.clone();
        expected_complete_state.request_id = request_id;
        expected_complete_state
            .workload_states
            .sort_by(|left, right| left.workload_name.cmp(&right.workload_name));
        assert_eq!(expected_complete_state, complete_state);
    }

    #[test]
    fn utest_server_state_get_complete_state_by_field_mask() {
        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        let w2 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_2.to_string(),
            RUNTIME.to_string(),
        );

        let w3 = generate_test_workload_spec_with_param(
            AGENT_B.to_string(),
            WORKLOAD_NAME_3.to_string(),
            RUNTIME.to_string(),
        );

        let server_state = ServerState {
            state: generate_test_complete_state(
                "".to_string(),
                vec![w1.clone(), w2.clone(), w3.clone()],
            ),
            ..Default::default()
        };

        let request_id = "cli@request_id".to_string();
        let request_complete_state = RequestCompleteState {
            request_id: request_id.clone(),
            field_mask: vec![
                format!("currentState.workloads.{}", WORKLOAD_NAME_1),
                format!("currentState.workloads.{}.agent", WORKLOAD_NAME_3),
            ],
        };

        let mut workload_state_db = WorkloadStateDB::default();
        workload_state_db.insert(server_state.state.workload_states.clone());

        let mut complete_state = server_state
            .get_complete_state_by_field_mask(&request_complete_state, &workload_state_db)
            .unwrap();

        // result must be sorted because inside WorkloadStateDB the order of workload states is not preserved
        complete_state
            .workload_states
            .sort_by(|left, right| left.workload_name.cmp(&right.workload_name));

        let mut expected_complete_state = server_state.state.clone();
        expected_complete_state.current_state.workloads = HashMap::from([
            (w1.name.clone(), w1.clone()),
            (
                w3.name.clone(),
                WorkloadSpec {
                    agent: AGENT_B.to_string(),
                    ..Default::default()
                },
            ),
        ]);
        expected_complete_state.request_id = request_id;
        expected_complete_state.workload_states.clear();
        assert_eq!(expected_complete_state, complete_state);
    }

    #[test]
    fn utest_server_state_get_complete_state_by_field_mask_continue_on_invalid_mask() {
        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        let server_state = ServerState {
            state: generate_test_complete_state("".to_string(), vec![w1.clone()]),
            ..Default::default()
        };

        let request_id = "cli@request_id".to_string();
        let request_complete_state = RequestCompleteState {
            request_id: request_id.clone(),
            field_mask: vec![
                "workloads.invalidMask".to_string(), // invalid not existing workload
                format!("currentState.workloads.{}", WORKLOAD_NAME_1),
            ],
        };

        let mut workload_state_db = WorkloadStateDB::default();
        workload_state_db.insert(server_state.state.workload_states.clone());

        let mut complete_state = server_state
            .get_complete_state_by_field_mask(&request_complete_state, &workload_state_db)
            .unwrap();

        // result must be sorted because inside WorkloadStateDB the order of workload states is not preserved
        complete_state
            .workload_states
            .sort_by(|left, right| left.workload_name.cmp(&right.workload_name));

        let mut expected_complete_state = server_state.state.clone();
        expected_complete_state.current_state.workloads =
            HashMap::from([(w1.name.clone(), w1.clone())]);
        expected_complete_state.request_id = request_id;
        expected_complete_state.workload_states.clear();
        assert_eq!(expected_complete_state, complete_state);
    }

    // [utest->swdd~agent-from-agent-field~1]
    #[test]
    fn utest_server_state_get_workloads_per_agent() {
        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        let w2 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_2.to_string(),
            RUNTIME.to_string(),
        );

        let w3 = generate_test_workload_spec_with_param(
            AGENT_B.to_string(),
            WORKLOAD_NAME_3.to_string(),
            RUNTIME.to_string(),
        );

        let server_state = ServerState {
            state: generate_test_complete_state(
                "".to_string(),
                vec![w1.clone(), w2.clone(), w3.clone()],
            ),
            ..Default::default()
        };

        let mut workloads = server_state.get_workloads_for_agent(&AGENT_A.to_string());
        workloads.sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(workloads, vec![w1, w2]);

        let workloads = server_state.get_workloads_for_agent(&AGENT_B.to_string());
        assert_eq!(workloads, vec![w3]);

        let workloads = server_state.get_workloads_for_agent(&"unknown_agent".to_string());
        assert_eq!(workloads.len(), 0);
    }

    // [utest->swdd~server-state-rejects-state-with-cyclic-dependencies~1]
    #[test]
    fn utest_server_state_update_state_reject_state_with_cyclic_dependencies() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        // workload has a self cycle to workload A
        let new_workload_1 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            "workload A".to_string(),
            RUNTIME.to_string(),
        );

        let mut new_workload_2 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );
        new_workload_2.dependencies.clear();

        let old_state = CompleteState {
            current_state: State {
                workloads: HashMap::from([(workload.name.clone(), workload)]),
                ..Default::default()
            },
            ..Default::default()
        };

        let rejected_new_state = CompleteState {
            current_state: State {
                workloads: HashMap::from([
                    (new_workload_1.name.clone(), new_workload_1),
                    (new_workload_2.name.clone(), new_workload_2),
                ]),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };

        let result = server_state.update(rejected_new_state.clone(), vec![]);
        assert_eq!(
            result,
            Err(UpdateStateError::CycleInDependencies(
                "workload A".to_string()
            ))
        );

        // server state shall be the old state, new state shall be rejected
        assert_eq!(old_state, server_state.state);
    }

    // [utest->swdd~update-current-state-empty-update-mask~1]
    #[test]
    fn utest_replace_all_if_update_mask_empty() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_state.clone(), vec![]).unwrap();

        assert_eq!(update_state, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_replace_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["currentState.workloads.workload_1".into()];

        let mut expected = old_state.clone();
        expected.current_state.workloads.insert(
            "workload_1".into(),
            update_state
                .current_state
                .workloads
                .get("workload_1")
                .unwrap()
                .clone(),
        );

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_add_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["currentState.workloads.workload_4".into()];

        let mut expected = old_state.clone();
        expected.current_state.workloads.insert(
            "workload_4".into(),
            update_state
                .current_state
                .workloads
                .get("workload_4")
                .unwrap()
                .clone(),
        );

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_remove_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["currentState.workloads.workload_2".into()];

        let mut expected = old_state.clone();
        expected.current_state.workloads.remove("workload_2");

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_remove_non_existing_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["currentState.workloads.workload_5".into()];

        let expected = &old_state;

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(*expected, server_state.state);
    }

    #[test]
    fn utest_remove_fails_from_non_map() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["currentState.workloads.workload_2.tags.x".into()];

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let result = server_state.update(update_state, update_mask);

        assert!(result.is_err());
        assert_eq!(server_state.state, old_state);
    }

    #[test]
    fn utest_fails_with_update_mask_empty_string() {
        let _ = env_logger::builder().is_test(true).try_init();
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["".into()];

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let result = server_state.update(update_state, update_mask);
        assert!(result.is_err());
        assert_eq!(server_state.state, old_state);
    }

    #[test]
    fn utest_extract_added_and_deleted_workloads_no_update() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut server_state = ServerState::default();

        let update_cmd = server_state
            .update(CompleteState::default(), vec![])
            .unwrap();
        assert!(update_cmd.is_none());
        assert_eq!(server_state.state, CompleteState::default());
    }

    // [utest->swdd~server-detects-new-workload~1]
    #[test]
    fn utest_extract_added_and_deleted_workloads_new_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let new_state = generate_test_update_state();
        let update_mask = vec![];

        let mut server_state = ServerState::default();

        let added_deleted_workloads = server_state.update(new_state.clone(), update_mask).unwrap();

        let expected_added_workloads: Vec<WorkloadSpec> = new_state
            .clone()
            .current_state
            .workloads
            .into_values()
            .collect();

        let expected_deleted_workloads: Vec<DeletedWorkload> = Vec::new();
        assert!(added_deleted_workloads.is_some());
        let (added_workloads, deleted_workloads) = added_deleted_workloads.unwrap();
        assert_eq!(added_workloads, expected_added_workloads);
        assert_eq!(deleted_workloads, expected_deleted_workloads);
        assert_eq!(server_state.state, new_state);
    }

    // [utest->swdd~server-detects-deleted-workload~1]
    #[test]
    fn utest_extract_added_and_deleted_workloads_deleted_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_complete_state = generate_test_old_state();
        let update_state = CompleteState::default();
        let update_mask = vec![];

        let mut server_state = ServerState {
            state: current_complete_state.clone(),
            ..Default::default()
        };

        let added_deleted_workloads = server_state.update(update_state, update_mask).unwrap();

        let expected_added_workloads: Vec<WorkloadSpec> = Vec::new();
        let expected_deleted_workloads: Vec<DeletedWorkload> = current_complete_state
            .current_state
            .workloads
            .iter()
            .map(|(k, v)| DeletedWorkload {
                agent: v.agent.clone(),
                name: k.clone(),
                dependencies: HashMap::new(),
            })
            .collect();

        assert!(added_deleted_workloads.is_some());
        let (added_workloads, deleted_workloads) = added_deleted_workloads.unwrap();
        assert_eq!(added_workloads, expected_added_workloads);
        assert_eq!(deleted_workloads, expected_deleted_workloads);
        assert_eq!(server_state.state, CompleteState::default());
    }

    // [utest->swdd~server-detects-changed-workload~1]
    #[test]
    fn utest_extract_added_and_deleted_workloads_updated_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_complete_state = generate_test_old_state();
        let mut new_state = current_complete_state.current_state.clone();

        let wl_name_to_update = "workload_1";
        let wls_to_update = current_complete_state
            .current_state
            .workloads
            .get(wl_name_to_update)
            .unwrap();
        let wls_update = generate_test_workload_spec_with_param(
            "agent_B".into(),
            "workload_4".into(),
            "runtime_2".into(),
        );
        new_state
            .workloads
            .insert(wl_name_to_update.to_string(), wls_update.clone());

        let new_complete_state = CompleteState {
            current_state: new_state.clone(),
            ..Default::default()
        };
        let update_mask = vec![];

        let mut server_state = ServerState {
            state: current_complete_state.clone(),
            ..Default::default()
        };

        let added_deleted_workloads = server_state
            .update(new_complete_state.clone(), update_mask)
            .unwrap();

        let expected_added_workloads = vec![wls_update];
        let expected_deleted_workloads = vec![DeletedWorkload {
            agent: wls_to_update.agent.clone(),
            name: wl_name_to_update.to_string(),
            dependencies: HashMap::new(),
        }];
        assert!(added_deleted_workloads.is_some());
        let (added_workloads, deleted_workloads) = added_deleted_workloads.unwrap();
        assert_eq!(added_workloads, expected_added_workloads);
        assert_eq!(deleted_workloads, expected_deleted_workloads);
        assert_eq!(server_state.state, new_complete_state);
    }

    #[test]
    fn utest_server_state_update_state_update_delete_graph() {
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
            "workload_4".to_string(),
            RUNTIME.to_string(),
        );

        workload_1.dependencies =
            HashMap::from([(workload_2.name.clone(), AddCondition::AddCondRunning)]);

        workload_2.dependencies =
            HashMap::from([(workload_3.name.clone(), AddCondition::AddCondSucceeded)]);

        workload_3.dependencies =
            HashMap::from([(workload_4.name.clone(), AddCondition::AddCondFailed)]);

        workload_4.dependencies.clear();

        let new_state = CompleteState {
            current_state: State {
                workloads: HashMap::from([
                    (workload_1.name.clone(), workload_1.clone()),
                    (workload_2.name.clone(), workload_2.clone()),
                    (workload_3.name.clone(), workload_3.clone()),
                    (workload_4.name.clone(), workload_4.clone()),
                ]),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut server_state = ServerState::default();

        let result = server_state.update(new_state.clone(), vec![]);
        assert!(result.unwrap().is_some());

        let expected_delete_graph = HashMap::from([(
            workload_2.name.clone(),
            HashMap::from([(
                workload_1.name.clone(),
                DeleteCondition::DelCondNotPendingNorRunning,
            )]),
        )]);
        assert_eq!(expected_delete_graph, server_state.delete_graph);
        assert_eq!(new_state, server_state.state);
        log::info!("{:?}", server_state.delete_graph);
    }

    fn generate_test_old_state() -> CompleteState {
        generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                generate_test_workload_spec_with_param(
                    "agent_A".into(),
                    "workload_1".into(),
                    "runtime_1".into(),
                ),
                generate_test_workload_spec_with_param(
                    "agent_A".into(),
                    "workload_2".into(),
                    "runtime_2".into(),
                ),
                generate_test_workload_spec_with_param(
                    "agent_B".into(),
                    "workload_3".into(),
                    "runtime_1".into(),
                ),
            ],
        )
    }

    fn generate_test_update_state() -> CompleteState {
        generate_test_complete_state(
            "request_id".to_owned(),
            vec![
                generate_test_workload_spec_with_param(
                    "agent_B".into(),
                    "workload_1".into(),
                    "runtime_2".into(),
                ),
                generate_test_workload_spec_with_param(
                    "agent_B".into(),
                    "workload_3".into(),
                    "runtime_2".into(),
                ),
                generate_test_workload_spec_with_param(
                    "agent_A".into(),
                    "workload_4".into(),
                    "runtime_1".into(),
                ),
            ],
        )
    }
}

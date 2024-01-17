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

// #[cfg(not(test))]
// use super::update_state::update_state;
// use common::objects::WorkloadSpec;
// #[cfg(test)]
// use tests::update_state_mock as update_state;

use super::cyclic_check;
use crate::state_manipulation::{Object, Path};
use crate::workload_state_db::WorkloadStateDB;
use common::std_extensions::IllegalStateResult;
use common::{
    commands::{CompleteState, RequestCompleteState, UpdateStateRequest, UpdateWorkload},
    execution_interface::ExecutionCommand,
    objects::{DeleteCondition, DeletedWorkload, State, WorkloadSpec},
};
use std::collections::HashMap;
use std::fmt::Display;

#[cfg(test)]
use mockall::automock;

fn update_state(
    current_state: &CompleteState,
    update: UpdateStateRequest,
) -> Result<CompleteState, UpdateStateError> {
    // [impl->swdd~update-current-state-empty-update-mask~1]
    if update.update_mask.is_empty() {
        return Ok(update.state);
    }

    // [impl->swdd~update-current-state-with-update-mask~1]
    let mut new_state: Object = current_state.try_into().map_err(|err| {
        UpdateStateError::ResultInvalid(format!("Failed to parse current state, '{}'", err))
    })?;
    let state_from_update: Object = update.state.try_into().map_err(|err| {
        UpdateStateError::ResultInvalid(format!("Failed to parse new state, '{}'", err))
    })?;

    for field in update.update_mask {
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

fn prepare_update_workload(
    current_state: &State,
    new_state: &State,
) -> Option<common::execution_interface::ExecutionCommand> {
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

    log::info!(
        "The update has {} new or updated workloads, {} workloads to delete",
        added_workloads.len(),
        deleted_workloads.len()
    );

    Some(ExecutionCommand::UpdateWorkload(UpdateWorkload {
        added_workloads,
        deleted_workloads,
    }))
}

#[derive(Debug)]
pub enum UpdateStateError {
    FieldNotFound(String),
    ResultInvalid(String),
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
        }
    }
}

pub type DeleteGraph = HashMap<String, HashMap<String, DeleteCondition>>;

#[derive(Default)]
pub struct ServerState {
    state: CompleteState,
    delete_conditions: DeleteGraph,
}

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
        update_request: UpdateStateRequest,
    ) -> Result<Option<ExecutionCommand>, String> {
        match update_state(&self.state, update_request) {
            Ok(new_state) => {
                let cmd =
                    prepare_update_workload(&self.state.current_state, &new_state.current_state);

                if let Some(cmd) = cmd {
                    let ExecutionCommand::UpdateWorkload(UpdateWorkload {
                        added_workloads,
                        deleted_workloads: _,
                    }) = &cmd
                    else {
                        std::unreachable!("Expected ExecutionCommand::UpdateWorkload");
                    };

                    log::debug!("update_state => added_workloads = {:?}", added_workloads);
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
                    log::debug!(
                        "Execute cyclic dependency check with start_nodes = {:?}",
                        start_nodes
                    );
                    let result = cyclic_check::dfs(
                        &new_state.current_state,
                        cyclic_check::StartNodes::Subset(start_nodes),
                    );

                    log::debug!("cyclic dependency check result = {:?}", result);
                    self.state = new_state;
                    Ok(Some(cmd))
                } else {
                    Ok(None)
                }
            }
            Err(error) => Err(format!("Could not execute UpdateRequest: '{}'", error)),
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
        commands::{CompleteState, RequestCompleteState, UpdateStateRequest, UpdateWorkload},
        objects::{DeletedWorkload, WorkloadSpec},
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };

    use crate::workload_state_db::WorkloadStateDB;

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
                "workoads.invalidMask".to_string(), // invalid not existing workload
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

    // [utest->swdd~update-current-state-empty-update-mask~1]
    #[test]
    fn utest_replace_all_if_update_mask_empty() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec![],
        };

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_request.clone()).unwrap();

        let expected = update_request.state.clone();
        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_replace_workload() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["currentState.workloads.workload_1".into()],
        };

        let mut expected = old_state.clone();
        expected.current_state.workloads.insert(
            "workload_1".into(),
            update_request
                .state
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
        server_state.update(update_request).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_add_workload() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["currentState.workloads.workload_4".into()],
        };

        let mut expected = old_state.clone();
        expected.current_state.workloads.insert(
            "workload_4".into(),
            update_request
                .state
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
        server_state.update(update_request).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_remove_workload() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["currentState.workloads.workload_2".into()],
        };

        let mut expected = old_state.clone();
        expected.current_state.workloads.remove("workload_2");

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_request).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-current-state-with-update-mask~1]
    #[test]
    fn utest_remove_non_existing_workload() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["currentState.workloads.workload_5".into()],
        };

        let expected = &old_state;

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        server_state.update(update_request).unwrap();

        assert_eq!(*expected, server_state.state);
    }

    #[test]
    fn utest_remove_fails_from_non_map() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["currentState.workloads.workload_2.tags.x".into()],
        };

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let result = server_state.update(update_request);

        assert!(result.is_err());
        assert_eq!(server_state.state, old_state);
    }

    #[test]
    fn utest_fails_with_update_mask_empty_string() {
        let _ = env_logger::builder().is_test(true).try_init();
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["".into()],
        };

        let mut server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let result = server_state.update(update_request);
        assert!(result.is_err());
        assert_eq!(server_state.state, old_state);
    }

    #[test]
    fn utest_prepare_update_workload_no_update() {
        let _ = env_logger::builder().is_test(true).try_init();

        let update_request = UpdateStateRequest {
            state: CompleteState::default(),
            update_mask: vec![],
        };

        let mut server_state = ServerState::default();

        let update_cmd = server_state.update(update_request).unwrap();
        assert!(update_cmd.is_none());
        assert_eq!(server_state.state, CompleteState::default());
    }

    // [utest->swdd~server-detects-new-workload~1]
    #[test]
    fn utest_prepare_update_workload_new_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let new_state = generate_test_update_state();

        let update_request = UpdateStateRequest {
            state: new_state.clone(),
            update_mask: vec![],
        };

        let mut server_state = ServerState::default();

        let update_cmd = server_state.update(update_request).unwrap();

        let expected_cmd =
            common::execution_interface::ExecutionCommand::UpdateWorkload(UpdateWorkload {
                added_workloads: new_state
                    .clone()
                    .current_state
                    .workloads
                    .into_values()
                    .collect(),
                deleted_workloads: Vec::new(),
            });
        assert!(update_cmd.is_some());
        assert_eq!(update_cmd.unwrap(), expected_cmd);
        assert_eq!(server_state.state, new_state);
    }

    // [utest->swdd~server-detects-deleted-workload~1]
    #[test]
    fn utest_prepare_update_workload_deleted_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_complete_state = generate_test_old_state();

        let update_request = UpdateStateRequest {
            state: CompleteState::default(),
            update_mask: vec![],
        };

        let mut server_state = ServerState {
            state: current_complete_state.clone(),
            ..Default::default()
        };

        let update_cmd = server_state.update(update_request).unwrap();

        let expected_cmd =
            common::execution_interface::ExecutionCommand::UpdateWorkload(UpdateWorkload {
                added_workloads: Vec::new(),
                deleted_workloads: current_complete_state
                    .current_state
                    .workloads
                    .iter()
                    .map(|(k, v)| DeletedWorkload {
                        agent: v.agent.clone(),
                        name: k.clone(),
                        dependencies: HashMap::new(),
                    })
                    .collect(),
            });
        assert!(update_cmd.is_some());
        assert_eq!(update_cmd.unwrap(), expected_cmd);
        assert_eq!(server_state.state, CompleteState::default());
    }

    // [utest->swdd~server-detects-changed-workload~1]
    #[test]
    fn utest_prepare_update_workload_updated_workloads() {
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
        let update_request = UpdateStateRequest {
            state: new_complete_state.clone(),
            update_mask: vec![],
        };

        let mut server_state = ServerState {
            state: current_complete_state.clone(),
            ..Default::default()
        };

        let update_cmd = server_state.update(update_request).unwrap();

        let expected_cmd =
            common::execution_interface::ExecutionCommand::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![wls_update],
                deleted_workloads: vec![DeletedWorkload {
                    agent: wls_to_update.agent.clone(),
                    name: wl_name_to_update.to_string(),
                    dependencies: HashMap::new(),
                }],
            });
        assert!(update_cmd.is_some());
        assert_eq!(update_cmd.unwrap(), expected_cmd);
        assert_eq!(server_state.state, new_complete_state);
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

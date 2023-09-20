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

use std::fmt::Display;

use crate::state_manipulation::{Object, Path};
use common::{commands::CompleteState, commands::UpdateStateRequest, commands::UpdateWorkload};
use common::{
    execution_interface::ExecutionCommand,
    objects::{DeletedWorkload, State, WorkloadSpec},
};

pub fn update_state(
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

pub fn prepare_update_workload(
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
                    dependencies: wls.dependencies.clone(),
                });
            }
        } else {
            // [impl->swdd~server-detects-deleted-workload~1]
            deleted_workloads.push(DeletedWorkload {
                agent: wls.agent.clone(),
                name: wl_name.clone(),
                dependencies: wls.dependencies.clone(),
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
        commands::{CompleteState, UpdateStateRequest, UpdateWorkload},
        objects::{DeletedWorkload, State},
        test_utils::{generate_test_complete_state, generate_test_workload_spec_with_param},
    };

    use super::prepare_update_workload;

    // [utest->swdd~update-current-state-empty-update-mask~1]
    #[test]
    fn utest_replace_all_if_update_mask_empty() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec![],
        };

        let expected = update_request.state.clone();

        let actual = super::update_state(&old_state, update_request).unwrap();

        assert_eq!(expected, actual);
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

        let actual = super::update_state(&old_state, update_request).unwrap();

        assert_eq!(expected, actual);
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

        let actual = super::update_state(&old_state, update_request).unwrap();

        assert_eq!(expected, actual);
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

        let actual = super::update_state(&old_state, update_request).unwrap();

        assert_eq!(expected, actual);
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

        let actual = super::update_state(&old_state, update_request).unwrap();

        assert_eq!(*expected, actual);
    }

    #[test]
    fn utest_remove_fails_from_non_map() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["currentState.workloads.workload_2.tags.x".into()],
        };

        let actual = super::update_state(&old_state, update_request);

        assert!(actual.is_err());
    }

    #[test]
    fn utest_fails_with_update_mask_empty_string() {
        let old_state = generate_test_old_state();
        let update_request = UpdateStateRequest {
            state: generate_test_update_state(),
            update_mask: vec!["".into()],
        };

        let actual = super::update_state(&old_state, update_request);

        assert!(actual.is_err());
    }

    #[test]
    fn utest_prepare_update_workload_no_update() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_state = State {
            workloads: HashMap::new(),
            configs: HashMap::new(),
            cron_jobs: HashMap::new(),
        };
        let new_state = &current_state;

        let update_cmd = prepare_update_workload(&current_state, new_state);
        assert!(update_cmd.is_none());
    }

    // [utest->swdd~server-detects-new-workload~1]
    #[test]
    fn utest_prepare_update_workload_new_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_state = State {
            workloads: HashMap::new(),
            configs: HashMap::new(),
            cron_jobs: HashMap::new(),
        };
        let new_state = generate_test_update_state();

        let update_cmd = prepare_update_workload(&current_state, &new_state.current_state);

        let expected_cmd =
            common::execution_interface::ExecutionCommand::UpdateWorkload(UpdateWorkload {
                added_workloads: new_state.current_state.workloads.into_values().collect(),
                deleted_workloads: Vec::new(),
            });
        assert!(update_cmd.is_some());
        assert_eq!(update_cmd.unwrap(), expected_cmd);
    }

    // [utest->swdd~server-detects-deleted-workload~1]
    #[test]
    fn utest_prepare_update_workload_deleted_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_complete_state = generate_test_old_state();
        let new_state = State {
            workloads: HashMap::new(),
            configs: HashMap::new(),
            cron_jobs: HashMap::new(),
        };

        let update_cmd = prepare_update_workload(&current_complete_state.current_state, &new_state);

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
                        dependencies: v.dependencies.clone(),
                    })
                    .collect(),
            });
        assert!(update_cmd.is_some());
        assert_eq!(update_cmd.unwrap(), expected_cmd);
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

        let update_cmd = prepare_update_workload(&current_complete_state.current_state, &new_state);

        let expected_cmd =
            common::execution_interface::ExecutionCommand::UpdateWorkload(UpdateWorkload {
                added_workloads: vec![wls_update],
                deleted_workloads: vec![DeletedWorkload {
                    agent: wls_to_update.agent.clone(),
                    name: wl_name_to_update.to_string(),
                    dependencies: wls_to_update.dependencies.clone(),
                }],
            });
        assert!(update_cmd.is_some());
        assert_eq!(update_cmd.unwrap(), expected_cmd);
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

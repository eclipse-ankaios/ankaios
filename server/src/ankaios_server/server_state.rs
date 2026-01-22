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
use super::rendered_workloads::RenderedWorkloads;

use ankaios_api::ALLOWED_CHAR_SET;
use ankaios_api::ank_base::{
    AgentMapSpec, CompleteState, CompleteStateRequest, CompleteStateSpec, DeletedWorkload,
    StateSpec, WorkloadInstanceName, WorkloadNamed, WorkloadStateSpec, WorkloadStatesMapSpec,
};
use common::state_manipulation::{Object, Path};
use common::std_extensions::IllegalStateResult;

use std::fmt::Display;

#[cfg_attr(test, mockall_double::double)]
use super::config_renderer::ConfigRenderer;
#[cfg_attr(test, mockall_double::double)]
use super::delete_graph::DeleteGraph;

#[cfg(test)]
use mockall::automock;

#[derive(Debug, Default)]
pub struct StateGenerationResult {
    pub old_desired_state: StateSpec,
    pub new_desired_state: StateSpec,
    pub new_agent_map: AgentMapSpec,
}

fn extract_added_and_deleted_workloads(
    current_workloads: &RenderedWorkloads,
    new_workloads: &RenderedWorkloads,
) -> Option<AddedDeletedWorkloads> {
    let mut added_workloads: Vec<WorkloadNamed> = Vec::new();
    let mut deleted_workloads: Vec<DeletedWorkload> = Vec::new();

    // find updated or deleted workloads
    current_workloads.iter().for_each(|(wl_name, wls)| {
        if let Some(new_wls) = new_workloads.get(wl_name) {
            // The new workload is identical with existing or updated. Lets check if it is an update.
            if wls != new_wls {
                // [impl->swdd~server-detects-changed-workload~1]
                added_workloads.push(new_wls.clone());
                deleted_workloads.push(DeletedWorkload {
                    instance_name: wls.instance_name.clone(),
                    ..Default::default()
                });
            }
        } else {
            // [impl->swdd~server-detects-deleted-workload~1]
            deleted_workloads.push(DeletedWorkload {
                instance_name: wls.instance_name.clone(),
                ..Default::default()
            });
        }
    });

    // find new workloads
    // [impl->swdd~server-detects-new-workload~1]
    new_workloads.iter().for_each(|(new_wl_name, new_wls)| {
        if !current_workloads.contains_key(new_wl_name) {
            added_workloads.push(new_wls.clone());
        }
    });

    if added_workloads.is_empty() && deleted_workloads.is_empty() {
        None
    } else {
        Some(AddedDeletedWorkloads {
            added_workloads,
            deleted_workloads,
        })
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
                write!(f, "Could not find field {field}")
            }
            UpdateStateError::ResultInvalid(reason) => {
                write!(f, "Resulting State is invalid, reason: '{reason}'")
            }
            UpdateStateError::CycleInDependencies(workload_part_of_cycle) => {
                write!(
                    f,
                    "workload dependency '{workload_part_of_cycle}' is part of a cycle."
                )
            }
        }
    }
}

#[derive(Default)]
pub struct ServerState {
    state: CompleteStateSpec,
    rendered_workloads: RenderedWorkloads,
    delete_graph: DeleteGraph,
    config_renderer: ConfigRenderer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddedDeletedWorkloads {
    pub added_workloads: Vec<WorkloadNamed>,
    pub deleted_workloads: Vec<DeletedWorkload>,
}

// [impl->swdd~server-filters-get-complete-state-workload-state-substate~1]
fn include_both_state_and_substate_filters(filters: &mut Vec<String>) {
    let state_suffix = ".state";
    let substate_suffix = ".subState";
    let state_regex = format!(r"^workloadStates(\.{ALLOWED_CHAR_SET}+){{3}}");

    let execution_state_regex =
        regex::Regex::new(&format!(r"{state_regex}{}$", regex::escape(state_suffix)))
            .unwrap_or_illegal_state();
    let execution_substate_regex = regex::Regex::new(&format!(
        r"{state_regex}{}$",
        regex::escape(substate_suffix)
    ))
    .unwrap_or_illegal_state();

    fn replace_suffix(s: &str, old_suffix: &str, new_suffix: &str) -> String {
        s[..s.len() - old_suffix.len()].to_string() + new_suffix
    }

    let current_length = filters.len();

    for i in 0..current_length {
        if execution_state_regex.is_match(&filters[i]) {
            let substate_filter = replace_suffix(&filters[i], state_suffix, substate_suffix);
            if !filters.contains(&substate_filter) {
                filters.push(substate_filter);
            }
        } else if execution_substate_regex.is_match(&filters[i]) {
            let state_filter = replace_suffix(&filters[i], substate_suffix, state_suffix);
            if !filters.contains(&state_filter) {
                filters.push(state_filter);
            }
        }
    }
}

#[cfg_attr(test, automock)]
impl ServerState {
    const API_VERSION_FILTER_MASK: &'static str = "desiredState.apiVersion";
    const DESIRED_STATE_FIELD_MASK_PART: &'static str = "desiredState";

    // [impl->swdd~server-provides-interface-get-complete-state~2]
    // [impl->swdd~server-filters-get-complete-state-result~2]
    pub fn get_complete_state_by_field_mask(
        &self,
        request_complete_state: CompleteStateRequest,
        workload_states_map: &WorkloadStatesMapSpec,
        agent_map: &AgentMapSpec,
    ) -> Result<CompleteState, String> {
        let current_complete_state: CompleteState = CompleteStateSpec {
            desired_state: self.state.desired_state.clone(),
            workload_states: workload_states_map.clone(),
            agents: agent_map.clone(),
        }
        .into();

        if !request_complete_state.field_mask.is_empty() {
            let mut filters = request_complete_state.field_mask;
            if filters
                .iter()
                .any(|filter| filter.starts_with(Self::DESIRED_STATE_FIELD_MASK_PART))
            {
                filters.push(Self::API_VERSION_FILTER_MASK.to_owned());
            }

            // [impl->swdd~server-filters-get-complete-state-workload-state-substate~1]
            include_both_state_and_substate_filters(&mut filters);

            let current_complete_state: Object =
                current_complete_state.try_into().unwrap_or_illegal_state();
            let mut return_state = Object::default();

            let filters = filters.into_iter().map(|f| f.into()).collect::<Vec<Path>>();
            //[impl->swdd~server-filters-get-complete-state-result-with-wildcards~1]
            let filters = current_complete_state.expand_wildcards(&filters);

            log::debug!("Current state: {current_complete_state:?}");
            for field in &filters {
                if let Some(value) = current_complete_state.get(field) {
                    return_state.set(field, value.to_owned())?;
                } else {
                    log::debug!(
                        concat!(
                            "Result for CompleteState incomplete, as requested field does not exist:\n",
                            "   field: {}"
                        ),
                        field
                    );
                    continue;
                };
            }

            return_state.try_into().map_err(|err: serde_yaml::Error| {
                format!("The result for CompleteState is invalid: '{err}'")
            })
        } else {
            Ok(current_complete_state)
        }
    }

    // [impl->swdd~agent-from-agent-field~1]
    pub fn get_workloads_for_agent(&self, agent_name: &str) -> Vec<WorkloadNamed> {
        self.rendered_workloads
            .iter()
            .filter(|(_, workload)| workload.instance_name.agent_name().eq(agent_name))
            .map(|(_, workload)| workload.clone())
            .collect()
    }

    // [impl->swdd~server-handles-logs-request-message~1]
    pub fn desired_state_contains_instance_name(
        &self,
        instance_name: &WorkloadInstanceName,
    ) -> bool {
        self.rendered_workloads
            .get(&instance_name.workload_name)
            .is_some_and(|workload| {
                workload.instance_name.agent_name == instance_name.agent_name
                    && workload.instance_name.id == instance_name.id
            })
    }

    pub fn update(
        &mut self,
        new_desired_state: StateSpec,
    ) -> Result<Option<AddedDeletedWorkloads>, UpdateStateError> {
        // [impl->swdd~update-desired-state-with-update-mask~1]
        // [impl->swdd~update-desired-state-empty-update-mask~1]
        // [impl->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
        // println!("call 1: {:#?}", new_desired_state.workloads.workloads);
        // println!("call 2: {:#?}", self.state.desired_state.configs.configs);

        let new_rendered_workloads = self
            .config_renderer
            .render_workloads(
                &new_desired_state.workloads.workloads,
                &new_desired_state.configs.configs,
            )
            .map_err(|err| UpdateStateError::ResultInvalid(err.to_string()))?;

        // [impl->swdd~server-state-triggers-validation-of-workload-fields~1]
        new_rendered_workloads
            .validate()
            .map_err(UpdateStateError::ResultInvalid)?;

        // [impl->swdd~server-state-compares-rendered-workloads~1]
        let added_deleted_workloads =
            extract_added_and_deleted_workloads(&self.rendered_workloads, &new_rendered_workloads);

        if let Some(mut added_deleted_workloads) = added_deleted_workloads {
            let added_workloads = &added_deleted_workloads.added_workloads;

            let start_nodes: Vec<&str> = added_workloads
                .iter()
                .filter_map(|w| {
                    if !w.workload.dependencies.dependencies.is_empty() {
                        Some(w.instance_name.workload_name())
                    } else {
                        None
                    }
                })
                .collect();

            // [impl->swdd~server-state-rejects-state-with-cyclic-dependencies~1]
            if let Some(workload_part_of_cycle) =
                cycle_check::dfs(&new_desired_state, Some(start_nodes))
            {
                return Err(UpdateStateError::CycleInDependencies(
                    workload_part_of_cycle,
                ));
            }

            // [impl->swdd~server-state-stores-delete-condition~1]
            self.delete_graph.insert(added_workloads);

            // [impl->swdd~server-state-adds-delete-conditions-to-deleted-workload~1]
            self.delete_graph
                .apply_delete_conditions_to(&mut added_deleted_workloads.deleted_workloads);

            self.set_desired_state(new_desired_state);
            self.rendered_workloads = new_rendered_workloads;
            Ok(Some(added_deleted_workloads))
        } else {
            // update state with changed fields not affecting workloads, e.g. config items
            // [impl->swdd~server-state-updates-state-on-unmodified-workloads~1]
            self.set_desired_state(new_desired_state);
            Ok(None)
        }
    }

    pub fn generate_new_state(
        &self,
        updated_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<StateGenerationResult, UpdateStateError> {
        // [impl->swdd~update-desired-state-empty-update-mask~1]
        if update_mask.is_empty() {
            let new_complete_state: CompleteStateSpec =
                updated_state.try_into().map_err(|err| {
                    UpdateStateError::ResultInvalid(format!(
                        "Could not parse into CompleteState: '{err}'"
                    ))
                })?;
            return Ok(StateGenerationResult {
                old_desired_state: self.state.desired_state.clone(),
                new_desired_state: new_complete_state.desired_state,
                new_agent_map: new_complete_state.agents,
            });
        }

        // [impl->swdd~update-desired-state-with-update-mask~1]
        let old_state: Object = (&self.state).try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Failed to parse current state, '{err}'"))
        })?;
        let state_from_update: Object = updated_state.try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Failed to parse new state, '{err}'"))
        })?;

        let mut new_state = old_state.clone();

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

        let new_complete_state: CompleteStateSpec =
            new_state.clone().try_into().map_err(|err| {
                UpdateStateError::ResultInvalid(format!(
                    "Could not parse into CompleteState: '{err}'"
                ))
            })?;

        Ok(StateGenerationResult {
            old_desired_state: self.state.desired_state.clone(),
            new_desired_state: new_complete_state.desired_state,
            new_agent_map: new_complete_state.agents,
        })
    }

    // [impl->swdd~server-cleans-up-state~1]
    pub fn cleanup_state(&mut self, new_workload_states: &[WorkloadStateSpec]) {
        // [impl->swdd~server-removes-obsolete-delete-graph-entires~1]
        self.delete_graph
            .remove_deleted_workloads_from_delete_graph(new_workload_states);
    }

    fn set_desired_state(&mut self, new_desired_state: StateSpec) {
        self.state.desired_state = new_desired_state;
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
    use super::ServerState;
    use std::collections::HashMap;

    use crate::ankaios_server::{
        config_renderer::{ConfigRenderError, MockConfigRenderer},
        delete_graph::MockDeleteGraph,
        rendered_workloads::RenderedWorkloads,
        server_state::{
            AddedDeletedWorkloads, UpdateStateError, extract_added_and_deleted_workloads,
        },
    };
    use ankaios_api::ank_base::{
        AgentMapSpec, CompleteState, CompleteStateRequest,
        CompleteStateSpec, DeletedWorkload, State, StateSpec, Workload, WorkloadInstanceName,
        WorkloadInstanceNameSpec, WorkloadMap, WorkloadMapSpec, WorkloadNamed, WorkloadStateSpec,
        WorkloadStatesMapSpec,
    };
    use ankaios_api::test_utils::{
        fixtures, generate_test_complete_state, generate_test_config_map,
        generate_test_proto_complete_state, generate_test_state_from_workloads,
        generate_test_workload, generate_test_workload_named,
        generate_test_workload_named_with_params, generate_test_workload_named_with_runtime_config,
        generate_test_workload_with_params,
    };

    fn generate_rendered_workloads_from_state(state: &StateSpec) -> RenderedWorkloads {
        RenderedWorkloads(
            state
                .workloads
                .workloads
                .iter()
                .map(|(name, wl)| {
                    (
                        name.to_owned(),
                        WorkloadNamed::from((name.to_owned(), wl.to_owned())),
                    )
                })
                .collect(),
        )
    }

    fn from_map_to_vec(value: WorkloadStatesMapSpec) -> Vec<WorkloadStateSpec> {
        value
            .agent_state_map
            .into_iter()
            .flat_map(|(agent_name, name_state_map)| {
                name_state_map.wl_name_state_map.into_iter().flat_map(
                    move |(wl_name, id_state_map)| {
                        let agent_name = agent_name.clone();
                        id_state_map
                            .id_state_map
                            .into_iter()
                            .map(move |(wl_id, exec_state)| WorkloadStateSpec {
                                instance_name: WorkloadInstanceNameSpec::new(
                                    agent_name.clone(),
                                    wl_name.clone(),
                                    wl_id,
                                ),
                                execution_state: exec_state,
                            })
                    },
                )
            })
            .collect()
    }

    // [utest->swdd~server-filters-get-complete-state-workload-state-substate~1]
    #[test]
    fn test_include_both_state_and_substate_filters() {
        let mut filters = vec![
            "workloadStates.agent_A.workload_A.1234.state".to_string(),
            "workloadStates.agent_A.workload_B.5678.subState".to_string(),
            "workloadStates.agent_A.state_workload_A.1234.state".to_string(),
            "desiredState.workloads.workload_C".to_string(),
            "workloadStates.agent_A.workload_D.12345678.state".to_string(),
            "workloadStates.agent_A.workload_D.12345678.subState".to_string(),
        ];

        super::include_both_state_and_substate_filters(&mut filters);

        let expected_filters = vec![
            "workloadStates.agent_A.workload_A.1234.state".to_string(),
            "workloadStates.agent_A.workload_B.5678.subState".to_string(),
            "workloadStates.agent_A.state_workload_A.1234.state".to_string(),
            "desiredState.workloads.workload_C".to_string(),
            "workloadStates.agent_A.workload_D.12345678.state".to_string(),
            "workloadStates.agent_A.workload_D.12345678.subState".to_string(),
            "workloadStates.agent_A.workload_A.1234.subState".to_string(),
            "workloadStates.agent_A.workload_B.5678.state".to_string(),
            "workloadStates.agent_A.state_workload_A.1234.subState".to_string(),
        ];

        assert_eq!(filters, expected_filters);
    }

    // [utest->swdd~server-filters-get-complete-state-workload-state-substate~1]
    #[test]
    fn test_get_complete_state_by_field_mask_workload_state_with_substate_only() {
        let w1 = generate_test_workload_named();

        let server_state = ServerState {
            state: generate_test_complete_state(vec![w1.clone()]),
            ..Default::default()
        };

        let request_complete_state = CompleteStateRequest {
            field_mask: vec![format!(
                "workloadStates.{}.{}.{}.subState",
                fixtures::AGENT_NAMES[0],
                fixtures::WORKLOAD_NAMES[0],
                fixtures::WORKLOAD_IDS[0]
            )],
            subscribe_for_events: false,
        };

        let mut workload_state_map = WorkloadStatesMapSpec::default();
        workload_state_map
            .process_new_states(from_map_to_vec(server_state.state.workload_states.clone()));

        let received_complete_state = server_state
            .get_complete_state_by_field_mask(
                request_complete_state,
                &workload_state_map,
                &AgentMapSpec::default(),
            )
            .unwrap();

        let mut expected_workload_states = ankaios_api::ank_base::WorkloadStatesMap::default();
        expected_workload_states
            .agent_state_map
            .entry(fixtures::AGENT_NAMES[0].to_owned())
            .or_default()
            .wl_name_state_map
            .entry(fixtures::WORKLOAD_NAMES[0].to_owned())
            .or_default()
            .id_state_map
            .entry(fixtures::WORKLOAD_IDS[0].to_owned())
            .or_insert(ankaios_api::ank_base::ExecutionState {
                additional_info: None,
                execution_state_enum: Some(ankaios_api::ank_base::ExecutionStateEnum::Running(
                    ankaios_api::ank_base::Running::Ok as i32,
                )),
            });

        let expected_complete_state = CompleteState {
            desired_state: None,
            workload_states: Some(expected_workload_states),
            agents: None,
        };

        assert_eq!(received_complete_state, expected_complete_state);
    }

    // [utest->swdd~server-provides-interface-get-complete-state~2]
    // [utest->swdd~server-filters-get-complete-state-result~2]
    #[test]
    fn utest_server_state_get_complete_state_by_field_mask_empty_mask() {
        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w2 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w3 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let mut complete_state =
            generate_test_complete_state(vec![w1.clone(), w2.clone(), w3.clone()]);
        // the server state only cares about the desired state, workload states and agents are stored separately
        let workload_states_map = std::mem::take(&mut complete_state.workload_states);
        let agent_map = std::mem::take(&mut complete_state.agents);

        let server_state = ServerState {
            state: complete_state,
            ..Default::default()
        };

        let request_complete_state = CompleteStateRequest {
            field_mask: vec![],
            subscribe_for_events: false,
        };

        let received_complete_state = server_state
            .get_complete_state_by_field_mask(
                request_complete_state,
                &workload_states_map,
                &agent_map,
            )
            .unwrap();

        let mut expected_complete_state = server_state.state.clone();
        expected_complete_state.workload_states = workload_states_map;
        expected_complete_state.agents = agent_map;

        let expected_complete_state = CompleteState::from(expected_complete_state);
        assert_eq!(received_complete_state, expected_complete_state);
    }

    // [utest->swdd~server-provides-interface-get-complete-state~2]
    // [utest->swdd~server-filters-get-complete-state-result~2]
    #[test]
    fn utest_server_state_get_complete_state_by_field_mask_continue_on_invalid_mask() {
        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let mut server_state = ServerState {
            state: generate_test_complete_state(vec![w1]),
            ..Default::default()
        };

        server_state.state.workload_states = WorkloadStatesMapSpec::default();
        server_state.state.agents = AgentMapSpec::default();

        let request_complete_state = CompleteStateRequest {
            field_mask: vec![
                "workloads.invalidMask".to_string(), // invalid not existing workload
                format!("desiredState.workloads.{}", fixtures::WORKLOAD_NAMES[0]), // valid existing workload
            ],
            subscribe_for_events: false,
        };

        let received_complete_state = server_state
            .get_complete_state_by_field_mask(
                request_complete_state,
                &WorkloadStatesMapSpec::default(),
                &AgentMapSpec::default(),
            )
            .unwrap();

        let mut expected_complete_state = CompleteState {
            desired_state: Some(server_state.state.desired_state.clone().into()),
            workload_states: None,
            agents: None,
        };
        if let Some(expected_desired_state) = &mut expected_complete_state.desired_state {
            expected_desired_state.configs = None;
        }

        assert_eq!(received_complete_state, expected_complete_state);
    }

    // [utest->swdd~server-provides-interface-get-complete-state~2]
    // [utest->swdd~server-filters-get-complete-state-result~2]
    // [utest->swdd~server-filters-get-complete-state-result-with-wildcards~1]
    #[test]
    fn utest_server_state_get_complete_state_by_field_mask() {
        let mut w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w2 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w3 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        w1.workload.configs.configs.clear();

        let server_state = ServerState {
            state: generate_test_complete_state(vec![w1.clone(), w2.clone(), w3.clone()]),
            ..Default::default()
        };

        let request_complete_state = CompleteStateRequest {
            field_mask: vec![
                format!("desiredState.workloads.{}", fixtures::WORKLOAD_NAMES[0]),
                format!(
                    "desiredState.workloads.{}.agent",
                    fixtures::WORKLOAD_NAMES[2]
                ),
                format!("desiredState.workloads.*.runtime"),
            ],
            subscribe_for_events: false,
        };

        let mut workload_state_map = WorkloadStatesMapSpec::default();
        workload_state_map
            .process_new_states(from_map_to_vec(server_state.state.workload_states.clone()));

        let complete_state = server_state
            .get_complete_state_by_field_mask(
                request_complete_state,
                &workload_state_map,
                &AgentMapSpec::default(),
            )
            .unwrap();

        let expected_workloads = [
            (
                w3.instance_name.workload_name(),
                Workload {
                    agent: Some(w3.instance_name.agent_name().to_string()),
                    restart_policy: None,
                    dependencies: None,
                    tags: None,
                    runtime: Some(w3.workload.runtime.clone()),
                    runtime_config: None,
                    control_interface_access: None,
                    configs: None,
                    files: Some(Default::default()),
                },
            ),
            (
                w2.instance_name.workload_name(),
                Workload {
                    agent: None,
                    restart_policy: None,
                    dependencies: None,
                    tags: None,
                    runtime: Some(w2.workload.runtime.clone()),
                    runtime_config: None,
                    control_interface_access: None,
                    configs: None,
                    files: Some(Default::default()),
                },
            ),
            (
                w1.instance_name.workload_name(),
                generate_test_workload().into(),
            ),
        ];
        let mut expected_complete_state = generate_test_proto_complete_state(&expected_workloads);
        if let Some(expected_desired_state) = &mut expected_complete_state.desired_state {
            expected_desired_state.configs = None;
        }

        assert_eq!(expected_complete_state, complete_state);
    }

    // [utest->swdd~agent-from-agent-field~1]
    #[test]
    fn utest_server_state_get_workloads_per_agent() {
        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w2 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let w3 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
        );

        let old_complete_state =
            generate_test_complete_state(vec![w1.clone(), w2.clone(), w3.clone()]);

        let server_state = ServerState {
            rendered_workloads: generate_rendered_workloads_from_state(
                &old_complete_state.desired_state,
            ),
            state: old_complete_state,
            ..Default::default()
        };

        let mut workloads = server_state.get_workloads_for_agent(fixtures::AGENT_NAMES[0]);
        workloads.sort_by(|left, right| {
            left.instance_name
                .workload_name()
                .cmp(right.instance_name.workload_name())
        });
        assert_eq!(workloads, vec![w1, w2]);

        let workloads = server_state.get_workloads_for_agent(fixtures::AGENT_NAMES[1]);
        assert_eq!(workloads, vec![w3]);

        let workloads = server_state.get_workloads_for_agent("unknown_agent");
        assert_eq!(workloads.len(), 0);
    }

    // [utest->swdd~server-handles-logs-request-message~1]
    #[test]
    fn utest_desired_state_contains_instance_name() {
        let workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let mut other_workload_instance_name: WorkloadInstanceName =
            workload.instance_name.clone().into();
        other_workload_instance_name.workload_name = fixtures::WORKLOAD_NAMES[1].to_string();

        let complete_state = generate_test_complete_state(vec![workload.clone()]);

        let server_state = ServerState {
            rendered_workloads: generate_rendered_workloads_from_state(
                &complete_state.desired_state,
            ),
            state: complete_state,
            ..Default::default()
        };

        let instance_name = workload.instance_name.into();
        assert!(server_state.desired_state_contains_instance_name(&instance_name));

        assert!(!server_state.desired_state_contains_instance_name(&other_workload_instance_name));
    }

    // [utest->swdd~server-state-rejects-state-with-cyclic-dependencies~1]
    #[test]
    fn utest_server_state_update_state_reject_state_with_cyclic_dependencies() {
        let _ = env_logger::builder().is_test(true).try_init();

        let workload = generate_test_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        // workload has a self cycle to workload_B
        let new_workload_1 = generate_test_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        )
        .into();

        let mut new_workload_2: Workload = generate_test_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        )
        .into();
        new_workload_2.dependencies = None;

        let old_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(fixtures::WORKLOAD_NAMES[0].to_string(), workload)]),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let rejected_new_state = CompleteState {
            desired_state: Some(State {
                workloads: Some(WorkloadMap {
                    workloads: HashMap::from([
                        (fixtures::WORKLOAD_NAMES[1].to_string(), new_workload_1),
                        (fixtures::WORKLOAD_NAMES[0].to_string(), new_workload_2),
                    ]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().never();
        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .never();

        let mut mock_config_renderer = MockConfigRenderer::new();
        let desired_state_spec: StateSpec = rejected_new_state
            .desired_state
            .clone()
            .unwrap()
            .try_into()
            .unwrap();
        let clones_desired_state = desired_state_spec.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Ok(generate_rendered_workloads_from_state(
                    &clones_desired_state,
                ))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let result = server_state.update(desired_state_spec);
        assert_eq!(
            result,
            Err(UpdateStateError::CycleInDependencies(
                fixtures::WORKLOAD_NAMES[1].to_string()
            ))
        );

        // server state shall be the old state, new state shall be rejected
        assert_eq!(old_state, server_state.state);
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_replace_all_if_update_mask_empty() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let old_state = generate_test_old_state();
        let update_state: CompleteState = generate_test_update_state().into();
        let update_mask = vec![];

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let state_generation_result = server_state
            .generate_new_state(update_state.clone(), update_mask)
            .unwrap();

        let expected_desired_state: StateSpec =
            update_state.desired_state.unwrap().try_into().unwrap();
        assert_eq!(
            expected_desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_replace_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[0]
        )];

        let new_workload = update_state
            .desired_state
            .workloads
            .workloads
            .get(fixtures::WORKLOAD_NAMES[0])
            .unwrap()
            .clone();

        let mut expected = old_state.clone();
        expected
            .desired_state
            .workloads
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[0].to_owned(), new_workload.clone());

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };

        let state_generation_result = server_state
            .generate_new_state(update_state.into(), update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_add_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[3]
        )];

        let new_workload = update_state
            .desired_state
            .workloads
            .workloads
            .get(fixtures::WORKLOAD_NAMES[3])
            .unwrap()
            .clone();

        let mut expected = old_state.clone();
        expected
            .desired_state
            .workloads
            .workloads
            .insert(fixtures::WORKLOAD_NAMES[3].into(), new_workload.clone());

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };

        let state_generation_result = server_state
            .generate_new_state(update_state.into(), update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_update_configs() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let old_state = generate_test_old_state();
        let mut state_with_updated_config = old_state.clone();
        state_with_updated_config.desired_state.configs = generate_test_config_map();

        let update_mask = vec!["desiredState".to_string()];

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };

        let expected = state_with_updated_config.clone();

        let state_generation_result = server_state
            .generate_new_state(state_with_updated_config.into(), update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_remove_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[1]
        )];

        let mut expected = old_state.clone();
        expected
            .desired_state
            .workloads
            .workloads
            .remove(fixtures::WORKLOAD_NAMES[1])
            .unwrap();

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };

        let state_generation_result = server_state
            .generate_new_state(update_state.into(), update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_update_state_remove_non_existing_workload() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["desiredState.workloads.non_existing".into()];

        let expected = &old_state;

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };

        let state_generation_result = server_state
            .generate_new_state(update_state.into(), update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_update_state_remove_fails_from_non_map() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let field_mask = "desiredState.workloads.non.existing";
        let update_mask = vec![field_mask.into()];

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let result = server_state.generate_new_state(update_state.into(), update_mask);
        assert!(result.is_err());
        assert_eq!(
            UpdateStateError::FieldNotFound(field_mask.into()),
            result.unwrap_err()
        );
        assert_eq!(server_state.state, old_state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_update_state_fails_with_update_mask_empty_string() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["".into()];

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let result = server_state.generate_new_state(update_state.into(), update_mask);
        assert!(result.is_err());
        assert_eq!(
            UpdateStateError::FieldNotFound("".into()),
            result.unwrap_err()
        );
        assert_eq!(server_state.state, old_state);
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_no_changes_on_equal_states() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let server_state = ServerState::default(); // empty old state

        let state_generation_result = server_state
            .generate_new_state(CompleteState::default(), vec![])
            .unwrap();
        assert_eq!(
            state_generation_result.new_desired_state,
            StateSpec::default()
        );
        assert_eq!(server_state.state, CompleteStateSpec::default());
    }

    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    // [utest->swdd~server-state-updates-state-on-unmodified-workloads~1]
    #[test]
    fn utest_server_state_update_state_no_update() {
        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().never();
        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .never();

        let mut mock_config_renderer = MockConfigRenderer::new();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(|_, _| Ok(RenderedWorkloads::new()));

        let mut server_state = ServerState {
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
            ..Default::default()
        };

        let added_deleted_workloads = server_state.update(StateSpec::default()).unwrap();
        assert!(added_deleted_workloads.is_none());
        assert_eq!(server_state.state, CompleteStateSpec::default());
    }

    // [utest->swdd~server-state-updates-state-on-unmodified-workloads~1]
    #[test]
    fn utest_server_state_update_state_on_unmodified_workloads() {
        let mut server_state = ServerState::default();
        server_state
            .config_renderer
            .expect_render_workloads()
            .once()
            .returning(|_, _| Ok(RenderedWorkloads::default()));

        server_state.delete_graph.expect_insert().never();

        server_state
            .delete_graph
            .expect_apply_delete_conditions_to()
            .never();

        let new_state_with_configs = CompleteStateSpec {
            desired_state: StateSpec {
                configs: generate_test_config_map(),
                ..Default::default()
            },
            ..Default::default()
        };

        let added_deleted_workloads = server_state
            .update(new_state_with_configs.desired_state.clone())
            .unwrap();
        assert!(added_deleted_workloads.is_none());
        assert_eq!(server_state.state, new_state_with_configs);
    }

    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_workload_references_removed_configs() {
        let mut old_state = generate_test_old_state();
        old_state.desired_state.configs = generate_test_config_map();

        let mut updated_state = old_state.clone();
        updated_state.desired_state.configs.configs.clear();

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().never();

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .never();

        let mut mock_config_renderer = MockConfigRenderer::new();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Err(ConfigRenderError::Field(
                    "agent".to_string(),
                    "config item does not exist".to_string(),
                ))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let result = server_state.update(updated_state.desired_state);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("config item does not exist")
        );

        assert_eq!(old_state, server_state.state); // keep old state
    }

    // [utest->swdd~server-detects-changed-workload~1]
    // [utest->swdd~server-state-compares-rendered-workloads~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_detects_changed_workloads() {
        let templated_runtime_config = "{{templated_runtime_config}}".to_owned();
        let templated_workload = generate_test_workload_named_with_runtime_config(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
            templated_runtime_config,
        );

        let unchanged_workload = generate_test_workload_named();

        let old_state = CompleteStateSpec {
            desired_state: generate_test_state_from_workloads(vec![
                templated_workload.clone(),
                unchanged_workload.clone(),
            ]),
            ..Default::default()
        };

        let mut rendered_workload = templated_workload.clone();
        rendered_workload.workload.runtime_config = fixtures::RUNTIME_CONFIGS[0].to_owned();

        // old and new state are identical but the workload has been changed after rendering
        let new_state = old_state.clone();

        let mut server_state = ServerState {
            state: old_state,
            rendered_workloads: RenderedWorkloads::from([
                (
                    fixtures::WORKLOAD_NAMES[0].to_owned(),
                    rendered_workload.clone(),
                ),
                (
                    fixtures::WORKLOAD_NAMES[1].to_owned(),
                    unchanged_workload.clone(),
                ),
            ]),
            ..Default::default()
        };

        rendered_workload.workload.runtime_config =
            "updated_runtime_config_after_rendering".to_owned();
        let new_rendered_workloads = RenderedWorkloads::from([
            (
                fixtures::WORKLOAD_NAMES[0].to_owned(),
                rendered_workload.clone(),
            ),
            (
                fixtures::WORKLOAD_NAMES[1].to_owned(),
                unchanged_workload.clone(),
            ),
        ]);
        server_state
            .config_renderer
            .expect_render_workloads()
            .once()
            .return_once(|_, _| Ok(new_rendered_workloads));

        server_state
            .delete_graph
            .expect_insert()
            .once()
            .return_const(());

        server_state
            .delete_graph
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let added_deleted_workloads = server_state.update(new_state.desired_state).unwrap();

        assert_eq!(
            Some(AddedDeletedWorkloads {
                added_workloads: vec![rendered_workload],
                deleted_workloads: vec![DeletedWorkload {
                    instance_name: templated_workload.instance_name.clone(),
                    ..Default::default()
                }],
            }),
            added_deleted_workloads
        );
    }

    // [utest->swdd~server-detects-new-workload~1]
    // [utest->swdd~server-state-compares-rendered-workloads~1]
    #[test]
    fn utest_server_state_extract_added_and_deleted_workloads_new_workloads() {
        let current_rendered_workloads = RenderedWorkloads::default();

        let new_state = generate_test_update_state();
        let new_rendered_workloads =
            generate_rendered_workloads_from_state(&new_state.desired_state);

        let added_deleted_workloads = extract_added_and_deleted_workloads(
            &current_rendered_workloads,
            &new_rendered_workloads,
        );
        assert!(added_deleted_workloads.is_some());

        let AddedDeletedWorkloads {
            mut added_workloads,
            deleted_workloads,
        } = added_deleted_workloads.unwrap();

        added_workloads.sort_by(|left, right| {
            left.instance_name
                .workload_name()
                .cmp(right.instance_name.workload_name())
        });

        let mut expected_added_workloads: Vec<WorkloadNamed> = new_state
            .clone()
            .desired_state
            .workloads
            .workloads
            .iter()
            .map(|(name, wl_spec)| WorkloadNamed::from((name.clone(), wl_spec.clone())))
            .collect();
        expected_added_workloads.sort_by(|left, right| {
            left.instance_name
                .workload_name()
                .cmp(right.instance_name.workload_name())
        });

        assert_eq!(added_workloads, expected_added_workloads);
        assert_eq!(deleted_workloads, Vec::default());
    }

    // [utest->swdd~server-detects-deleted-workload~1]
    // [utest->swdd~server-state-compares-rendered-workloads~1]
    #[test]
    fn utest_server_state_extract_added_and_deleted_workloads_deleted_workloads() {
        let current_complete_state = generate_test_old_state();
        let current_rendered_workloads =
            generate_rendered_workloads_from_state(&current_complete_state.desired_state);

        let new_rendered_workloads = RenderedWorkloads::default();

        let added_deleted_workloads = extract_added_and_deleted_workloads(
            &current_rendered_workloads,
            &new_rendered_workloads,
        );

        assert!(added_deleted_workloads.is_some());

        let AddedDeletedWorkloads {
            added_workloads,
            mut deleted_workloads,
        } = added_deleted_workloads.unwrap();

        let expected_added_workloads: Vec<WorkloadNamed> = Vec::new();
        assert_eq!(added_workloads, expected_added_workloads);

        deleted_workloads.sort_by(|left, right| {
            left.instance_name
                .workload_name()
                .cmp(right.instance_name.workload_name())
        });
        let mut expected_deleted_workloads: Vec<DeletedWorkload> = current_complete_state
            .desired_state
            .workloads
            .workloads
            .iter()
            .map(|(name, wl_spec)| {
                let wl_named = WorkloadNamed::from((name.clone(), wl_spec.clone()));
                DeletedWorkload {
                    instance_name: wl_named.instance_name,
                    dependencies: HashMap::new(),
                }
            })
            .collect();
        expected_deleted_workloads.sort_by(|left, right| {
            left.instance_name
                .workload_name()
                .cmp(right.instance_name.workload_name())
        });
        assert_eq!(deleted_workloads, expected_deleted_workloads);
    }

    // [utest->swdd~server-state-triggers-validation-of-workload-fields~1]
    #[test]
    fn utest_server_state_update_state_triggers_workload_field_validations() {
        let invalid_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            "invalid.agent?name",
            fixtures::RUNTIME_NAMES[0],
        );
        let mut server_state = ServerState::default();
        let cloned_invalid_workload = invalid_workload.clone();
        server_state
            .config_renderer
            .expect_render_workloads()
            .once()
            .return_once(|_, _| {
                Ok(RenderedWorkloads::from([(
                    fixtures::WORKLOAD_NAMES[0].to_string(),
                    cloned_invalid_workload,
                )]))
            });

        server_state.delete_graph.expect_insert().never();
        server_state
            .delete_graph
            .expect_apply_delete_conditions_to()
            .never();

        let result = server_state.update(StateSpec {
            workloads: WorkloadMapSpec {
                workloads: HashMap::from([(
                    fixtures::WORKLOAD_NAMES[0].to_string(),
                    invalid_workload.workload,
                )]),
            },
            ..Default::default()
        });

        assert!(matches!(result, Err(UpdateStateError::ResultInvalid(_))));
    }

    // [utest->swdd~server-state-stores-delete-condition~1]
    // [utest->swdd~server-state-adds-delete-conditions-to-deleted-workload~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_store_and_add_delete_conditions() {
        let workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let current_complete_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        workload.instance_name.workload_name().to_owned(),
                        workload.workload.clone(),
                    )]),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let new_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
        );

        let new_complete_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(
                        new_workload.instance_name.workload_name().to_owned(),
                        new_workload.workload.clone(),
                    )]),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock
            .expect_insert()
            .with(mockall::predicate::eq(vec![new_workload]))
            .once()
            .return_const(());
        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .with(mockall::predicate::eq(vec![DeletedWorkload {
                instance_name: workload.instance_name,
                dependencies: HashMap::new(),
            }]))
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        let cloned_expected_state = new_complete_state.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Ok(generate_rendered_workloads_from_state(
                    &cloned_expected_state,
                ))
            });

        let mut server_state = ServerState {
            rendered_workloads: generate_rendered_workloads_from_state(
                &current_complete_state.desired_state,
            ),
            state: current_complete_state,
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let added_deleted_workloads = server_state
            .update(new_complete_state.desired_state)
            .unwrap();
        assert!(added_deleted_workloads.is_some());
    }

    // [utest->swdd~server-removes-obsolete-delete-graph-entires~1]
    #[test]
    fn utest_remove_deleted_workloads_from_delete_graph() {
        let mut mock_delete_graph = MockDeleteGraph::default();
        mock_delete_graph
            .expect_remove_deleted_workloads_from_delete_graph()
            .once()
            .return_const(());

        let mut server_state = ServerState {
            delete_graph: mock_delete_graph,
            ..Default::default()
        };

        let workload_states = vec![];

        server_state.cleanup_state(&workload_states);
    }

    fn generate_test_old_state() -> CompleteStateSpec {
        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let mut w2 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[1],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[1],
        );
        let mut w3 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
        );
        w2.workload.dependencies.dependencies.clear();
        w3.workload.dependencies.dependencies.clear();

        generate_test_complete_state(vec![w1, w2, w3])
    }

    fn generate_test_update_state() -> CompleteStateSpec {
        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[1],
        );
        let mut w3 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[2],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[1],
        );
        let mut w4 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[3],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        w3.workload.dependencies.dependencies.clear();
        w4.workload.dependencies.dependencies.clear();

        generate_test_complete_state(vec![w1, w3, w4])
    }
}

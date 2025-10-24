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

use super::config_renderer::RenderedWorkloads;
use api::ank_base;

#[cfg_attr(test, mockall_double::double)]
use super::config_renderer::ConfigRenderer;

use super::cycle_check;
#[cfg_attr(test, mockall_double::double)]
use super::delete_graph::DeleteGraph;
use common::objects::{AgentMap, State, WorkloadInstanceName, WorkloadState, WorkloadStatesMap};
use common::std_extensions::IllegalStateResult;
use common::{
    objects::{CompleteState, DeletedWorkload, WorkloadSpec},
    state_manipulation::{Object, Path, StateComparator},
};
use std::fmt::Display;

#[cfg(test)]
use mockall::automock;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StateGenerationResult {
    pub state_comparator: StateComparator,
    pub new_desired_state: State,
}

// [impl->swdd~server-state-triggers-validation-of-workload-fields~1]
fn verify_workload_fields_format(workloads: &RenderedWorkloads) -> Result<(), UpdateStateError> {
    for workload_spec in workloads.values() {
        workload_spec
            .verify_fields_format()
            .map_err(UpdateStateError::ResultInvalid)?;
    }
    Ok(())
}

fn extract_added_and_deleted_workloads(
    current_workloads: &RenderedWorkloads,
    new_workloads: &RenderedWorkloads,
) -> Option<AddedDeletedWorkloads> {
    let mut added_workloads: Vec<WorkloadSpec> = Vec::new();
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
    state: CompleteState,
    rendered_workloads: RenderedWorkloads,
    delete_graph: DeleteGraph,
    config_renderer: ConfigRenderer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddedDeletedWorkloads {
    pub added_workloads: Vec<WorkloadSpec>,
    pub deleted_workloads: Vec<DeletedWorkload>,
}

#[cfg_attr(test, automock)]
impl ServerState {
    const API_VERSION_FILTER_MASK: &'static str = "desiredState.apiVersion";
    const DESIRED_STATE_FIELD_MASK_PART: &'static str = "desiredState";

    // [impl->swdd~server-provides-interface-get-complete-state~2]
    // [impl->swdd~server-filters-get-complete-state-result~2]
    pub fn get_complete_state_by_field_mask(
        &self,
        field_masks: Vec<String>,
        workload_states_map: &WorkloadStatesMap,
        agent_map: &AgentMap,
    ) -> Result<ank_base::CompleteState, String> {
        let current_complete_state: ank_base::CompleteState = CompleteState {
            desired_state: self.state.desired_state.clone(),
            workload_states: workload_states_map.clone(),
            agents: agent_map.clone(),
        }
        .into();

        if !field_masks.is_empty() {
            let mut filters = field_masks;
            if filters
                .iter()
                .any(|field| field.starts_with(Self::DESIRED_STATE_FIELD_MASK_PART))
            {
                filters.push(Self::API_VERSION_FILTER_MASK.to_owned());
            }

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
    pub fn get_workloads_for_agent(&self, agent_name: &str) -> Vec<WorkloadSpec> {
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
            .get(instance_name.workload_name())
            .is_some_and(|workload_spec| workload_spec.instance_name == *instance_name)
    }

    pub fn update(
        &mut self,
        new_desired_state: State,
    ) -> Result<Option<AddedDeletedWorkloads>, UpdateStateError> {
        // [impl->swdd~update-desired-state-with-update-mask~1]
        // [impl->swdd~update-desired-state-empty-update-mask~1]
        // [impl->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
        let new_rendered_workloads = self
            .config_renderer
            .render_workloads(&new_desired_state.workloads, &new_desired_state.configs)
            .map_err(|err| UpdateStateError::ResultInvalid(err.to_string()))?;

        // [impl->swdd~server-state-triggers-validation-of-workload-fields~1]
        verify_workload_fields_format(&new_rendered_workloads)?;

        // [impl->swdd~server-state-compares-rendered-workloads~1]
        let added_deleted_workloads =
            extract_added_and_deleted_workloads(&self.rendered_workloads, &new_rendered_workloads);

        if let Some(mut added_deleted_workloads) = added_deleted_workloads {
            let added_workloads = &added_deleted_workloads.added_workloads;

            let start_nodes: Vec<&str> = added_workloads
                .iter()
                .filter_map(|w| {
                    if !w.dependencies.is_empty() {
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
        // [impl->swdd~update-desired-state-with-update-mask~1]
        let old_state: Object = (&self.state).try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Failed to parse current state, '{err}'"))
        })?;
        let state_from_update: Object = (&updated_state).try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Failed to parse new state, '{err}'"))
        })?;

        // [impl->swdd~update-desired-state-empty-update-mask~1]
        if update_mask.is_empty() {
            return Ok(StateGenerationResult {
                state_comparator: StateComparator::new(old_state.into(), state_from_update.into()),
                new_desired_state: updated_state.desired_state,
            });
        }

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

        let new_complete_state: CompleteState = new_state.clone().try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Could not parse into CompleteState: '{err}'"))
        })?;

        Ok(StateGenerationResult {
            state_comparator: StateComparator::new(old_state.into(), new_state.into()),
            new_desired_state: new_complete_state.desired_state,
        })
    }

    // [impl->swdd~server-cleans-up-state~1]
    pub fn cleanup_state(&mut self, new_workload_states: &[WorkloadState]) {
        // [impl->swdd~server-removes-obsolete-delete-graph-entires~1]
        self.delete_graph
            .remove_deleted_workloads_from_delete_graph(new_workload_states);
    }

    fn set_desired_state(&mut self, new_desired_state: State) {
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
    use std::collections::HashMap;

    use api::ank_base::{self, Dependencies, Tags};
    use common::{
        commands::CompleteStateRequest,
        objects::{
            AgentMap, CompleteState, DeletedWorkload, State, WorkloadSpec, WorkloadStatesMap,
            generate_test_configs, generate_test_runtime_config,
            generate_test_stored_workload_spec, generate_test_workload_spec,
            generate_test_workload_spec_with_control_interface_access,
            generate_test_workload_spec_with_param,
            generate_test_workload_spec_with_runtime_config,
        },
        test_utils::{self, generate_test_complete_state, generate_test_state_from_workloads},
    };

    use crate::ankaios_server::{
        config_renderer::{ConfigRenderError, MockConfigRenderer, RenderedWorkloads},
        delete_graph::MockDeleteGraph,
        server_state::{
            AddedDeletedWorkloads, UpdateStateError, extract_added_and_deleted_workloads,
        },
    };

    use super::ServerState;
    const AGENT_A: &str = "agent_A";
    const AGENT_B: &str = "agent_B";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const WORKLOAD_NAME_3: &str = "workload_3";
    const WORKLOAD_NAME_4: &str = "workload_4";
    const RUNTIME: &str = "runtime";

    fn generate_rendered_workloads_from_state(state: &State) -> RenderedWorkloads {
        state
            .workloads
            .iter()
            .map(|(name, spec)| {
                (
                    name.to_owned(),
                    WorkloadSpec::from((name.to_owned(), spec.to_owned())),
                )
            })
            .collect()
    }

    // [utest->swdd~server-provides-interface-get-complete-state~2]
    // [utest->swdd~server-filters-get-complete-state-result~2]
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
                request_complete_state.field_mask,
                &workload_states_map,
                &agent_map,
            )
            .unwrap();

        let mut expected_complete_state = server_state.state.clone();
        expected_complete_state.workload_states = workload_states_map;
        expected_complete_state.agents = agent_map;

        let expected_complete_state = ank_base::CompleteState::from(expected_complete_state);
        assert_eq!(received_complete_state, expected_complete_state);
    }

    // [utest->swdd~server-provides-interface-get-complete-state~2]
    // [utest->swdd~server-filters-get-complete-state-result~2]
    #[test]
    fn utest_server_state_get_complete_state_by_field_mask_continue_on_invalid_mask() {
        let w1 = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );
        let mut server_state = ServerState {
            state: generate_test_complete_state(vec![w1]),
            ..Default::default()
        };

        server_state.state.workload_states = WorkloadStatesMap::default();
        server_state.state.agents = AgentMap::default();

        let request_complete_state = CompleteStateRequest {
            field_mask: vec![
                "workloads.invalidMask".to_string(), // invalid not existing workload
                format!("desiredState.workloads.{}", WORKLOAD_NAME_1), // valid existing workload
            ],
            subscribe_for_events: false,
        };

        let received_complete_state = server_state
            .get_complete_state_by_field_mask(
                request_complete_state.field_mask,
                &WorkloadStatesMap::default(),
                &AgentMap::default(),
            )
            .unwrap();

        let mut expected_complete_state = ank_base::CompleteState {
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
        let w1 = generate_test_workload_spec_with_control_interface_access(
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
            state: generate_test_complete_state(vec![w1.clone(), w2.clone(), w3.clone()]),
            ..Default::default()
        };

        let request_complete_state = CompleteStateRequest {
            field_mask: vec![
                format!("desiredState.workloads.{}", WORKLOAD_NAME_1),
                format!("desiredState.workloads.{}.agent", WORKLOAD_NAME_3),
                format!("desiredState.workloads.*.runtime"),
            ],
            subscribe_for_events: false,
        };

        let mut workload_state_map = WorkloadStatesMap::default();
        workload_state_map.process_new_states(server_state.state.workload_states.clone().into());

        let complete_state = server_state
            .get_complete_state_by_field_mask(
                request_complete_state.field_mask,
                &workload_state_map,
                &AgentMap::default(),
            )
            .unwrap();

        let expected_workloads = [
            (
                w3.instance_name.workload_name(),
                ank_base::Workload {
                    agent: Some(w3.instance_name.agent_name().to_string()),
                    restart_policy: None,
                    dependencies: None,
                    tags: None,
                    runtime: Some(w3.runtime.clone()),
                    runtime_config: None,
                    control_interface_access: None,
                    configs: None,
                    files: None,
                },
            ),
            (
                w2.instance_name.workload_name(),
                ank_base::Workload {
                    agent: None,
                    restart_policy: None,
                    dependencies: None,
                    tags: None,
                    runtime: Some(w2.runtime.clone()),
                    runtime_config: None,
                    control_interface_access: None,
                    configs: None,
                    files: None,
                },
            ),
            (
                w1.instance_name.workload_name(),
                ank_base::Workload {
                    agent: Some(w1.instance_name.agent_name().to_string()),
                    restart_policy: Some(w1.restart_policy as i32),
                    dependencies: Some(Dependencies {
                        dependencies: w1
                            .dependencies
                            .into_iter()
                            .map(|(k, v)| (k, v as i32))
                            .collect(),
                    }),
                    tags: Some(Tags {
                        tags: w1.tags.into_iter().map(ank_base::Tag::from).collect(),
                    }),
                    runtime: Some(w1.runtime.clone()),
                    runtime_config: Some(w1.runtime_config.clone()),
                    control_interface_access: w1.control_interface_access.into(),
                    configs: Some(Default::default()),
                    files: Some(ank_base::Files {
                        files: w1.files.into_iter().map(Into::into).collect(),
                    }),
                },
            ),
        ];
        let mut expected_complete_state =
            test_utils::generate_test_proto_complete_state(&expected_workloads);
        if let Some(expected_desired_state) = &mut expected_complete_state.desired_state {
            expected_desired_state.configs = None;
        }

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

        let old_complete_state =
            generate_test_complete_state(vec![w1.clone(), w2.clone(), w3.clone()]);

        let server_state = ServerState {
            rendered_workloads: generate_rendered_workloads_from_state(
                &old_complete_state.desired_state,
            ),
            state: old_complete_state,
            ..Default::default()
        };

        let mut workloads = server_state.get_workloads_for_agent(AGENT_A);
        workloads.sort_by(|left, right| {
            left.instance_name
                .workload_name()
                .cmp(right.instance_name.workload_name())
        });
        assert_eq!(workloads, vec![w1, w2]);

        let workloads = server_state.get_workloads_for_agent(AGENT_B);
        assert_eq!(workloads, vec![w3]);

        let workloads = server_state.get_workloads_for_agent("unknown_agent");
        assert_eq!(workloads.len(), 0);
    }

    // [utest->swdd~server-handles-logs-request-message~1]
    #[test]
    fn utest_desired_state_contains_instance_name() {
        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        let other_workload_instance_name = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_2.to_string(),
            RUNTIME.to_string(),
        )
        .instance_name;

        let complete_state = generate_test_complete_state(vec![workload_spec.clone()]);

        let server_state = ServerState {
            rendered_workloads: generate_rendered_workloads_from_state(
                &complete_state.desired_state,
            ),
            state: complete_state,
            ..Default::default()
        };

        let instance_name = workload_spec.instance_name.clone();
        assert!(server_state.desired_state_contains_instance_name(&instance_name));

        assert!(!server_state.desired_state_contains_instance_name(&other_workload_instance_name));
    }

    // [utest->swdd~server-state-rejects-state-with-cyclic-dependencies~1]
    #[test]
    fn utest_server_state_update_state_reject_state_with_cyclic_dependencies() {
        let workload = generate_test_stored_workload_spec(AGENT_A.to_string(), RUNTIME.to_string());

        // workload has a self cycle to workload_A
        let new_workload_1 =
            generate_test_stored_workload_spec(AGENT_A.to_string(), RUNTIME.to_string());

        let mut new_workload_2 =
            generate_test_stored_workload_spec(AGENT_A.to_string(), RUNTIME.to_string());
        new_workload_2.dependencies.clear();

        let old_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(WORKLOAD_NAME_1.to_string(), workload)]),
                ..Default::default()
            },
            ..Default::default()
        };

        let rejected_new_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([
                    ("workload_A".to_string(), new_workload_1),
                    (WORKLOAD_NAME_1.to_string(), new_workload_2),
                ]),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().never();
        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .never();

        let mut mock_config_renderer = MockConfigRenderer::new();
        let cloned_rejected_state = rejected_new_state.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Ok(generate_rendered_workloads_from_state(
                    &cloned_rejected_state,
                ))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let result = server_state.update(rejected_new_state.desired_state);
        assert_eq!(
            result,
            Err(UpdateStateError::CycleInDependencies(
                "workload_A".to_string()
            ))
        );

        // server state shall be the old state, new state shall be rejected
        assert_eq!(old_state, server_state.state);
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_replace_all_if_update_mask_empty() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![];

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let state_generation_result = server_state
            .generate_new_state(update_state.clone(), update_mask)
            .unwrap();

        assert_eq!(
            update_state.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_replace_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_1)];

        let new_workload = update_state
            .desired_state
            .workloads
            .get(WORKLOAD_NAME_1)
            .unwrap()
            .clone();

        let mut expected = old_state.clone();
        expected
            .desired_state
            .workloads
            .insert(WORKLOAD_NAME_1.to_owned(), new_workload.clone());

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let state_generation_result = server_state
            .generate_new_state(update_state, update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_add_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_4)];

        let new_workload = update_state
            .desired_state
            .workloads
            .get(WORKLOAD_NAME_4)
            .unwrap()
            .clone();

        let mut expected = old_state.clone();
        expected
            .desired_state
            .workloads
            .insert(WORKLOAD_NAME_4.into(), new_workload.clone());

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let state_generation_result = server_state
            .generate_new_state(update_state, update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_update_configs() {
        let old_state = generate_test_old_state();
        let mut state_with_updated_config = old_state.clone();
        state_with_updated_config.desired_state.configs = generate_test_configs();

        let update_mask = vec!["desiredState".to_string()];

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };

        let expected = state_with_updated_config.clone();

        let state_generation_result = server_state
            .generate_new_state(state_with_updated_config, update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_remove_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![format!("desiredState.workloads.{}", WORKLOAD_NAME_2)];

        let mut expected = old_state.clone();
        expected
            .desired_state
            .workloads
            .remove(WORKLOAD_NAME_2)
            .unwrap();

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let state_generation_result = server_state
            .generate_new_state(update_state, update_mask)
            .unwrap();

        assert_eq!(
            expected.desired_state,
            state_generation_result.new_desired_state
        );
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_update_state_remove_non_existing_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["desiredState.workloads.workload_5".into()];

        let expected = &old_state;

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let state_generation_result = server_state
            .generate_new_state(update_state, update_mask)
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
        let field_mask = "desiredState.workloads.workload_2.tags.x";
        let update_mask = vec![field_mask.into()];

        let server_state = ServerState {
            state: old_state.clone(),
            ..Default::default()
        };
        let result = server_state.generate_new_state(update_state, update_mask);

        assert_eq!(
            Err(UpdateStateError::FieldNotFound(field_mask.into())),
            result
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
        let result = server_state.generate_new_state(update_state, update_mask);
        assert_eq!(Err(UpdateStateError::FieldNotFound("".into())), result);
        assert_eq!(server_state.state, old_state);
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    #[test]
    fn utest_server_state_generate_new_state_no_changes_on_equal_states() {
        let server_state = ServerState::default(); // empty old state

        let state_generation_result = server_state
            .generate_new_state(CompleteState::default(), vec![])
            .unwrap();
        assert_eq!(state_generation_result.new_desired_state, State::default());
        assert_eq!(server_state.state, CompleteState::default());
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
            .returning(|_, _| Ok(HashMap::new()));

        let mut server_state = ServerState {
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
            ..Default::default()
        };

        let added_deleted_workloads = server_state.update(State::default()).unwrap();
        assert!(added_deleted_workloads.is_none());
        assert_eq!(server_state.state, CompleteState::default());
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

        let new_state_with_configs = CompleteState {
            desired_state: State {
                configs: generate_test_configs(),
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
        old_state.desired_state.configs = generate_test_configs();

        let mut updated_state = old_state.clone();
        updated_state.desired_state.configs.clear();

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
        let templated_workload = generate_test_workload_spec_with_runtime_config(
            AGENT_A.to_owned(),
            WORKLOAD_NAME_1.to_owned(),
            RUNTIME.to_owned(),
            templated_runtime_config,
        );

        let unchanged_workload = generate_test_workload_spec();

        let old_state = CompleteState {
            desired_state: generate_test_state_from_workloads(vec![
                templated_workload.clone(),
                unchanged_workload.clone(),
            ]),
            ..Default::default()
        };

        let mut rendered_workload = templated_workload.clone();
        rendered_workload.runtime_config = generate_test_runtime_config();

        // old and new state are identical but the workload has been changed after rendering
        let new_state = old_state.clone();

        let mut server_state = ServerState {
            state: old_state,
            rendered_workloads: RenderedWorkloads::from([
                (WORKLOAD_NAME_1.to_owned(), rendered_workload.clone()),
                (WORKLOAD_NAME_2.to_owned(), unchanged_workload.clone()),
            ]),
            ..Default::default()
        };

        rendered_workload.runtime_config = "updated_runtime_config_after_rendering".to_owned();
        let new_rendered_workloads = RenderedWorkloads::from([
            (WORKLOAD_NAME_1.to_owned(), rendered_workload.clone()),
            (WORKLOAD_NAME_2.to_owned(), unchanged_workload.clone()),
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

        let mut expected_added_workloads: Vec<WorkloadSpec> = new_state
            .clone()
            .desired_state
            .workloads
            .iter()
            .map(|(name, spec)| (name.to_owned(), spec.to_owned()).into())
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

        let expected_added_workloads: Vec<WorkloadSpec> = Vec::new();
        assert_eq!(added_workloads, expected_added_workloads);

        deleted_workloads.sort_by(|left, right| {
            left.instance_name
                .workload_name()
                .cmp(right.instance_name.workload_name())
        });
        let mut expected_deleted_workloads: Vec<DeletedWorkload> = current_complete_state
            .desired_state
            .workloads
            .iter()
            .map(|(name, workload_spec)| DeletedWorkload {
                instance_name: (name.to_owned(), workload_spec).into(),
                dependencies: HashMap::new(),
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
        let invalid_workload = generate_test_workload_spec_with_param(
            "invalid.agent?name".to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );
        let mut server_state = ServerState::default();
        let cloned_invalid_workload = invalid_workload.clone();
        server_state
            .config_renderer
            .expect_render_workloads()
            .once()
            .return_once(|_, _| {
                Ok(RenderedWorkloads::from([(
                    WORKLOAD_NAME_1.to_string(),
                    cloned_invalid_workload,
                )]))
            });

        server_state.delete_graph.expect_insert().never();
        server_state
            .delete_graph
            .expect_apply_delete_conditions_to()
            .never();

        let result = server_state.update(State {
            workloads: HashMap::from([(WORKLOAD_NAME_1.to_string(), invalid_workload.into())]),
            ..Default::default()
        });
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported agent name.")
        );
    }

    // [utest->swdd~server-state-stores-delete-condition~1]
    // [utest->swdd~server-state-adds-delete-conditions-to-deleted-workload~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_store_and_add_delete_conditions() {
        let workload = generate_test_workload_spec_with_param(
            AGENT_A.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME.to_string(),
        );

        let current_complete_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(
                    workload.instance_name.workload_name().to_owned(),
                    workload.clone().into(),
                )]),
                ..Default::default()
            },
            ..Default::default()
        };

        let new_workload = generate_test_workload_spec_with_param(
            AGENT_B.to_string(),
            workload.instance_name.workload_name().to_owned(),
            RUNTIME.to_string(),
        );

        let new_complete_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([(
                    new_workload.instance_name.workload_name().to_owned(),
                    new_workload.clone().into(),
                )]),
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

    fn generate_test_old_state() -> CompleteState {
        generate_test_complete_state(vec![
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
        ])
    }

    fn generate_test_update_state() -> CompleteState {
        generate_test_complete_state(vec![
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
        ])
    }
}

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

use ankaios_api::ank_base::{
    AgentAttributesSpec, AgentStatusSpec, CompleteState, CompleteStateRequestSpec,
    CompleteStateSpec, CpuUsageSpec, DeletedWorkload, FreeMemorySpec, StateSpec, TagsSpec,
    WorkloadInstanceNameSpec, WorkloadNamed, WorkloadStateSpec, WorkloadStatesMapSpec,
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

fn extract_added_and_deleted_workloads(
    current_workloads: &RenderedWorkloads,
    new_workloads: &RenderedWorkloads,
) -> Option<(Vec<WorkloadNamed>, Vec<DeletedWorkload>)> {
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
        return None;
    }

    Some((added_workloads, deleted_workloads))
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

pub type AddedDeletedWorkloads = Option<(Vec<WorkloadNamed>, Vec<DeletedWorkload>)>;

#[cfg_attr(test, automock)]
impl ServerState {
    const API_VERSION_FILTER_MASK: &'static str = "desiredState.apiVersion";
    const DESIRED_STATE_FIELD_MASK_PART: &'static str = "desiredState";

    // [impl->swdd~server-provides-interface-get-complete-state~2]
    // [impl->swdd~server-filters-get-complete-state-result~2]
    pub fn get_complete_state_by_field_mask(
        &self,
        request_complete_state: CompleteStateRequestSpec,
        workload_states_map: &WorkloadStatesMapSpec,
    ) -> Result<CompleteState, String> {
        let current_complete_state: CompleteState = CompleteStateSpec {
            desired_state: self.state.desired_state.clone(),
            workload_states: workload_states_map.clone(),
            agents: self.state.agents.clone(),
        }
        .into();

        if !request_complete_state.field_mask.is_empty() {
            let mut filters = request_complete_state.field_mask;
            if filters
                .iter()
                .any(|field| field.starts_with(Self::DESIRED_STATE_FIELD_MASK_PART))
            {
                filters.push(Self::API_VERSION_FILTER_MASK.to_owned());
            }

            let current_complete_state: Object =
                current_complete_state.try_into().unwrap_or_illegal_state();
            let mut return_state = Object::default();

            log::debug!("Current state: {current_complete_state:?}");
            for field in &filters {
                if let Some(value) = current_complete_state.get(&field.into()) {
                    return_state.set(&field.into(), value.to_owned())?;
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
        instance_name: &WorkloadInstanceNameSpec,
    ) -> bool {
        self.rendered_workloads
            .get(instance_name.workload_name())
            .is_some_and(|workload| workload.instance_name == *instance_name)
    }

    pub fn update(
        &mut self,
        new_state: CompleteStateSpec,
        update_mask: Vec<String>,
    ) -> Result<AddedDeletedWorkloads, UpdateStateError> {
        // [impl->swdd~update-desired-state-with-update-mask~1]
        // [impl->swdd~update-desired-state-empty-update-mask~1]
        match self.generate_new_state(new_state, update_mask) {
            Ok(new_templated_state) => {
                // [impl->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
                let new_rendered_workloads = self
                    .config_renderer
                    .render_workloads(
                        &new_templated_state.desired_state.workloads.workloads,
                        &new_templated_state.desired_state.configs.configs,
                    )
                    .map_err(|err| UpdateStateError::ResultInvalid(err.to_string()))?;

                // [impl->swdd~server-state-triggers-validation-of-workload-fields~1]
                new_rendered_workloads
                    .validate()
                    .map_err(UpdateStateError::ResultInvalid)?;

                // [impl->swdd~server-state-compares-rendered-workloads~1]
                let cmd = extract_added_and_deleted_workloads(
                    &self.rendered_workloads,
                    &new_rendered_workloads,
                );

                if let Some((added_workloads, mut deleted_workloads)) = cmd {
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
                        cycle_check::dfs(&new_templated_state.desired_state, Some(start_nodes))
                    {
                        return Err(UpdateStateError::CycleInDependencies(
                            workload_part_of_cycle,
                        ));
                    }

                    // [impl->swdd~server-state-stores-delete-condition~1]
                    self.delete_graph.insert(&added_workloads);

                    // [impl->swdd~server-state-adds-delete-conditions-to-deleted-workload~1]
                    self.delete_graph
                        .apply_delete_conditions_to(&mut deleted_workloads);

                    self.set_desired_state(new_templated_state.desired_state);
                    self.rendered_workloads = new_rendered_workloads;
                    Ok(Some((added_workloads, deleted_workloads)))
                } else {
                    // update state with changed fields not affecting workloads, e.g. config items
                    // [impl->swdd~server-state-updates-state-on-unmodified-workloads~1]
                    self.set_desired_state(new_templated_state.desired_state);
                    Ok(None)
                }
            }
            Err(error) => Err(error),
        }
    }

    // [impl->swdd~server-state-stores-agent-in-complete-state~1]
    pub fn add_agent(&mut self, agent_name: String, tags: TagsSpec) {
        self.state
            .agents
            .agents
            .entry(agent_name)
            .or_insert(AgentAttributesSpec {
                status: Default::default(),
                tags,
            });
    }

    // [impl->swdd~server-state-removes-agent-from-complete-state~1]
    pub fn remove_agent(&mut self, agent_name: &str) {
        self.state.agents.agents.remove(agent_name);
    }

    // [impl->swdd~server-state-provides-connected-agent-exists-check~1]
    pub fn contains_connected_agent(&self, agent_name: &str) -> bool {
        self.state.agents.agents.contains_key(agent_name)
    }

    // [impl->swdd~server-updates-resource-availability~1]
    pub fn update_agent_resource_availability(
        &mut self,
        agent_load_status: common::commands::AgentLoadStatus,
    ) {
        self.state
            .agents
            .agents
            .entry(agent_load_status.agent_name)
            .and_modify(|e| {
                e.status = Some(AgentStatusSpec {
                    cpu_usage: Some(CpuUsageSpec {
                        cpu_usage: agent_load_status.cpu_usage.cpu_usage,
                    }),
                    free_memory: Some(FreeMemorySpec {
                        free_memory: agent_load_status.free_memory.free_memory,
                    }),
                });
            });
    }

    // [impl->swdd~server-cleans-up-state~1]
    pub fn cleanup_state(&mut self, new_workload_states: &[WorkloadStateSpec]) {
        // [impl->swdd~server-removes-obsolete-delete-graph-entires~1]
        self.delete_graph
            .remove_deleted_workloads_from_delete_graph(new_workload_states);
    }

    fn generate_new_state(
        &mut self,
        updated_state: CompleteStateSpec,
        update_mask: Vec<String>,
    ) -> Result<CompleteStateSpec, UpdateStateError> {
        // [impl->swdd~update-desired-state-empty-update-mask~1]
        if update_mask.is_empty() {
            return Ok(updated_state);
        }

        // [impl->swdd~update-desired-state-with-update-mask~1]
        let mut new_state: Object = (&self.state).try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Failed to parse current state, '{err}'"))
        })?;
        let state_from_update: Object = updated_state.try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Failed to parse new state, '{err}'"))
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

        new_state.try_into().map_err(|err| {
            UpdateStateError::ResultInvalid(format!("Could not parse into CompleteState: '{err}'"))
        })
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
    use crate::ankaios_server::{
        config_renderer::{ConfigRenderError, MockConfigRenderer},
        delete_graph::MockDeleteGraph,
        rendered_workloads::RenderedWorkloads,
        server_state::UpdateStateError,
    };

    use ankaios_api::ank_base::{
        AgentMapSpec, CompleteState, CompleteStateRequestSpec, CompleteStateSpec,
        ConfigItemEnumSpec, ConfigItemSpec, ConfigMapSpec, ConfigObjectSpec, DeletedWorkload,
        StateSpec, Workload, WorkloadInstanceNameSpec, WorkloadMapSpec, WorkloadNamed,
        WorkloadStateSpec, WorkloadStatesMapSpec,
    };
    use ankaios_api::test_utils::{
        fixtures, generate_test_agent_map, generate_test_agent_tags, generate_test_complete_state,
        generate_test_config_map, generate_test_proto_complete_state, generate_test_workload,
        generate_test_workload_named_with_params, generate_test_workload_with_params,
    };
    use common::commands::AgentLoadStatus;

    use mockall::predicate;
    use std::collections::HashMap;

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

        let server_state = ServerState {
            state: generate_test_complete_state(vec![w1.clone(), w2.clone(), w3.clone()]),
            ..Default::default()
        };

        let request_complete_state = CompleteStateRequestSpec { field_mask: vec![] };

        let mut workload_state_db = WorkloadStatesMapSpec::default();
        workload_state_db
            .process_new_states(from_map_to_vec(server_state.state.workload_states.clone()));

        let received_complete_state = server_state
            .get_complete_state_by_field_mask(request_complete_state, &workload_state_db)
            .unwrap();

        let expected_complete_state = CompleteState::from(server_state.state);
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

        let server_state = ServerState {
            state: generate_test_complete_state(vec![w1.clone()]),
            ..Default::default()
        };

        let request_complete_state = CompleteStateRequestSpec {
            field_mask: vec![
                "workloads.invalidMask".to_string(), // invalid not existing workload
                format!("desiredState.workloads.{}", fixtures::WORKLOAD_NAMES[0]),
            ],
        };

        let mut workload_state_map = WorkloadStatesMapSpec::default();
        workload_state_map
            .process_new_states(from_map_to_vec(server_state.state.workload_states.clone()));

        let received_complete_state = server_state
            .get_complete_state_by_field_mask(request_complete_state, &workload_state_map)
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

        let request_complete_state = CompleteStateRequestSpec {
            field_mask: vec![
                format!("desiredState.workloads.{}", fixtures::WORKLOAD_NAMES[0]),
                format!(
                    "desiredState.workloads.{}.agent",
                    fixtures::WORKLOAD_NAMES[2]
                ),
            ],
        };

        let mut workload_state_map = WorkloadStatesMapSpec::default();
        workload_state_map
            .process_new_states(from_map_to_vec(server_state.state.workload_states.clone()));

        let complete_state = server_state
            .get_complete_state_by_field_mask(request_complete_state, &workload_state_map)
            .unwrap();

        let expected_workloads = [
            (
                w3.instance_name.workload_name(),
                Workload {
                    agent: Some(w3.instance_name.agent_name().to_string()),
                    restart_policy: None,
                    dependencies: None,
                    tags: None,
                    runtime: None,
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

        let mut other_workload_instance_name = workload.instance_name.clone();
        other_workload_instance_name.workload_name = fixtures::WORKLOAD_NAMES[1].to_string();

        let complete_state = generate_test_complete_state(vec![workload.clone()]);

        let server_state = ServerState {
            rendered_workloads: generate_rendered_workloads_from_state(
                &complete_state.desired_state,
            ),
            state: complete_state,
            ..Default::default()
        };

        let instance_name = workload.instance_name.clone();
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
        );

        let mut new_workload_2 = generate_test_workload_with_params(
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        new_workload_2.dependencies.dependencies.clear();

        let old_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([(fixtures::WORKLOAD_NAMES[0].to_string(), workload)]),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let rejected_new_state = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: HashMap::from([
                        ("workload_B".to_string(), new_workload_1),
                        (fixtures::WORKLOAD_NAMES[0].to_string(), new_workload_2),
                    ]),
                },
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

        let result = server_state.update(rejected_new_state, vec![]);
        assert_eq!(
            result,
            Err(UpdateStateError::CycleInDependencies(
                "workload_B".to_string()
            ))
        );

        // server state shall be the old state, new state shall be rejected
        assert_eq!(old_state, server_state.state);
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_replace_all_if_update_mask_empty() {
        let _ = env_logger::builder().is_test(true).try_init();
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec![];

        let mut delete_graph_mock = MockDeleteGraph::new();

        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        let clone_updated_state = update_state.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Ok(generate_rendered_workloads_from_state(&clone_updated_state))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        server_state
            .update(update_state.clone(), update_mask)
            .unwrap();

        assert_eq!(update_state.desired_state, server_state.state.desired_state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_replace_workload() {
        let _ = env_logger::builder().is_test(true).try_init();
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

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        let cloned_expected_state = expected.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Ok(generate_rendered_workloads_from_state(
                    &cloned_expected_state,
                ))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    // [utest->swdd~server-state-triggers-validation-of-workload-fields~1]
    #[test]
    fn utest_server_state_update_state_add_workload() {
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

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Ok(RenderedWorkloads::from([(
                    fixtures::WORKLOAD_NAMES[3].to_owned(),
                    generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[3],
                        new_workload.agent.clone(),
                        new_workload.runtime.clone(),
                    ),
                )]))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    // [utest->swdd~server-state-updates-state-on-unmodified-workloads~1]
    #[test]
    fn utest_server_state_update_state_update_configs_not_affecting_workloads() {
        let old_state = generate_test_old_state();
        let mut state_with_updated_config = old_state.clone();
        state_with_updated_config.desired_state.configs = generate_test_config_map();

        let update_mask = vec!["desiredState".to_string()];

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().never();

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .never();

        let mut mock_config_renderer = MockConfigRenderer::new();
        let cloned_state_with_updated_config = state_with_updated_config.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .with(
                predicate::eq(
                    state_with_updated_config
                        .desired_state
                        .workloads
                        .workloads
                        .clone(),
                ),
                predicate::eq(
                    state_with_updated_config
                        .desired_state
                        .configs
                        .configs
                        .clone(),
                ),
            )
            .returning(move |_, _| {
                Ok(generate_rendered_workloads_from_state(
                    &cloned_state_with_updated_config,
                ))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let expected = state_with_updated_config.clone();

        let added_deleted_workloads = server_state
            .update(state_with_updated_config, update_mask)
            .unwrap();

        assert!(added_deleted_workloads.is_none());
        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_update_workload_with_existing_configs() {
        let mut old_state = generate_test_old_state();
        old_state.desired_state.configs = generate_test_config_map();

        let mut updated_state = old_state.clone();
        updated_state.desired_state.configs = ConfigMapSpec {
            configs: HashMap::from([(
                "config_1".to_string(),
                ConfigItemSpec {
                    config_item_enum: ConfigItemEnumSpec::Object(ConfigObjectSpec {
                        fields: HashMap::from([(
                            "agent_name".to_string(),
                            ConfigItemSpec {
                                config_item_enum: ConfigItemEnumSpec::String(
                                    fixtures::AGENT_NAMES[1].to_owned(),
                                ), // changed agent name in configs
                            },
                        )]),
                    }),
                },
            )]),
        };

        let updated_workload = updated_state
            .desired_state
            .workloads
            .workloads
            .get_mut(fixtures::WORKLOAD_NAMES[0])
            .unwrap();

        updated_workload.runtime_config = "updated runtime config".to_string(); // changed runtime config

        // update mask references only changed workload
        let update_mask = vec![format!(
            "desiredState.workloads.{}",
            fixtures::WORKLOAD_NAMES[0]
        )];

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        let state_to_render = updated_state.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .with(
                predicate::eq(updated_state.desired_state.workloads.workloads.clone()),
                predicate::eq(old_state.desired_state.configs.configs.clone()), // existing configs due to update mask
            )
            .returning(move |_, _| Ok(generate_rendered_workloads_from_state(&state_to_render)));

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let mut expected = updated_state.clone();
        expected.desired_state.configs = old_state.desired_state.configs.clone(); // existing configs due to update mask

        let result = server_state.update(updated_state, update_mask);
        assert!(result.is_ok());

        let (added_workloads, _) = result.unwrap().unwrap_or_default();

        let new_workload = added_workloads
            .iter()
            .find(|w| w.instance_name.workload_name() == fixtures::WORKLOAD_NAMES[0]);

        assert!(new_workload.is_some());
        assert_eq!(
            new_workload.unwrap().instance_name.agent_name(),
            fixtures::AGENT_NAMES[0]
        ); // assume not updated due to update mask

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    // [utest->swdd~server-state-compares-rendered-workloads~1]
    #[test]
    fn utest_server_state_update_state_update_workload_on_changed_configs() {
        let mut old_state = generate_test_old_state();
        old_state.desired_state.configs = generate_test_config_map();

        let mut updated_state = old_state.clone();
        if let Some(config_1) = updated_state
            .desired_state
            .configs
            .configs
            .get_mut("config_1")
            && let ConfigItemEnumSpec::Object(obj) = &mut config_1.config_item_enum
            && let Some(agent_name) = obj.fields.get_mut("agent_name")
        {
            // changed agent name in configs
            agent_name.config_item_enum =
                ConfigItemEnumSpec::String(fixtures::AGENT_NAMES[1].to_owned());
        } else {
            panic!("The configs should have the expected structure.");
        }

        let update_mask = vec!["desiredState.configs".to_string()];

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut state_to_render = updated_state.desired_state.clone();
        let new_rendered_workload = state_to_render
            .workloads
            .workloads
            .get_mut(fixtures::WORKLOAD_NAMES[0])
            .unwrap();
        new_rendered_workload.agent = fixtures::AGENT_NAMES[1].to_owned(); // updated agent name

        let mut mock_config_renderer = MockConfigRenderer::new();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .with(
                predicate::eq(updated_state.desired_state.workloads.workloads.clone()),
                predicate::eq(updated_state.desired_state.configs.configs.clone()),
            )
            .returning(move |_, _| Ok(generate_rendered_workloads_from_state(&state_to_render)));

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let expected = updated_state.clone();

        let result = server_state.update(updated_state, update_mask);
        assert!(result.is_ok());

        let (added_workloads, deleted_workloads) = result.unwrap().unwrap_or_default();

        let new_workload = added_workloads
            .iter()
            .find(|w| w.instance_name.workload_name() == fixtures::WORKLOAD_NAMES[0]);

        assert!(new_workload.is_some());
        assert_eq!(
            new_workload.unwrap().instance_name.agent_name(),
            fixtures::AGENT_NAMES[1]
        ); // updated with new agent name

        let deleted_workload = deleted_workloads
            .iter()
            .find(|w| w.instance_name.workload_name() == fixtures::WORKLOAD_NAMES[0]);
        assert!(deleted_workload.is_some());
        assert_eq!(
            deleted_workload.unwrap().instance_name.agent_name(),
            fixtures::AGENT_NAMES[0]
        ); // deleted with old agent name

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_workload_references_removed_configs() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mut old_state = generate_test_old_state();
        old_state.desired_state.configs = generate_test_config_map();

        let mut updated_state = old_state.clone();
        updated_state.desired_state.configs.configs.clear();

        let update_mask = vec!["desiredState".to_string()];

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

        let result = server_state.update(updated_state, update_mask);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("config item does not exist")
        );

        assert_eq!(old_state, server_state.state); // keep old state
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_remove_workload() {
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

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        let cloned_new_state = expected.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| Ok(generate_rendered_workloads_from_state(&cloned_new_state)));

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(expected, server_state.state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_remove_non_existing_workload() {
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["desiredState.workloads.workload_5".into()];

        let expected = &old_state;

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().never();
        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .never();

        let mut mock_config_renderer = MockConfigRenderer::new();
        let cloned_old_state = old_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| {
                Ok(generate_rendered_workloads_from_state(
                    &cloned_old_state.desired_state,
                ))
            });

        let mut server_state = ServerState {
            state: old_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(&old_state.desired_state),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };
        server_state.update(update_state, update_mask).unwrap();

        assert_eq!(*expected, server_state.state);
    }

    // [utest->swdd~update-desired-state-with-update-mask~1]
    #[test]
    fn utest_server_state_update_state_fails_with_update_mask_empty_string() {
        let _ = env_logger::builder().is_test(true).try_init();
        let old_state = generate_test_old_state();
        let update_state = generate_test_update_state();
        let update_mask = vec!["".into()];

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().never();
        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .never();

        let mut server_state = ServerState {
            state: old_state.clone(),
            delete_graph: delete_graph_mock,
            ..Default::default()
        };
        let result = server_state.update(update_state, update_mask);
        assert!(result.is_err());
        assert_eq!(server_state.state, old_state);
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_no_update() {
        let _ = env_logger::builder().is_test(true).try_init();

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

        let added_deleted_workloads = server_state
            .update(CompleteStateSpec::default(), vec![])
            .unwrap();
        assert!(added_deleted_workloads.is_none());
        assert_eq!(server_state.state, CompleteStateSpec::default());
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    // [utest->swdd~server-detects-new-workload~1]
    #[test]
    fn utest_server_state_update_state_new_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let new_state = generate_test_update_state();
        let update_mask = vec![];

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        let new_state_clone = new_state.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| Ok(generate_rendered_workloads_from_state(&new_state_clone)));

        let mut server_state = ServerState {
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
            ..Default::default()
        };

        let added_deleted_workloads = server_state.update(new_state.clone(), update_mask).unwrap();
        assert!(added_deleted_workloads.is_some());

        let (mut added_workloads, deleted_workloads) = added_deleted_workloads.unwrap();
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

        let expected_deleted_workloads: Vec<DeletedWorkload> = Vec::new();
        assert_eq!(deleted_workloads, expected_deleted_workloads);
        assert_eq!(server_state.state.desired_state, new_state.desired_state);
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    // [utest->swdd~server-detects-deleted-workload~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_deleted_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_complete_state = generate_test_old_state();
        let update_state = CompleteStateSpec::default();
        let update_mask = vec![];

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());

        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(|_, _| Ok(RenderedWorkloads::new()));

        let mut server_state = ServerState {
            state: current_complete_state.clone(),
            delete_graph: delete_graph_mock,
            rendered_workloads: generate_rendered_workloads_from_state(
                &current_complete_state.desired_state,
            ),
            config_renderer: mock_config_renderer,
        };

        let added_deleted_workloads = server_state.update(update_state, update_mask).unwrap();
        assert!(added_deleted_workloads.is_some());

        let (added_workloads, mut deleted_workloads) = added_deleted_workloads.unwrap();
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

        assert_eq!(server_state.state.desired_state, StateSpec::default());
    }

    // [utest->swdd~update-desired-state-empty-update-mask~1]
    // [utest->swdd~server-detects-changed-workload~1]
    // [utest->swdd~server-state-triggers-configuration-rendering-of-workloads~1]
    #[test]
    fn utest_server_state_update_state_updated_workload() {
        let _ = env_logger::builder().is_test(true).try_init();

        let current_complete_state = generate_test_old_state();
        let mut new_complete_state = current_complete_state.clone();
        let update_mask = vec![];

        let workload_to_update = WorkloadNamed::from((
            fixtures::WORKLOAD_NAMES[0].to_owned(),
            current_complete_state
                .desired_state
                .workloads
                .workloads
                .get(fixtures::WORKLOAD_NAMES[0])
                .unwrap()
                .clone(),
        ));

        let updated_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[1],
        );
        new_complete_state.desired_state.workloads.workloads.insert(
            fixtures::WORKLOAD_NAMES[0].to_string(),
            updated_workload.workload.clone(),
        );

        let mut delete_graph_mock = MockDeleteGraph::new();
        delete_graph_mock.expect_insert().once().return_const(());
        delete_graph_mock
            .expect_apply_delete_conditions_to()
            .once()
            .return_const(());

        let mut mock_config_renderer = MockConfigRenderer::new();
        let cloned_new_state = new_complete_state.desired_state.clone();
        mock_config_renderer
            .expect_render_workloads()
            .once()
            .returning(move |_, _| Ok(generate_rendered_workloads_from_state(&cloned_new_state)));

        let mut server_state = ServerState {
            state: current_complete_state.clone(),
            rendered_workloads: generate_rendered_workloads_from_state(
                &current_complete_state.desired_state,
            ),
            delete_graph: delete_graph_mock,
            config_renderer: mock_config_renderer,
        };

        let added_deleted_workloads = server_state
            .update(new_complete_state.clone(), update_mask)
            .unwrap();
        assert!(added_deleted_workloads.is_some());

        let (added_workloads, deleted_workloads) = added_deleted_workloads.unwrap();

        assert_eq!(added_workloads, vec![updated_workload]);

        assert_eq!(
            deleted_workloads,
            vec![DeletedWorkload {
                instance_name: workload_to_update.instance_name.clone(),
                dependencies: HashMap::new(),
            }]
        );

        assert_eq!(server_state.state, new_complete_state);
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

        let update_mask = vec![];

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
            .update(new_complete_state, update_mask)
            .unwrap();
        assert!(added_deleted_workloads.is_some());
    }

    // [utest->swdd~server-updates-resource-availability~1]
    #[test]
    fn utest_server_state_update_agent_resource_availability() {
        let w1 = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let mut server_state = ServerState {
            state: generate_test_complete_state(vec![w1.clone()]),
            ..Default::default()
        };
        server_state.update_agent_resource_availability(AgentLoadStatus {
            agent_name: fixtures::AGENT_NAMES[0].to_string(),
            cpu_usage: fixtures::CPU_USAGE_SPEC,
            free_memory: fixtures::FREE_MEMORY_SPEC,
        });

        let agent_status = server_state
            .state
            .agents
            .agents
            .entry(fixtures::AGENT_NAMES[0].to_string())
            .or_default()
            .to_owned()
            .status
            .unwrap_or_default();

        assert_eq!(agent_status.cpu_usage, Some(fixtures::CPU_USAGE_SPEC));
        assert_eq!(agent_status.free_memory, Some(fixtures::FREE_MEMORY_SPEC));
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

    // [utest->swdd~server-state-stores-agent-in-complete-state~1]
    #[test]
    fn utest_add_agent() {
        let mut server_state = ServerState::default();
        server_state.add_agent(
            fixtures::AGENT_NAMES[0].to_string(),
            generate_test_agent_tags(),
        );
        server_state.update_agent_resource_availability(AgentLoadStatus {
            agent_name: fixtures::AGENT_NAMES[0].to_string(),
            cpu_usage: fixtures::CPU_USAGE_SPEC,
            free_memory: fixtures::FREE_MEMORY_SPEC,
        });

        let expected_agent_map = generate_test_agent_map(fixtures::AGENT_NAMES[0]);

        assert_eq!(server_state.state.agents, expected_agent_map);
    }

    // [utest->swdd~server-state-removes-agent-from-complete-state~1]
    #[test]
    fn utest_remove_agent() {
        let mut server_state = ServerState {
            state: CompleteStateSpec {
                agents: generate_test_agent_map(fixtures::AGENT_NAMES[0]),
                ..Default::default()
            },
            ..Default::default()
        };

        server_state.remove_agent(fixtures::AGENT_NAMES[0]);

        let expected_agent_map = AgentMapSpec::default();
        assert_eq!(server_state.state.agents, expected_agent_map);
    }

    // [utest->swdd~server-state-provides-connected-agent-exists-check~1]
    #[test]
    fn utest_contains_connected_agent() {
        let server_state = ServerState {
            state: CompleteStateSpec {
                agents: generate_test_agent_map(fixtures::AGENT_NAMES[0]),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(server_state.contains_connected_agent(fixtures::AGENT_NAMES[0]));
        assert!(!server_state.contains_connected_agent(fixtures::AGENT_NAMES[1]));
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

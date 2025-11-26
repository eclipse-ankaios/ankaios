// Copyright (c) 2025 Elektrobit Automotive GmbH
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
use crate::ankaios_server::request_id::{AgentName, AgentRequestId, WorkloadName, to_string_id};
#[cfg_attr(test, mockall_double::double)]
use crate::ankaios_server::server_state::ServerState;

use crate::ankaios_server::state_comparator::StateDifferenceTree;

use super::request_id::{CliConnectionName, RequestId};
use common::{
    from_server_interface::{FromServerInterface, FromServerSender},
    state_manipulation::Path,
    std_extensions::IllegalStateResult,
};

use ankaios_api::ank_base::{AgentMapSpec, CompleteStateRequestSpec, WorkloadStatesMapSpec};
use std::collections::HashMap;

use serde_yaml::Value;

#[cfg(test)]
use mockall::automock;

type SubscribedFieldMasks = Vec<Path>;
#[derive(Debug, Default)]
pub struct EventHandler {
    subscriber_store: HashMap<RequestId, SubscribedFieldMasks>,
}

const WILDCARD_SYMBOL: &str = "*";

#[cfg_attr(test, automock)]
impl EventHandler {
    // [impl->swdd~provides-functionality-to-handle-event-subscriptions~1]
    pub fn add_subscriber(&mut self, request_id: String, field_masks: SubscribedFieldMasks) {
        log::debug!("Adding event subscriber '{request_id}' with field masks: {field_masks:?}",);
        self.subscriber_store.insert(request_id.into(), field_masks);
    }

    // [impl->swdd~provides-functionality-to-handle-event-subscriptions~1]
    pub fn remove_subscriber(&mut self, request_id: String) {
        log::debug!("Removing event subscriber '{request_id}'");
        self.subscriber_store.remove(&request_id.into());
    }

    // [impl->swdd~provides-functionality-to-handle-event-subscriptions~1]
    pub fn remove_subscribers_of_agent(&mut self, agent_name: &AgentName) {
        self.subscriber_store.retain(|request_id, _| {
            if let RequestId::AgentRequestId(agent_request_id) = request_id
                && agent_request_id.agent_name == *agent_name
            {
                log::debug!("Removing event subscriber '{request_id}' of agent '{agent_name}'",);
                false
            } else {
                true
            }
        });
    }

    // [impl->swdd~provides-functionality-to-handle-event-subscriptions~1]
    pub fn remove_cli_subscriber(&mut self, cli_connection_name: &CliConnectionName) {
        self.subscriber_store.retain(|request_id, _| {
            if let RequestId::CliRequestId(cli_request_id) = request_id && cli_request_id.cli_name == *cli_connection_name {
                log::debug!(
                    "Removing event subscriber '{request_id}' of CLI connection '{cli_connection_name}'",
                );
                false
            } else {
                true
            }
        });
    }

    // [impl->swdd~provides-functionality-to-handle-event-subscriptions~1]
    pub fn remove_workload_subscriber(
        &mut self,
        agent_name: &AgentName,
        workload_name: &WorkloadName,
    ) {
        self.subscriber_store.retain(|request_id, _| {
            if let RequestId::AgentRequestId(agent_request_id) = request_id
                && request_id_matches_agent_and_workload_name(agent_request_id, agent_name, workload_name)
            {
                log::debug!("Removing event subscriber '{request_id}' of agent '{agent_name}' and workload '{workload_name}'");
                false
            } else {
                true
            }
        });
    }

    // [impl->swdd~provides-functionality-to-handle-event-subscriptions~1]
    pub fn has_subscribers(&self) -> bool {
        !self.subscriber_store.is_empty()
    }

    // [impl->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
    pub async fn send_events(
        &self,
        server_state: &ServerState,
        workload_states_map: &WorkloadStatesMapSpec,
        agent_map: &AgentMapSpec,
        field_difference_tree: StateDifferenceTree,
        from_server_channel: &FromServerSender,
    ) {
        let added_tree = field_difference_tree.added_tree.into();
        let removed_tree = field_difference_tree.removed_tree.into();
        let updated_tree = field_difference_tree.updated_tree.into();
        for (request_id, subscribed_field_masks) in &self.subscriber_store {
            // [impl->swdd~event-handler-calculates-altered-field-masks-matching-subscribers-field-masks~1]
            let altered_fields = AlteredFields {
                added_fields: collect_altered_fields_matching_subscriber_masks(
                    &added_tree,
                    subscribed_field_masks,
                ),
                removed_fields: collect_altered_fields_matching_subscriber_masks(
                    &removed_tree,
                    subscribed_field_masks,
                ),
                updated_fields: collect_altered_fields_matching_subscriber_masks(
                    &updated_tree,
                    subscribed_field_masks,
                ),
            };

            let mut filter_masks = altered_fields.added_fields.clone();
            filter_masks.extend(altered_fields.removed_fields.clone());
            filter_masks.extend(altered_fields.updated_fields.clone());

            if !altered_fields.all_empty() {
                {
                    let complete_state_differences = server_state
                        .get_complete_state_by_field_mask(
                            CompleteStateRequestSpec {
                                field_mask: filter_masks,
                                subscribe_for_events: false,
                            },
                            workload_states_map,
                            agent_map,
                        )
                        .unwrap_or_illegal_state();

                    log::debug!(
                        "Sending event to subscriber '{request_id}' with altered fields: {altered_fields:?} and complete state differences: {complete_state_differences:?}",
                    );

                    let request_id = to_string_id(request_id);
                    from_server_channel
                        .complete_state(
                            request_id,
                            complete_state_differences,
                            Some(altered_fields.into()),
                        )
                        .await
                        .unwrap_or_illegal_state();
                }
            }
        }
    }
}

// [impl->swdd~event-handler-calculates-altered-field-masks-matching-subscribers-field-masks~1]
fn collect_altered_fields_matching_subscriber_masks(
    tree: &serde_yaml::Mapping,
    subscriber_field_masks: &[Path],
) -> Vec<String> {
    let mut altered_fields = Vec::new();
    for subscriber_mask in subscriber_field_masks {
        let mut stack_task = vec![(tree, subscriber_mask.parts().as_slice(), String::new())];

        while let Some((current_node, remaining_parts, current_path)) = stack_task.pop() {
            if remaining_parts.is_empty() {
                // We've reached the end of the subscriber mask; collect all leaf paths from here
                let leaf_paths =
                    collect_all_leaf_paths_iterative(&Value::Mapping(current_node.clone()));
                for leaf_path in leaf_paths {
                    let full_path = update_path_with_new_key(&current_path, &leaf_path);
                    altered_fields.push(full_path);
                }
            } else {
                let next_part = &remaining_parts[0];
                if next_part == WILDCARD_SYMBOL {
                    // Wildcard: traverse all children
                    for (key, child_node) in current_node {
                        let Value::String(key_str) = key else {
                            continue; // the difference tree only contains string keys
                        };

                        let new_path = update_path_with_new_key(&current_path, key_str);
                        if let Value::Mapping(child_map) = child_node {
                            stack_task.push((child_map, &remaining_parts[1..], new_path));
                        } else if let Value::Null = child_node
                            && remaining_parts[1..].is_empty()
                        {
                            // Treat Null as a leaf node
                            altered_fields.push(new_path);
                        }
                    }
                } else {
                    // Specific part: traverse that child if it exists
                    if let Some(child_node) = current_node.get(Value::String(next_part.clone())) {
                        let new_path = update_path_with_new_key(&current_path, next_part);
                        if let Value::Mapping(child_map) = child_node {
                            stack_task.push((child_map, &remaining_parts[1..], new_path));
                        } else if let Value::Null = child_node
                            && remaining_parts[1..].is_empty()
                        {
                            // Treat Null as a leaf node
                            altered_fields.push(new_path);
                        }
                    }
                }
            }
        }
    }

    altered_fields
}

// [impl->swdd~event-handler-calculates-altered-field-masks-matching-subscribers-field-masks~1]
pub fn collect_all_leaf_paths_iterative(start_node: &Value) -> Vec<String> {
    let node = start_node;
    let mut results = Vec::new();
    let mut stack = vec![(node, String::new())];
    while let Some((current, current_path)) = stack.pop() {
        match current {
            Value::Mapping(map) if !map.is_empty() => {
                for (current_key, current_value) in map {
                    let Value::String(new_key) = current_key else {
                        continue; // the difference tree only contains string keys
                    };
                    let new_path = update_path_with_new_key(&current_path, new_key);
                    stack.push((current_value, new_path));
                }
            }
            // Any non-mapping or empty mapping is treated as a leaf node
            _ => {
                results.push(current_path);
            }
        }
    }
    results
}

// [impl->swdd~event-handler-calculates-altered-field-masks-matching-subscribers-field-masks~1]
fn update_path_with_new_key(current_path: &str, new_key: &str) -> String {
    if current_path.is_empty() {
        new_key.to_owned()
    } else {
        format!("{current_path}.{new_key}")
    }
}

fn request_id_matches_agent_and_workload_name(
    agent_request_id: &AgentRequestId,
    agent_name: &AgentName,
    workload_name: &WorkloadName,
) -> bool {
    agent_request_id.agent_name == *agent_name && agent_request_id.workload_name == *workload_name
}

#[derive(Debug, Default)]
pub struct AlteredFields {
    pub added_fields: Vec<String>,
    pub removed_fields: Vec<String>,
    pub updated_fields: Vec<String>,
}

impl AlteredFields {
    pub fn all_empty(&self) -> bool {
        self.added_fields.is_empty()
            && self.removed_fields.is_empty()
            && self.updated_fields.is_empty()
    }
}

impl From<AlteredFields> for ankaios_api::ank_base::AlteredFields {
    fn from(altered_fields: AlteredFields) -> Self {
        ankaios_api::ank_base::AlteredFields {
            added_fields: altered_fields.added_fields,
            removed_fields: altered_fields.removed_fields,
            updated_fields: altered_fields.updated_fields,
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

    use common::from_server_interface::FromServer;

    use super::StateDifferenceTree;

    use ankaios_api::ank_base::{
        AgentMapSpec, CompleteStateRequestSpec, CompleteStateSpec, StateSpec, WorkloadMapSpec,
        WorkloadSpec, WorkloadStatesMapSpec, response::ResponseContent,
    };
    use ankaios_api::test_utils::generate_test_workload_with_param;
    use mockall::predicate;

    use super::EventHandler;
    use crate::ankaios_server::{
        create_from_server_channel,
        event_handler::collect_altered_fields_matching_subscriber_masks,
        server_state::MockServerState,
    };

    const AGENT_A_REQUEST_ID_1: &str = "agent_A@workload_1@1234";
    const AGENT_B_REQUEST_ID_1: &str = "agent_B@workload_2@5678";
    const AGENT_A_REQUEST_ID_2: &str = "agent_A@workload_3@9876";
    const CLI_CONN_1_REQUEST_ID_1: &str = "cli-conn-1@cli_request_id_1";
    const CLI_CONN_1_REQUEST_ID_2: &str = "cli-conn-1@cli_request_id_2";
    const CLI_CONN_2_REQUEST_ID_1: &str = "cli-conn-2@cli_request_id_2";

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_add_subscriber() {
        let mut event_handler = EventHandler::default();
        let subscriber_1_field_masks = vec!["path.to.field".into()];
        event_handler.add_subscriber(
            AGENT_A_REQUEST_ID_1.to_owned(),
            subscriber_1_field_masks.clone(),
        );
        assert!(event_handler.has_subscribers());
        assert_eq!(event_handler.subscriber_store.len(), 1);
        assert_eq!(
            event_handler
                .subscriber_store
                .get(&AGENT_A_REQUEST_ID_1.into()),
            Some(&subscriber_1_field_masks)
        );
        let subscriber_2_field_masks = vec!["another.path".into(), "more.paths".into()];
        event_handler.add_subscriber(
            CLI_CONN_1_REQUEST_ID_1.to_owned(),
            subscriber_2_field_masks.clone(),
        );
        assert_eq!(event_handler.subscriber_store.len(), 2);
        assert_eq!(
            event_handler
                .subscriber_store
                .get(&CLI_CONN_1_REQUEST_ID_1.into()),
            Some(&subscriber_2_field_masks)
        );
    }

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_remove_subscriber() {
        let mut event_handler = EventHandler {
            subscriber_store: HashMap::from([
                (AGENT_A_REQUEST_ID_1.into(), vec!["path.to.field".into()]),
                (
                    AGENT_B_REQUEST_ID_1.into(),
                    vec!["another.path".into(), "more.paths".into()],
                ),
            ]),
        };

        event_handler.remove_subscriber(AGENT_A_REQUEST_ID_1.to_owned());
        assert_eq!(event_handler.subscriber_store.len(), 1);
        assert!(
            !event_handler
                .subscriber_store
                .contains_key(&AGENT_A_REQUEST_ID_1.into())
        );
        assert!(
            event_handler
                .subscriber_store
                .contains_key(&AGENT_B_REQUEST_ID_1.into())
        );

        event_handler.remove_subscriber(AGENT_B_REQUEST_ID_1.to_owned());
        assert!(!event_handler.has_subscribers());
        assert_eq!(event_handler.subscriber_store.len(), 0);
    }

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_removes_subscribers_of_agent() {
        let mut event_handler = EventHandler {
            subscriber_store: HashMap::from([
                (AGENT_A_REQUEST_ID_1.into(), vec!["path.to.field".into()]),
                (AGENT_B_REQUEST_ID_1.into(), vec!["more.paths".into()]),
                (AGENT_A_REQUEST_ID_2.into(), vec!["another.path".into()]),
                (CLI_CONN_1_REQUEST_ID_1.into(), vec!["cli.path".into()]),
            ]),
        };

        event_handler.remove_subscribers_of_agent(&"agent_A".to_owned());
        assert_eq!(event_handler.subscriber_store.len(), 2);
        assert!(
            event_handler
                .subscriber_store
                .contains_key(&AGENT_B_REQUEST_ID_1.into())
        );

        assert!(
            event_handler
                .subscriber_store
                .contains_key(&CLI_CONN_1_REQUEST_ID_1.into())
        );
    }

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_removes_cli_subscriber() {
        let mut event_handler = EventHandler {
            subscriber_store: HashMap::from([
                (CLI_CONN_1_REQUEST_ID_1.into(), vec!["cli.path".into()]),
                (
                    CLI_CONN_1_REQUEST_ID_2.into(),
                    vec!["another.cli.path".into()],
                ),
                (
                    CLI_CONN_2_REQUEST_ID_1.into(),
                    vec!["different.cli.path".into()],
                ),
                (AGENT_A_REQUEST_ID_1.into(), vec!["another.path".into()]),
            ]),
        };

        event_handler.remove_cli_subscriber(&"cli-conn-1".to_owned());
        assert_eq!(event_handler.subscriber_store.len(), 2);
        assert!(
            event_handler
                .subscriber_store
                .contains_key(&CLI_CONN_2_REQUEST_ID_1.into())
        );
        assert!(
            event_handler
                .subscriber_store
                .contains_key(&AGENT_A_REQUEST_ID_1.into())
        );
    }

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_remove_workload_subscribers() {
        let agent_a_request_id_3 = "agent_B@workload_1@19234";
        let mut event_handler = EventHandler {
            subscriber_store: HashMap::from([
                (CLI_CONN_1_REQUEST_ID_1.into(), vec!["cli.path".into()]),
                (AGENT_A_REQUEST_ID_1.into(), vec!["another.path".into()]),
                (agent_a_request_id_3.into(), vec!["more.paths".into()]),
                (AGENT_B_REQUEST_ID_1.into(), vec!["different.path".into()]),
            ]),
        };

        event_handler.remove_workload_subscriber(&"agent_A".to_owned(), &"workload_1".to_owned());
        assert_eq!(event_handler.subscriber_store.len(), 3);
        assert!(
            !event_handler
                .subscriber_store
                .contains_key(&AGENT_A_REQUEST_ID_1.into())
        );
        assert!(
            event_handler
                .subscriber_store
                .contains_key(&agent_a_request_id_3.into())
        );
    }

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_has_subscribers() {
        let mut event_handler = EventHandler {
            subscriber_store: HashMap::from([(
                AGENT_A_REQUEST_ID_1.into(),
                vec!["path.to.field".into()],
            )]),
        };

        assert!(event_handler.has_subscribers());

        event_handler.subscriber_store.clear();
        assert!(!event_handler.has_subscribers());
    }

    // [utest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
    // [utest->swdd~event-handler-calculates-altered-field-masks-matching-subscribers-field-masks~1]
    #[tokio::test]
    async fn utest_event_handler_send_events() {
        let _ = env_logger::builder().is_test(true).try_init();
        let added_field_mask = "desiredState.workloads.workload_2";
        let updated_field_mask = "desiredState.workloads.workload_1.agent";
        let removed_field_mask = "configs.*";
        let expected_removed_field_mask = "configs.some_config";
        let added_workload: WorkloadSpec =
            generate_test_workload_with_param("agent_A", "runtime_1");
        let mut mock_server_state = MockServerState::default();
        mock_server_state
            .expect_get_complete_state_by_field_mask()
            .once()
            .with(
                mockall::predicate::function(|request_complete_state| {
                    request_complete_state
                        == &CompleteStateRequestSpec {
                            field_mask: vec![
                                added_field_mask.to_owned(),
                                expected_removed_field_mask.to_owned(),
                                updated_field_mask.to_owned(),
                            ],
                            subscribe_for_events: false,
                        }
                }),
                predicate::always(),
                predicate::always(),
            )
            .return_const(Ok(CompleteStateSpec {
                desired_state: StateSpec {
                    workloads: WorkloadMapSpec {
                        workloads: [
                            (
                                "workload_1".to_string(),
                                WorkloadSpec {
                                    agent: "agent_1".to_string(),
                                    ..Default::default()
                                },
                            ),
                            ("workload_2".to_string(), added_workload.clone()),
                        ]
                        .into(),
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
            .into()));
        let workload_states_map = WorkloadStatesMapSpec::default();
        let agent_map = AgentMapSpec::default();

        let mut state_difference_tree = StateDifferenceTree::new();
        state_difference_tree.insert_added_path(vec![
            "desiredState".to_owned(),
            "workloads".to_owned(),
            "workload_2".to_owned(),
        ]);

        state_difference_tree.insert_updated_path(vec![
            "desiredState".to_owned(),
            "workloads".to_owned(),
            "workload_1".to_owned(),
            "agent".to_owned(),
        ]);

        state_difference_tree
            .insert_removed_path(vec!["configs".to_owned(), "some_config".to_owned()]);

        let (to_agents, mut agents_receiver) = create_from_server_channel(1);

        let mut event_handler = EventHandler::default();
        event_handler.subscriber_store.insert(
            AGENT_A_REQUEST_ID_1.into(),
            vec![
                added_field_mask.into(),
                updated_field_mask.into(),
                removed_field_mask.into(),
            ],
        );

        event_handler
            .send_events(
                &mock_server_state,
                &workload_states_map,
                &agent_map,
                state_difference_tree,
                &to_agents,
            )
            .await;

        let received_message = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            agents_receiver.recv(),
        )
        .await;
        assert!(received_message.is_ok());
        let received_message = received_message.unwrap();
        assert!(received_message.is_some());
        let received_message = received_message.unwrap();
        let FromServer::Response(response) = received_message else {
            panic!("Expected FromServer::Response message");
        };

        assert_eq!(response.request_id, AGENT_A_REQUEST_ID_1.to_owned());
        let ResponseContent::CompleteStateResponse(complete_state_response) =
            response.response_content.unwrap()
        else {
            panic!("Expected CompleteStateResponse");
        };

        let complete_state = complete_state_response.complete_state.unwrap();
        let altered_fields = complete_state_response.altered_fields.unwrap();

        let expected_complete_state: ankaios_api::ank_base::CompleteState = CompleteStateSpec {
            desired_state: StateSpec {
                workloads: WorkloadMapSpec {
                    workloads: [
                        (
                            "workload_1".to_owned(),
                            WorkloadSpec {
                                agent: "agent_1".to_owned(),
                                ..Default::default()
                            },
                        ),
                        ("workload_2".to_owned(), added_workload),
                    ]
                    .into(),
                },
                ..Default::default()
            },
            ..Default::default()
        }
        .into();

        let expected_altered_fields = ankaios_api::ank_base::AlteredFields {
            added_fields: vec![added_field_mask.to_owned()],
            updated_fields: vec![updated_field_mask.to_owned()],
            removed_fields: vec![expected_removed_field_mask.to_owned()],
        };

        assert_eq!(complete_state, expected_complete_state);
        assert_eq!(altered_fields, expected_altered_fields);
    }

    // [utest->swdd~event-handler-calculates-altered-field-masks-matching-subscribers-field-masks~1]
    #[test]
    fn utest_collect_altered_fields_matching_subscriber_masks_with_masks_matching_sub_tree_or_exact_field_path()
     {
        let yaml_data = r#"
        root:
            child1:
                grandchild1: null
                grandchild2:
                    great_grandchild: null
            child2: null
        "#;

        let parsed_yaml: serde_yaml::Value = serde_yaml::from_str(yaml_data).unwrap();
        let serde_yaml::Value::Mapping(tree) = parsed_yaml else {
            panic!("Expected YAML mapping at root");
        };

        // case 1: all sub paths from root
        let altered_fields = collect_altered_fields_matching_subscriber_masks(&tree, &["".into()]);
        assert_eq!(altered_fields.len(), 3);
        assert!(altered_fields.contains(&"root.child1.grandchild1".to_owned()));
        assert!(altered_fields.contains(&"root.child1.grandchild2.great_grandchild".to_owned()));
        assert!(altered_fields.contains(&"root.child2".to_owned()));

        // case 2: all sub paths from a child
        let altered_fields =
            collect_altered_fields_matching_subscriber_masks(&tree, &["root.child1".into()]);
        assert_eq!(altered_fields.len(), 2);
        assert!(altered_fields.contains(&"root.child1.grandchild1".to_owned()));
        assert!(altered_fields.contains(&"root.child1.grandchild2.great_grandchild".to_owned()));
    }

    #[test]
    fn utest_collect_altered_fields_matching_subscriber_masks_with_wildcard_mask_matching_sub_tree()
    {
        let yaml_data = r#"
        root:
            child1:
                grandchild1: null
                grandchild2:
                    great_grandchild: null
            child2: null
        "#;

        let parsed_yaml: serde_yaml::Value = serde_yaml::from_str(yaml_data).unwrap();
        let serde_yaml::Value::Mapping(tree) = parsed_yaml else {
            panic!("Expected YAML mapping at root");
        };

        let altered_fields = collect_altered_fields_matching_subscriber_masks(&tree, &["*".into()]);
        assert_eq!(altered_fields.len(), 3);
        assert!(altered_fields.contains(&"root.child1.grandchild1".to_owned()));
        assert!(altered_fields.contains(&"root.child1.grandchild2.great_grandchild".to_owned()));
        assert!(altered_fields.contains(&"root.child2".to_owned()));
    }

    #[test]
    fn utest_collect_altered_fields_matching_subscriber_masks_with_not_existing_field() {
        let yaml_data = r#"
        root:
            child1:
                grandchild1: null
        "#;

        let parsed_yaml: serde_yaml::Value = serde_yaml::from_str(yaml_data).unwrap();
        let serde_yaml::Value::Mapping(tree) = parsed_yaml else {
            panic!("Expected YAML mapping at root");
        };

        let altered_fields = collect_altered_fields_matching_subscriber_masks(
            &tree,
            &["root.child1.grandchild1.non_existing".into()],
        );
        assert!(altered_fields.is_empty());
    }
}

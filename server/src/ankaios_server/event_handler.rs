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
use crate::ankaios_server::request_id::to_string_id;
#[cfg_attr(test, mockall_double::double)]
use crate::ankaios_server::server_state::ServerState;

use crate::ankaios_server::state_comparator::StateDifferenceTree;

use super::request_id::RequestId;
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
        log::debug!("Adding subscriber '{request_id}' with field masks: {field_masks:?}",);
        self.subscriber_store.insert(request_id.into(), field_masks);
    }

    // [impl->swdd~provides-functionality-to-handle-event-subscriptions~1]
    pub fn remove_subscriber(&mut self, request_id: String) {
        log::debug!("Removing subscriber '{request_id}'");
        self.subscriber_store.remove(&request_id.into());
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
        let added_first_difference_tree = field_difference_tree
            .added_tree
            .first_difference_tree
            .try_into()
            .unwrap_or_default();
        let added_full_difference_tree = field_difference_tree
            .added_tree
            .full_difference_tree
            .try_into()
            .unwrap_or_default();
        let removed_first_difference_tree = field_difference_tree
            .removed_tree
            .first_difference_tree
            .try_into()
            .unwrap_or_default();
        let removed_full_difference_tree = field_difference_tree
            .removed_tree
            .full_difference_tree
            .try_into()
            .unwrap_or_default();

        let updated_full_difference_tree = field_difference_tree
            .updated_tree
            .full_difference_tree
            .try_into()
            .unwrap_or_default();

        for (request_id, subscribed_field_masks) in &self.subscriber_store {
            // [impl->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
            // [impl->swdd~event-handler-creates-altered-fields-using-full-difference-tree~1]
            let added_altered_fields = create_altered_fields_matching_subscriber_masks(
                &added_full_difference_tree,
                &added_first_difference_tree,
                subscribed_field_masks,
            );

            // [impl->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
            // [impl->swdd~event-handler-creates-altered-fields-using-full-difference-tree~1]
            let removed_altered_fields = create_altered_fields_matching_subscriber_masks(
                &removed_full_difference_tree,
                &removed_first_difference_tree,
                subscribed_field_masks,
            );

            // [impl->swdd~event-handler-creates-altered-fields-using-full-difference-tree-for-updated-fields~1]
            let updated_altered_fields = create_altered_fields_matching_subscriber_masks(
                &updated_full_difference_tree,
                &updated_full_difference_tree,
                subscribed_field_masks,
            );

            let altered_fields = AlteredFields {
                added_fields: added_altered_fields,
                removed_fields: removed_altered_fields,
                updated_fields: updated_altered_fields,
            };

            let mut filter_masks = altered_fields.added_fields.clone();
            filter_masks.extend(altered_fields.removed_fields.clone());
            filter_masks.extend(altered_fields.updated_fields.clone());

            if !altered_fields.all_empty() {
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

// [impl->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
// [impl->swdd~event-handler-creates-altered-fields-using-full-difference-tree~1]
// [impl->swdd~event-handler-creates-altered-fields-using-full-difference-tree-for-updated-fields~1]
fn create_altered_fields_matching_subscriber_masks(
    extended_difference_tree: &serde_yaml::Mapping,
    difference_tree: &serde_yaml::Mapping,
    subscriber_field_masks: &[Path],
) -> Vec<String> {
    let mut altered_field_masks = Vec::new();
    let empty_mapping = serde_yaml::Mapping::new();
    for subscriber_mask in subscriber_field_masks {
        let mut stack_task = vec![(
            difference_tree,
            extended_difference_tree,
            subscriber_mask.parts().as_slice(),
            String::new(),
        )];

        while let Some((current_node, current_extended_node, subscriber_mask_parts, current_path)) =
            stack_task.pop()
        {
            if subscriber_mask_parts.is_empty() {
                // We've reached the end of the subscriber mask; collect all leaf paths from here
                let paths_to_first_difference =
                    collect_all_leaf_paths_iterative(&Value::Mapping(current_node.clone()));
                for path in paths_to_first_difference {
                    let first_difference_path = update_path_with_new_key(&current_path, &path);
                    altered_field_masks.push(first_difference_path);
                }
            } else {
                let next_part = &subscriber_mask_parts[0];
                if next_part == WILDCARD_SYMBOL {
                    // Wildcard: traverse all children
                    for (next_extended_key, next_extended_tree_level) in current_extended_node {
                        let Value::String(converted_key) = next_extended_key else {
                            continue; // the difference tree only contains string keys
                        };

                        let new_tree_path = update_path_with_new_key(&current_path, converted_key);
                        match (
                            current_node
                                .get(next_extended_key)
                                .unwrap_or(&serde_yaml::Value::Null),
                            next_extended_tree_level,
                        ) {
                            (Value::Mapping(next_node), Value::Mapping(next_extended_node)) => {
                                stack_task.push((
                                    next_node,
                                    next_extended_node,
                                    &subscriber_mask_parts[1..],
                                    new_tree_path,
                                ));
                            }
                            (Value::Null, _) if subscriber_mask_parts[1..].is_empty() => {
                                // Treat Null as a leaf node
                                altered_field_masks.push(new_tree_path);
                            }
                            (Value::Null, Value::Mapping(next_extended_tree_node))
                                if !subscriber_mask_parts[1..].is_empty() =>
                            {
                                stack_task.push((
                                    &empty_mapping,
                                    next_extended_tree_node,
                                    &subscriber_mask_parts[1..],
                                    new_tree_path,
                                ));
                            }
                            _ => {}
                        }
                    }
                } else {
                    // Specific part: traverse that child if it exists
                    let next_extended_tree_level =
                        current_extended_node.get(Value::String(next_part.clone()));
                    if let Some(next_tree_node) = current_node.get(Value::String(next_part.clone()))
                        && let Some(next_extended_tree_node) = next_extended_tree_level
                    {
                        let new_tree_path = update_path_with_new_key(&current_path, next_part);
                        match (next_tree_node, next_extended_tree_node) {
                            (Value::Mapping(next_node), Value::Mapping(next_extended_node)) => {
                                stack_task.push((
                                    next_node,
                                    next_extended_node,
                                    &subscriber_mask_parts[1..],
                                    new_tree_path,
                                ));
                            }
                            (Value::Null, _) if subscriber_mask_parts[1..].is_empty() => {
                                // Treat Null as a leaf node
                                altered_field_masks.push(new_tree_path);
                            }
                            (Value::Null, Value::Mapping(new_extended_tree_node))
                                if !subscriber_mask_parts[1..].is_empty() =>
                            {
                                stack_task.push((
                                    &empty_mapping,
                                    new_extended_tree_node,
                                    &subscriber_mask_parts[1..],
                                    new_tree_path,
                                ));
                            }
                            _ => {}
                        }
                    } else if let Some(next_extended_tree_node) = next_extended_tree_level {
                        // The path exists in the full tree but not in the first level tree
                        let new_tree_path = update_path_with_new_key(&current_path, next_part);
                        match next_extended_tree_node {
                            Value::Mapping(next_extended_node) => {
                                stack_task.push((
                                    &empty_mapping,
                                    next_extended_node,
                                    &subscriber_mask_parts[1..],
                                    new_tree_path,
                                ));
                            }
                            Value::Null if subscriber_mask_parts[1..].is_empty() => {
                                // Treat Null as a leaf node
                                altered_field_masks.push(new_tree_path);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    altered_field_masks
}

// [impl->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
// [impl->swdd~event-handler-creates-altered-fields-using-full-difference-tree~1]
pub fn collect_all_leaf_paths_iterative(start_node: &Value) -> Vec<String> {
    let mut tree_paths = Vec::new();
    let mut stack = vec![(start_node, String::new())];
    while let Some((current_tree_value, current_tree_path)) = stack.pop() {
        match current_tree_value {
            Value::Mapping(current_node) if !current_node.is_empty() => {
                for (next_key, next_tree_value) in current_node {
                    let Value::String(converted_next_key) = next_key else {
                        continue; // the difference tree only contains string keys
                    };
                    let next_tree_path =
                        update_path_with_new_key(&current_tree_path, converted_next_key);
                    stack.push((next_tree_value, next_tree_path));
                }
            }
            // Any non-mapping or empty mapping is treated as a leaf node
            _ => {
                tree_paths.push(current_tree_path);
            }
        }
    }
    tree_paths
}

fn update_path_with_new_key(current_path: &str, new_key: &str) -> String {
    if current_path.is_empty() {
        new_key.to_owned()
    } else if new_key.is_empty() {
        current_path.to_owned()
    } else {
        format!("{current_path}.{new_key}")
    }
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
    use crate::ankaios_server::state_comparator::generate_difference_tree_from_paths;
    use crate::ankaios_server::{
        create_from_server_channel, event_handler::create_altered_fields_matching_subscriber_masks,
        server_state::MockServerState,
    };

    const REQUEST_ID_1: &str = "agent_A@workload_1@1234";
    const REQUEST_ID_2: &str = "agent_B@workload_2@5678";

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_add_subscriber() {
        let mut event_handler = EventHandler::default();
        let subscriber_1_field_masks = vec!["path.to.field".into()];
        event_handler.add_subscriber(REQUEST_ID_1.to_owned(), subscriber_1_field_masks.clone());
        assert!(event_handler.has_subscribers());
        assert_eq!(event_handler.subscriber_store.len(), 1);
        assert_eq!(
            event_handler.subscriber_store.get(&REQUEST_ID_1.into()),
            Some(&subscriber_1_field_masks)
        );
        let subscriber_2_field_masks = vec!["another.path".into(), "more.paths".into()];
        event_handler.add_subscriber(REQUEST_ID_2.to_owned(), subscriber_2_field_masks.clone());
        assert_eq!(event_handler.subscriber_store.len(), 2);
        assert_eq!(
            event_handler.subscriber_store.get(&REQUEST_ID_2.into()),
            Some(&subscriber_2_field_masks)
        );
    }

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_remove_subscriber() {
        let mut event_handler = EventHandler {
            subscriber_store: HashMap::from([
                (REQUEST_ID_1.into(), vec!["path.to.field".into()]),
                (
                    REQUEST_ID_2.into(),
                    vec!["another.path".into(), "more.paths".into()],
                ),
            ]),
        };

        event_handler.remove_subscriber(REQUEST_ID_1.to_owned());
        assert_eq!(event_handler.subscriber_store.len(), 1);
        assert!(
            !event_handler
                .subscriber_store
                .contains_key(&REQUEST_ID_1.into())
        );
        assert!(
            event_handler
                .subscriber_store
                .contains_key(&REQUEST_ID_2.into())
        );

        event_handler.remove_subscriber(REQUEST_ID_2.to_owned());
        assert!(!event_handler.has_subscribers());
        assert_eq!(event_handler.subscriber_store.len(), 0);
    }

    // [utest->swdd~provides-functionality-to-handle-event-subscriptions~1]
    #[test]
    fn utest_event_handler_has_subscribers() {
        let mut event_handler = EventHandler {
            subscriber_store: HashMap::from([(REQUEST_ID_1.into(), vec!["path.to.field".into()])]),
        };

        assert!(event_handler.has_subscribers());

        event_handler.subscriber_store.clear();
        assert!(!event_handler.has_subscribers());
    }

    // [utest->swdd~event-handler-sends-complete-state-differences-including-altered-fields~1]
    // [utest->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
    // [utest->swdd~event-handler-creates-altered-fields-using-full-difference-tree~1]
    // [utest->swdd~event-handler-creates-altered-fields-using-full-difference-tree-for-updated-fields~1]
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
        state_difference_tree.added_tree.first_difference_tree =
            generate_difference_tree_from_paths(&[vec![
                "desiredState".to_owned(),
                "workloads".to_owned(),
                "workload_2".to_owned(),
            ]]);

        state_difference_tree.added_tree.full_difference_tree =
            generate_difference_tree_from_paths(&[vec![
                "desiredState".to_owned(),
                "workloads".to_owned(),
                "workload_2".to_owned(),
            ]]);

        state_difference_tree.insert_updated_path(vec![
            "desiredState".to_owned(),
            "workloads".to_owned(),
            "workload_1".to_owned(),
            "agent".to_owned(),
        ]);

        state_difference_tree.removed_tree.first_difference_tree =
            generate_difference_tree_from_paths(&[vec![
                "configs".to_owned(),
                "some_config".to_owned(),
            ]]);

        state_difference_tree.removed_tree.full_difference_tree =
            generate_difference_tree_from_paths(&[vec![
                "configs".to_owned(),
                "some_config".to_owned(),
                "deeper_config_item".to_owned(),
            ]]);

        let (to_agents, mut agents_receiver) = create_from_server_channel(1);

        let mut event_handler = EventHandler::default();
        event_handler.subscriber_store.insert(
            REQUEST_ID_1.into(),
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

        assert_eq!(response.request_id, REQUEST_ID_1.to_owned());
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

    // [utest->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
    #[test]
    fn utest_collect_altered_fields_matching_subscriber_masks_with_shorter_mask_than_first_difference()
     {
        let first_difference_tree_yaml = r#"
        root:
            child1:
                grandchild1: null
                grandchild2: null
            child2: null
        "#;

        let full_difference_tree_yaml = r#"
        root:
            child1:
                grandchild1: null
                grandchild2:
                    great_grandchild: null
            child2: null
        "#;

        let first_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(first_difference_tree_yaml).unwrap();

        let full_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(full_difference_tree_yaml).unwrap();

        // case 1: without wildcard
        let altered_fields = create_altered_fields_matching_subscriber_masks(
            &full_difference_tree,
            &first_difference_tree,
            &["root".into()],
        );
        assert_eq!(altered_fields.len(), 3);
        assert!(altered_fields.contains(&"root.child1.grandchild1".to_owned()));
        assert!(altered_fields.contains(&"root.child1.grandchild2".to_owned()));
        assert!(altered_fields.contains(&"root.child2".to_owned()));

        // case 2: with wildcard
        let altered_fields = create_altered_fields_matching_subscriber_masks(
            &full_difference_tree,
            &first_difference_tree,
            &["root.*".into()],
        );
        assert_eq!(altered_fields.len(), 3);
        assert!(altered_fields.contains(&"root.child1.grandchild1".to_owned()));
        assert!(altered_fields.contains(&"root.child1.grandchild2".to_owned()));
        assert!(altered_fields.contains(&"root.child2".to_owned()));
    }

    // [utest->swdd~event-handler-creates-altered-fields-using-full-difference-tree~1]
    #[test]
    fn utest_collect_altered_fields_matching_subscriber_masks_with_longer_masks_than_first_difference()
     {
        let first_difference_tree_yaml = r#"
        root:
            child1:
                grandchild1: null
                grandchild2: null
            child2: null
        "#;

        let full_difference_tree_yaml = r#"
        root:
            child1:
                grandchild1: null
                grandchild2:
                    great_grandchild: null
            child2: null
        "#;

        let first_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(first_difference_tree_yaml).unwrap();

        let full_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(full_difference_tree_yaml).unwrap();

        // case 1: without wildcard
        let altered_fields = create_altered_fields_matching_subscriber_masks(
            &full_difference_tree,
            &first_difference_tree,
            &["root.child1.grandchild2.great_grandchild".into()],
        );
        assert_eq!(altered_fields.len(), 1);
        assert!(altered_fields.contains(&"root.child1.grandchild2.great_grandchild".to_owned()));

        // case 2: with wildcard
        let altered_fields = create_altered_fields_matching_subscriber_masks(
            &full_difference_tree,
            &first_difference_tree,
            &["root.*.*.great_grandchild".into()],
        );
        assert_eq!(altered_fields.len(), 1);
        assert!(altered_fields.contains(&"root.child1.grandchild2.great_grandchild".to_owned()));
    }

    // [utest->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
    #[test]
    fn utest_collect_altered_fields_matching_subscriber_masks_with_exact_field_mask() {
        let first_difference_tree_yaml = r#"
        root:
            child1:
                grandchild1: null
                grandchild2: null
            child2: null
        "#;

        let full_difference_tree_yaml = r#"
        root:
            child1:
                grandchild1: null
                grandchild2:
                    great_grandchild: null
            child2: null
        "#;

        let first_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(first_difference_tree_yaml).unwrap();

        let full_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(full_difference_tree_yaml).unwrap();

        // case 1: without wildcard
        let altered_fields = create_altered_fields_matching_subscriber_masks(
            &full_difference_tree,
            &first_difference_tree,
            &["root.child1.grandchild1".into(), "root.child2".into()],
        );
        assert_eq!(altered_fields.len(), 2);
        assert!(altered_fields.contains(&"root.child1.grandchild1".to_owned()));
        assert!(altered_fields.contains(&"root.child2".to_owned()));

        // case 2: with wildcard
        let altered_fields = create_altered_fields_matching_subscriber_masks(
            &full_difference_tree,
            &first_difference_tree,
            &["root.*.*".into()],
        );
        assert_eq!(altered_fields.len(), 2);
        assert!(altered_fields.contains(&"root.child1.grandchild1".to_owned()));
        assert!(altered_fields.contains(&"root.child1.grandchild2".to_owned()));
    }

    // [utest->swdd~event-handler-creates-altered-fields-using-first-difference-tree~1]
    // [utest->swdd~event-handler-creates-altered-fields-using-full-difference-tree~1]
    #[test]
    fn utest_collect_altered_fields_matching_subscriber_masks_with_not_existing_field() {
        let first_difference_tree_yaml = r#"
        root:
            child1: null
        "#;

        let full_difference_tree_yaml = r#"
        root:
            child1:
                grandchild1: null
        "#;

        let first_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(first_difference_tree_yaml).unwrap();

        let full_difference_tree: serde_yaml::Mapping =
            serde_yaml::from_str(full_difference_tree_yaml).unwrap();

        let altered_fields = create_altered_fields_matching_subscriber_masks(
            &full_difference_tree,
            &first_difference_tree,
            &["root.child1.grandchild1.non_existing".into()],
        );
        assert!(altered_fields.is_empty());
    }
}

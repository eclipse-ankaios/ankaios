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
    objects::{AgentMap, WorkloadStatesMap},
    state_manipulation::{Object, Path},
    std_extensions::IllegalStateResult,
};
use std::collections::HashMap;

use serde_yaml::Value;

#[cfg(test)]
use mockall::automock;

type SubscribedFieldMasks = Vec<Path>;
#[derive(Debug, Default)]
pub struct EventHandler {
    subscriber_store: HashMap<RequestId, SubscribedFieldMasks>,
}

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
        workload_states_map: &WorkloadStatesMap,
        agent_map: &AgentMap,
        field_difference_tree: StateDifferenceTree,
        from_server_channel: &FromServerSender,
    ) {
        for (request_id, subscribed_field_masks) in &self.subscriber_store {
            let altered_fields = get_altered_fields(&field_difference_tree, subscribed_field_masks);
            let mut filter_masks = altered_fields.added_fields.clone();
            filter_masks.extend(altered_fields.removed_fields.clone());
            filter_masks.extend(altered_fields.updated_fields.clone());

            if !altered_fields.all_empty() {
                {
                    let complete_state_differences = server_state
                        .get_complete_state_by_field_mask(
                            filter_masks,
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

fn get_altered_fields(
    field_difference_tree: &StateDifferenceTree,
    paths: &[Path],
) -> AlteredFields {
    let mut altered_fields = AlteredFields::default();

    let expanded_added_paths = field_difference_tree.added_tree.expand_wildcards(paths);
    collect_altered_fields(
        &field_difference_tree.added_tree,
        &expanded_added_paths,
        &mut altered_fields.added_fields,
    );

    let expanded_removed_paths = field_difference_tree.removed_tree.expand_wildcards(paths);
    collect_altered_fields(
        &field_difference_tree.removed_tree,
        &expanded_removed_paths,
        &mut altered_fields.removed_fields,
    );

    let expanded_updated_paths = field_difference_tree.updated_tree.expand_wildcards(paths);
    collect_altered_fields(
        &field_difference_tree.updated_tree,
        &expanded_updated_paths,
        &mut altered_fields.updated_fields,
    );

    altered_fields
}

fn collect_altered_fields(tree: &Object, paths: &[Path], altered_fields: &mut Vec<String>) {
    for path in paths {
        if let Some(node) = tree.get(path).cloned() {
            let fields_matching_mask = collect_paths_iterative(&node, path.parts());
            fields_matching_mask.into_iter().for_each(|added_path| {
                altered_fields.push(added_path);
            });
        }
    }
}

/// Collect all leaf paths reachable from the provided start path.
pub fn collect_paths_iterative(root: &Value, start_path: &[String]) -> Vec<String> {
    let node = root;
    let prefix = start_path.join(".");
    let mut results = Vec::new();
    let mut stack = vec![(node, prefix)];
    while let Some((current, current_path)) = stack.pop() {
        match current {
            Value::Mapping(map) if !map.is_empty() => {
                for (current_key, current_value) in map {
                    if let Value::String(new_key) = current_key {
                        let new_path = format!("{current_path}.{new_key}");
                        stack.push((current_value, new_path));
                    }
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

impl From<AlteredFields> for api::ank_base::AlteredFields {
    fn from(altered_fields: AlteredFields) -> Self {
        api::ank_base::AlteredFields {
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

    use common::{
        from_server_interface::FromServer,
        objects::{
            AgentMap, CompleteState, State, WorkloadStatesMap, generate_test_stored_workload_spec,
        },
    };

    use super::StateDifferenceTree;

    use api::ank_base::response::ResponseContent;
    use mockall::predicate;

    use super::EventHandler;
    use crate::ankaios_server::{create_from_server_channel, server_state::MockServerState};

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
    // [utest->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
    #[tokio::test]
    async fn utest_event_handler_send_events() {
        let _ = env_logger::builder().is_test(true).try_init();
        let added_field_mask = "desiredState.workloads.workload_2";
        let updated_field_mask = "desiredState.workloads.workload_1.agent";
        let removed_field_mask = "configs.*";
        let expected_removed_field_mask = "configs.some_config";
        let added_workload = generate_test_stored_workload_spec("agent_A", "runtime_1");
        let mut mock_server_state = MockServerState::default();
        mock_server_state
            .expect_get_complete_state_by_field_mask()
            .once()
            .with(
                predicate::eq(vec![
                    added_field_mask.to_owned(),
                    expected_removed_field_mask.to_owned(),
                    updated_field_mask.to_owned(),
                ]),
                predicate::always(),
                predicate::always(),
            )
            .return_const(Ok(CompleteState {
                desired_state: State {
                    workloads: HashMap::from([
                        (
                            "workload_1".to_string(),
                            common::objects::StoredWorkloadSpec {
                                agent: "agent_1".to_string(),
                                ..Default::default()
                            },
                        ),
                        ("workload_2".to_string(), added_workload.clone()),
                    ]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into()));
        let workload_states_map = WorkloadStatesMap::default();
        let agent_map = AgentMap::default();

        let mut state_difference_tree = StateDifferenceTree::new();
        state_difference_tree.insert_added(vec![
            "desiredState".to_owned(),
            "workloads".to_owned(),
            "workload_2".to_owned(),
        ]);

        state_difference_tree.insert_updated(vec![
            "desiredState".to_owned(),
            "workloads".to_owned(),
            "workload_1".to_owned(),
            "agent".to_owned(),
        ]);

        state_difference_tree.insert_removed(vec!["configs".to_owned(), "some_config".to_owned()]);

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

        let expected_complete_state: api::ank_base::CompleteState = CompleteState {
            desired_state: State {
                workloads: HashMap::from([
                    (
                        "workload_1".to_owned(),
                        common::objects::StoredWorkloadSpec {
                            agent: "agent_1".to_owned(),
                            ..Default::default()
                        },
                    ),
                    ("workload_2".to_owned(), added_workload),
                ]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into();

        let expected_altered_fields = api::ank_base::AlteredFields {
            added_fields: vec![added_field_mask.to_owned()],
            updated_fields: vec![updated_field_mask.to_owned()],
            removed_fields: vec![expected_removed_field_mask.to_owned()],
        };

        assert_eq!(complete_state, expected_complete_state);
        assert_eq!(altered_fields, expected_altered_fields);
    }
}

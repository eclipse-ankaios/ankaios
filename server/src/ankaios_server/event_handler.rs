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

use super::request_id::RequestId;
use common::{
    from_server_interface::{FromServerInterface, FromServerSender},
    objects::{AgentMap, WorkloadStatesMap},
    state_manipulation::Path,
    std_extensions::IllegalStateResult,
};

use super::state_comparator::FieldDifference;
use std::collections::HashMap;

#[cfg(test)]
use mockall::automock;

const WILDCARD_SEPARATOR: &str = "*";

#[derive(Debug, PartialEq, Eq)]
enum MaskComparisonResult {
    ShorterSubscriberFieldMask,
    ShorterAlteredFieldMask,
    EqualLength,
    NoMatch,
}

#[derive(Debug, Default)]
struct AlteredFields {
    added_fields: Vec<String>,
    removed_fields: Vec<String>,
    updated_fields: Vec<String>,
}

impl AlteredFields {
    fn all_empty(&self) -> bool {
        self.added_fields.is_empty()
            && self.removed_fields.is_empty()
            && self.updated_fields.is_empty()
    }
}

type SubscribedFieldMasks = Vec<Path>;
#[derive(Debug, Default)]
pub struct EventHandler {
    subscriber_store: HashMap<RequestId, SubscribedFieldMasks>,
}

// [impl->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
fn fill_altered_fields_and_filter_masks(
    mut altered_fields: Vec<String>,
    mut filter_masks: Vec<String>,
    altered_field_mask: &Path,
    subscribed_field_masks: &SubscribedFieldMasks,
) -> (Vec<String>, Vec<String>) {
    for subscriber_mask in subscribed_field_masks {
        match compare_subscriber_mask_with_altered_field_mask(subscriber_mask, altered_field_mask) {
            MaskComparisonResult::NoMatch => {}
            MaskComparisonResult::ShorterSubscriberFieldMask
            | MaskComparisonResult::EqualLength => {
                filter_masks.push(String::from(altered_field_mask));
                altered_fields.push(altered_field_mask.into());
            }
            MaskComparisonResult::ShorterAlteredFieldMask => {
                // [impl->swdd~event-handler-expands-subscriber-field-mask-using-altered-field-masks~1]s
                let expanded_subscriber_mask =
                    expand_wildcards_in_subscriber_mask(subscriber_mask, altered_field_mask);
                filter_masks.push(String::from(expanded_subscriber_mask.clone()));
                altered_fields.push(expanded_subscriber_mask.into());
            }
        }
    }

    (altered_fields, filter_masks)
}

fn compare_subscriber_mask_with_altered_field_mask(
    subscriber_mask: &Path,
    altered_field_path: &Path,
) -> MaskComparisonResult {
    let mut subscriber_parts_iter = subscriber_mask.parts().iter();
    let mut altered_field_parts_iter = altered_field_path.parts().iter();

    let mut next_subscriber_part = subscriber_parts_iter.next();
    let mut next_altered_field_part = altered_field_parts_iter.next();
    while let Some(subscriber_part) = next_subscriber_part
        && let Some(altered_field_part) = next_altered_field_part
    {
        if subscriber_part != WILDCARD_SEPARATOR && subscriber_part != altered_field_part {
            return MaskComparisonResult::NoMatch;
        }
        next_subscriber_part = subscriber_parts_iter.next();
        next_altered_field_part = altered_field_parts_iter.next();
    }

    if next_subscriber_part.is_some() && next_altered_field_part.is_none() {
        return MaskComparisonResult::ShorterAlteredFieldMask;
    }

    if next_altered_field_part.is_some() && next_subscriber_part.is_none() {
        return MaskComparisonResult::ShorterSubscriberFieldMask;
    }

    MaskComparisonResult::EqualLength
}

// [impl->swdd~event-handler-expands-subscriber-field-mask-using-altered-field-masks~1]
fn expand_wildcards_in_subscriber_mask(subscriber_mask: &Path, altered_field_mask: &Path) -> Path {
    let mut expanded_subscriber_mask = altered_field_mask.parts().to_vec();

    for part in subscriber_mask
        .parts()
        .iter()
        .skip(expanded_subscriber_mask.len())
    {
        if part == WILDCARD_SEPARATOR {
            break;
        }
        expanded_subscriber_mask.push(part.clone());
    }

    Path::from(expanded_subscriber_mask)
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
        field_differences: Vec<FieldDifference>,
        from_server_channel: &FromServerSender,
    ) {
        for (request_id, subscribed_field_masks) in &self.subscriber_store {
            let mut filter_masks: Vec<String> = Vec::new();
            let mut altered_fields = AlteredFields::default();

            for field_difference in &field_differences {
                match field_difference {
                    FieldDifference::Added(path) => {
                        let added_mask: Path = path.clone().into();
                        (altered_fields.added_fields, filter_masks) =
                            fill_altered_fields_and_filter_masks(
                                altered_fields.added_fields,
                                filter_masks,
                                &added_mask,
                                subscribed_field_masks,
                            );
                    }
                    FieldDifference::Removed(path) => {
                        let removed_mask: Path = path.clone().into();
                        (altered_fields.removed_fields, filter_masks) =
                            fill_altered_fields_and_filter_masks(
                                altered_fields.removed_fields,
                                filter_masks,
                                &removed_mask,
                                subscribed_field_masks,
                            );
                    }
                    FieldDifference::Updated(path) => {
                        let updated_mask: Path = path.clone().into();
                        (altered_fields.updated_fields, filter_masks) =
                            fill_altered_fields_and_filter_masks(
                                altered_fields.updated_fields,
                                filter_masks,
                                &updated_mask,
                                subscribed_field_masks,
                            );
                    }
                }
            }

            if !altered_fields.all_empty() {
                {
                    let complete_state_differences = server_state
                        .get_complete_state_by_field_mask(
                            filter_masks.clone(),
                            workload_states_map,
                            agent_map,
                        )
                        .unwrap_or_illegal_state();

                    let altered_fields = api::ank_base::AlteredFields {
                        added_fields: altered_fields.added_fields,
                        updated_fields: altered_fields.updated_fields,
                        removed_fields: altered_fields.removed_fields,
                    };

                    log::debug!(
                        "Sending event to subscriber '{request_id}' with altered fields: {altered_fields:?} and complete state differences: {complete_state_differences:?}",
                    );

                    let request_id = to_string_id(request_id);
                    from_server_channel
                        .complete_state(
                            request_id,
                            complete_state_differences,
                            Some(altered_fields),
                        )
                        .await
                        .unwrap_or_illegal_state();
                }
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
        from_server_interface::FromServer,
        objects::{
            AgentMap, CompleteState, State, WorkloadStatesMap, generate_test_stored_workload_spec,
        },
    };

    use api::ank_base::response::ResponseContent;
    use mockall::predicate;

    use super::EventHandler;
    use crate::ankaios_server::{
        create_from_server_channel, server_state::MockServerState,
        state_comparator::FieldDifference,
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
    // [utest->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
    #[tokio::test]
    async fn utest_event_handler_send_events_subscriber_masks_equal() {
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
                    updated_field_mask.to_owned(),
                    expected_removed_field_mask.to_owned(),
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
        let field_differences = vec![
            FieldDifference::Added(vec![
                "desiredState".to_owned(),
                "workloads".to_owned(),
                "workload_2".to_owned(),
            ]),
            FieldDifference::Updated(vec![
                "desiredState".to_owned(),
                "workloads".to_owned(),
                "workload_1".to_owned(),
                "agent".to_owned(),
            ]),
            FieldDifference::Removed(vec!["configs".to_owned(), "some_config".to_owned()]),
        ];
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
                field_differences,
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

    // [utest->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
    // [utest->swdd~event-handler-expands-subscriber-field-mask-using-altered-field-masks~1]
    #[test]
    fn utest_fill_altered_fields_and_filter_masks_shorter_wildcard_subscriber_masks() {
        let expected_field_difference_mask = "desiredState.workloads.workload_1.agent";
        let subscribed_field_masks = vec!["desiredState.workloads.*".into()];
        let field_difference_mask = expected_field_difference_mask.into();

        let (altered_fields, filter_masks) = super::fill_altered_fields_and_filter_masks(
            Vec::new(),
            Vec::new(),
            &field_difference_mask,
            &subscribed_field_masks,
        );

        assert_eq!(
            altered_fields,
            vec![expected_field_difference_mask.to_owned(),]
        );
        assert_eq!(
            filter_masks,
            vec![expected_field_difference_mask.to_owned(),]
        );
    }

    // [utest->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
    #[test]
    fn utest_fill_altered_fields_and_filter_masks_shorter_subscriber_masks() {
        let expected_field_difference_mask = "desiredState.workloads.workload_1.agent";
        let subscribed_field_masks = vec!["desiredState.workloads".into()];
        let field_difference_mask = expected_field_difference_mask.into();

        let (altered_fields, filter_masks) = super::fill_altered_fields_and_filter_masks(
            Vec::new(),
            Vec::new(),
            &field_difference_mask,
            &subscribed_field_masks,
        );

        assert_eq!(
            altered_fields,
            vec![expected_field_difference_mask.to_owned(),]
        );
        assert_eq!(
            filter_masks,
            vec![expected_field_difference_mask.to_owned(),]
        );
    }

    // [utest->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
    // [utest->swdd~event-handler-expands-subscriber-field-mask-using-altered-field-masks~1]
    #[test]
    fn utest_fill_altered_fields_and_filter_masks_shorter_altered_field_mask_subscriber_wildcards()
    {
        let altered_field_mask = "desiredState.workloads.workload_1";
        let subscribed_field_masks = vec!["desiredState.workloads.*.agent".into()];
        let altered_field_mask_path = altered_field_mask.into();
        let expected_altered_field_mask = "desiredState.workloads.workload_1.agent";

        let (altered_fields, filter_masks) = super::fill_altered_fields_and_filter_masks(
            Vec::new(),
            Vec::new(),
            &altered_field_mask_path,
            &subscribed_field_masks,
        );

        assert_eq!(
            altered_fields,
            vec![expected_altered_field_mask.to_owned(),]
        );
        assert_eq!(filter_masks, vec![expected_altered_field_mask.to_owned(),]);
    }

    // [utest->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
    // [utest->swdd~event-handler-expands-subscriber-field-mask-using-altered-field-masks~1]
    #[test]
    fn utest_fill_altered_fields_and_filter_masks_shorter_altered_field_mask_no_wildcards_in_expanded_mask()
     {
        let altered_field_mask = "desiredState.workloads.workload_1";
        let subscribed_field_masks = vec!["desiredState.*.*.*".into()];
        let altered_field_mask_path = altered_field_mask.into();
        let expected_altered_field_masks = vec!["desiredState.workloads.workload_1".to_owned()];

        let (altered_fields, filter_masks) = super::fill_altered_fields_and_filter_masks(
            Vec::new(),
            Vec::new(),
            &altered_field_mask_path,
            &subscribed_field_masks,
        );

        assert_eq!(altered_fields, expected_altered_field_masks);
        assert_eq!(filter_masks, expected_altered_field_masks);
    }

    // [utest->swdd~event-handler-creates-altered-fields-and-filter-masks~1]
    #[test]
    fn utest_fill_altered_fields_and_filter_masks_shorter_altered_field_mask() {
        let altered_field_mask = "desiredState.workloads.workload_1";
        let expected_altered_field_mask = "desiredState.workloads.workload_1.agent";
        let subscribed_field_masks = vec![expected_altered_field_mask.into()];
        let altered_field_mask_path = altered_field_mask.into();

        let (altered_fields, filter_masks) = super::fill_altered_fields_and_filter_masks(
            Vec::new(),
            Vec::new(),
            &altered_field_mask_path,
            &subscribed_field_masks,
        );

        assert_eq!(
            altered_fields,
            vec![expected_altered_field_mask.to_owned(),]
        );
        assert_eq!(filter_masks, vec![expected_altered_field_mask.to_owned(),]);
    }
}

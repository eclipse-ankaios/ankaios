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
#[cfg_attr(test, mockall_double::double)]
use crate::ankaios_server::server_state::ServerState;

use super::request_id::RequestId;
use common::from_server_interface::{FromServerInterface, FromServerSender};
use common::objects::{AgentMap, WorkloadStatesMap};
use common::state_manipulation::{FieldDifference, Path};
use common::std_extensions::IllegalStateResult;
use std::collections::HashMap;

type SubscribedFieldMasks = Vec<Path>;

#[derive(Debug, PartialEq, Eq)]
enum MaskComparisonResult {
    SubscriberMaskShorter,
    FieldDifferenceMaskShorter,
    MasksEqualLength,
    NoMatch,
}

#[derive(Debug, Default)]
pub struct EventHandler {
    subscriber_store: HashMap<RequestId, SubscribedFieldMasks>,
}

fn fill_altered_fields_and_filter_masks(
    mut altered_fields: Vec<String>,
    mut filter_masks: Vec<String>,
    field_difference_mask: &Path,
    subscribed_field_masks: &SubscribedFieldMasks,
) -> (Vec<String>, Vec<String>) {
    for subscriber_mask in subscribed_field_masks {
        match compare_subscriber_mask_with_field_difference_mask(
            subscriber_mask,
            field_difference_mask,
        ) {
            MaskComparisonResult::NoMatch => {}
            MaskComparisonResult::SubscriberMaskShorter => {
                filter_masks.push(String::from(field_difference_mask));
                altered_fields.push(field_difference_mask.into());
            }
            MaskComparisonResult::FieldDifferenceMaskShorter => {
                filter_masks.push(String::from(subscriber_mask));
                altered_fields.push(subscriber_mask.into());
            }
            MaskComparisonResult::MasksEqualLength => {
                filter_masks.push(String::from(subscriber_mask));
                altered_fields.push(field_difference_mask.into());
            }
        }
    }

    (altered_fields, filter_masks)
}

fn compare_subscriber_mask_with_field_difference_mask(
    subscriber_mask: &Path,
    field_difference_path: &Path,
) -> MaskComparisonResult {
    let mut subscriber_parts_iter = subscriber_mask.parts().iter();
    let mut field_difference_parts_iter = field_difference_path.parts().iter();

    while let Some(subscriber_part) = subscriber_parts_iter.next()
        && let Some(field_difference_part) = field_difference_parts_iter.next()
    {
        if subscriber_part != "*" && subscriber_part != field_difference_part {
            return MaskComparisonResult::NoMatch;
        }
    }

    if subscriber_parts_iter.next().is_some() && field_difference_parts_iter.next().is_none() {
        return MaskComparisonResult::FieldDifferenceMaskShorter;
    }

    if field_difference_parts_iter.next().is_some() && subscriber_parts_iter.next().is_none() {
        return MaskComparisonResult::SubscriberMaskShorter;
    }

    MaskComparisonResult::MasksEqualLength
}

impl EventHandler {
    pub fn add_subscriber(&mut self, request_id: String, field_masks: SubscribedFieldMasks) {
        log::debug!("Adding subscriber '{request_id}' with field masks: {field_masks:?}",);
        self.subscriber_store.insert(request_id.into(), field_masks);
    }

    pub fn remove_subscriber(&mut self, request_id: String) {
        log::debug!("Removing subscriber '{request_id}'");
        self.subscriber_store.remove(&request_id.into());
    }

    pub fn has_subscribers(&self) -> bool {
        !self.subscriber_store.is_empty()
    }

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
            let mut added_fields: Vec<String> = Vec::new();
            let mut removed_fields: Vec<String> = Vec::new();
            let mut updated_fields: Vec<String> = Vec::new();

            for field_difference in &field_differences {
                match field_difference {
                    FieldDifference::Added(path) => {
                        let added_mask: Path = path.clone().into();
                        let (extended_added_fields, extended_filter_masks) =
                            fill_altered_fields_and_filter_masks(
                                added_fields,
                                filter_masks,
                                &added_mask,
                                subscribed_field_masks,
                            );
                        added_fields = extended_added_fields;
                        filter_masks = extended_filter_masks;
                    }
                    FieldDifference::Removed(path) => {
                        let removed_mask: Path = path.clone().into();
                        let (extended_removed_fields, extended_filter_masks) =
                            fill_altered_fields_and_filter_masks(
                                removed_fields,
                                filter_masks,
                                &removed_mask,
                                subscribed_field_masks,
                            );
                        removed_fields = extended_removed_fields;
                        filter_masks = extended_filter_masks;
                    }
                    FieldDifference::Updated(path) => {
                        let updated_mask: Path = path.clone().into();
                        let (extended_updated_fields, extended_filter_masks) =
                            fill_altered_fields_and_filter_masks(
                                updated_fields,
                                filter_masks,
                                &updated_mask,
                                subscribed_field_masks,
                            );
                        updated_fields = extended_updated_fields;
                        filter_masks = extended_filter_masks;
                    }
                }
            }

            if !added_fields.is_empty() || !removed_fields.is_empty() || !updated_fields.is_empty()
            {
                let new_complete_state = server_state
                    .get_complete_state_by_field_mask(
                        filter_masks.clone(),
                        workload_states_map,
                        agent_map,
                    )
                    .unwrap_or_illegal_state();

                // TODO: rename fields and assign directly to avoid the risk of mixing the vectors
                let altered_fields = api::ank_base::AlteredFields {
                    added_fields,
                    updated_fields,
                    removed_fields,
                };

                let new_complete_state_yaml = serde_yaml::to_string(&new_complete_state);
                log::debug!(
                    "Sending event to subscriber '{}' with filter masks: {:?}, altered fields: {:?} and complete state: {}",
                    request_id,
                    filter_masks,
                    altered_fields,
                    new_complete_state_yaml.unwrap_or_default()
                );

                from_server_channel
                    .complete_state(
                        request_id.to_string(),
                        new_complete_state,
                        Some(altered_fields),
                    )
                    .await
                    .unwrap_or_illegal_state();
            }
        }
    }
}

#[cfg(test)]
mod tests {}

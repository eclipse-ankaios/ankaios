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

use super::CliCommands;
use crate::cli::OutputFormat;
use crate::cli_error::CliError;
use crate::filtered_complete_state::{FilteredAlteredFields, FilteredCompleteState, FilteredEvent};
use crate::output;
use crate::output_debug;
use api::ank_base;
use chrono::Utc;

impl CliCommands {
    pub async fn get_events(
        &mut self,
        object_field_mask: Vec<String>,
        output_format: OutputFormat,
    ) -> Result<(), CliError> {
        output_debug!(
            "Got: object_field_mask: {:?}, output_format: {:?} ",
            object_field_mask,
            output_format
        );

        let mut subscription = self
            .server_connection
            .subscribe_and_listen_for_events(object_field_mask)
            .await?;

        while let Some(event) = self
            .server_connection
            .receive_next_event(&mut subscription)
            .await?
        {
            Self::output_event(&event, &output_format)?;
        }

        Ok(())
    }

    fn output_event(
        event: &ank_base::CompleteStateResponse,
        output_format: &OutputFormat,
    ) -> Result<(), CliError> {
        let filtered_state: FilteredCompleteState = (*event).clone().into();

        let timestamp_str = Utc::now().to_rfc3339();

        let altered_fields = event
            .altered_fields
            .as_ref()
            .map(|af| FilteredAlteredFields {
                added_fields: af.added_fields.clone(),
                updated_fields: af.updated_fields.clone(),
                removed_fields: af.removed_fields.clone(),
            });

        let event_output = FilteredEvent {
            timestamp: timestamp_str,
            altered_fields,
            complete_state: Some(filtered_state),
        };

        let output = match output_format {
            OutputFormat::Yaml => serde_yaml::to_string(&event_output)
                .map_err(|err| CliError::ExecutionError(err.to_string()))?,
            OutputFormat::Json => serde_json::to_string_pretty(&event_output)
                .map_err(|err| CliError::ExecutionError(err.to_string()))?,
        };

        output!("{output}");

        Ok(())
    }
}

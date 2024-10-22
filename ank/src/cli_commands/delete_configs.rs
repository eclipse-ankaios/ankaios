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

use common::objects::CompleteState;

use crate::{cli_error::CliError, output_debug};

use super::CliCommands;

impl CliCommands {
    pub async fn delete_configs(&mut self, config_names: Vec<String>) -> Result<(), CliError> {
        let complete_state_update = CompleteState::default();

        let update_mask = config_names
            .into_iter()
            .map(|name_of_config_to_delete| {
                format!("desiredState.configs.{}", name_of_config_to_delete)
            })
            .collect();

        output_debug!(
            "Updating with empty complete state and update mask {:?}",
            update_mask
        );

        self.server_connection
            .update_state(complete_state_update, update_mask)
            .await
            .map_err(|error| {
                CliError::ExecutionError(format!("Failed to delete configs: {:?}", error))
            })?;

        Ok(())
    }
}

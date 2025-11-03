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
use crate::output_debug;

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

        self.server_connection
            .subscribe_and_listen_for_events(object_field_mask, output_format)
            .await?;

        Ok(())
    }
}

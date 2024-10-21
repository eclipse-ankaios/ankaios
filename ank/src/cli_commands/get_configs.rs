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
use super::CliCommands;

use crate::cli_commands::config_table_row::ConfigTableRow;
use crate::{cli_commands::cli_table::CliTable, cli_error::CliError, output_debug};
use common::objects::ConfigItem;
const EMPTY_FILTER_MASK: [String; 0] = [];

impl CliCommands {
    pub async fn get_configs(&mut self) -> Result<String, CliError> {
        let filtered_complete_state = self
            .server_connection
            .get_complete_state(&EMPTY_FILTER_MASK)
            .await?;

        let configs = filtered_complete_state
            .desired_state
            .and_then(|state| state.configs)
            .unwrap_or_default()
            .into_iter();

        let config_table_rows = transform_into_table_rows(configs);

        output_debug!("Got configs: {:?}", config_table_rows);

        Ok(CliTable::new(&config_table_rows).create_default_table())
    }
}

fn transform_into_table_rows(
    configs: impl Iterator<Item = (String, ConfigItem)>,
) -> Vec<ConfigTableRow> {
    let config_table_rows: Vec<ConfigTableRow> = configs
        .map(|(config_str, _config_item)| ConfigTableRow { config: config_str })
        .collect();
    config_table_rows
}

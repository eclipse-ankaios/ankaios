use std::collections::HashMap;

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
use crate::{
    cli_commands::{agent_table_row::AgentTableRow, table_builder::TableBuilder},
    cli_error::CliError,
    filtered_complete_state::FilteredWorkloadSpec,
    output_debug,
};

use super::CliCommands;
const DEFAULT_WORKLOAD_COUNT: u32 = 0;

impl CliCommands {
    pub async fn get_agents(&mut self) -> Result<String, CliError> {
        let empty_filter_mask = [];

        let filtered_complete_state = self
            .server_connection
            .get_complete_state(&empty_filter_mask)
            .await?;

        let workloads = filtered_complete_state
            .desired_state
            .and_then(|desired_state| desired_state.workloads)
            .unwrap_or_default()
            .into_values();

        let workload_count_per_agent = self.count_workloads_per_agent(workloads);

        let connected_agents = filtered_complete_state
            .agents
            .and_then(|agents| agents.agents)
            .unwrap_or_default()
            .into_keys();

        let agent_table_rows =
            self.transform_into_table_rows(connected_agents, workload_count_per_agent);

        output_debug!("Got agents of complete state: {:?}", agent_table_rows);

        let table = TableBuilder::new(agent_table_rows)
            .style_blank()
            .disable_surrounding_padding()
            .build();

        Ok(table)
    }

    fn count_workloads_per_agent(
        &self,
        workload_specs: impl Iterator<Item = FilteredWorkloadSpec>,
    ) -> HashMap<String, u32> {
        workload_specs.fold(HashMap::new(), |mut init, workload| {
            if let Some(agent) = workload.agent {
                let count = init.entry(agent).or_insert(DEFAULT_WORKLOAD_COUNT);
                *count += 1;
            }
            init
        })
    }

    fn transform_into_table_rows(
        &self,
        agents_map: impl Iterator<Item = String>,
        mut workload_count_per_agent: HashMap<String, u32>,
    ) -> Vec<AgentTableRow> {
        let mut agent_table_rows: Vec<AgentTableRow> = agents_map
            .map(|agent_name| {
                let workload_count = workload_count_per_agent
                    .remove(&agent_name)
                    .unwrap_or(DEFAULT_WORKLOAD_COUNT);
                AgentTableRow {
                    agent_name,
                    workloads: workload_count,
                }
            })
            .collect();

        // sort to ensure consistent output
        agent_table_rows.sort_by(|a, b| a.agent_name.cmp(&b.agent_name));
        agent_table_rows
    }
}

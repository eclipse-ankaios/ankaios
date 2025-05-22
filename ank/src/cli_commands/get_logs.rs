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

use std::collections::HashSet;

use common::objects::{WorkloadInstanceName, WorkloadState};

use crate::cli::LogsArgs;
use crate::cli_error::CliError;

use super::CliCommands;

impl CliCommands {
    pub async fn follow_logs(&mut self, args: LogsArgs) -> Result<(), CliError> {
        let workload_instance_names = self
            .workload_names_to_instance_names(args.workload_name.clone())
            .await?;
        self.server_connection
            .stream_logs(workload_instance_names, args)
            .await
            .map_err(|e| {
                CliError::ExecutionError(format!(
                    "Failed to get logs for workload instances: '{:?}'",
                    e
                ))
            })
    }

    pub async fn fetch_logs(&mut self, args: LogsArgs) -> Result<(), CliError> {
        let workload_instance_names = self
            .workload_names_to_instance_names(args.workload_name.clone())
            .await?;
        self.server_connection
            .get_logs(workload_instance_names, args)
            .await
            .map_err(|e| {
                CliError::ExecutionError(format!(
                    "Failed to get logs for workload instances: '{:?}'",
                    e
                ))
            })
    }

    async fn workload_names_to_instance_names(
        &mut self,
        workload_names: Vec<String>,
    ) -> Result<Vec<WorkloadInstanceName>, CliError> {
        let filter_mask_workload_states = ["workloadStates".to_string()];
        let complete_state = self
            .server_connection
            .get_complete_state(&filter_mask_workload_states)
            .await?;

        if let Some(wl_states) = complete_state.workload_states {
            let available_instance_names: HashSet<WorkloadInstanceName> =
                Vec::<WorkloadState>::from(wl_states)
                    .into_iter()
                    .map(|wl_state| wl_state.instance_name)
                    .collect();

            Ok(workload_names
                .into_iter()
                .fold(vec![], |mut acc, workload_name| {
                    if let Some(instance_name) = available_instance_names
                        .iter()
                        .find(|name| name.workload_name().eq(&workload_name))
                    {
                        acc.push(instance_name.clone());
                    }
                    acc
                }))
        } else {
            Err(CliError::ExecutionError(
                "No workload states available to convert workload names to instance names."
                    .to_string(),
            ))
        }
    }
}

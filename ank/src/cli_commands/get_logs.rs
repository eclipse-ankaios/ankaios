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

use std::collections::BTreeSet;
use std::collections::HashMap;

use common::objects::{WorkloadInstanceName, WorkloadState};

use crate::cli::LogsArgs;
use crate::cli_error::CliError;

use super::CliCommands;

impl CliCommands {
    pub async fn get_logs_blocking(&mut self, args: LogsArgs) -> Result<(), CliError> {
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

    async fn workload_names_to_instance_names(
        &mut self,
        workload_names: Vec<String>,
    ) -> Result<BTreeSet<WorkloadInstanceName>, CliError> {
        let filter_mask_workload_states = ["workloadStates".to_string()];
        let complete_state = self
            .server_connection
            .get_complete_state(&filter_mask_workload_states)
            .await?;

        if let Some(wl_states) = complete_state.workload_states {
            let available_instance_names: HashMap<String, WorkloadInstanceName> =
                Vec::<WorkloadState>::from(wl_states)
                    .into_iter()
                    .map(|wl_state| {
                        (
                            wl_state.instance_name.workload_name().to_owned(),
                            wl_state.instance_name,
                        )
                    })
                    .collect();

            let mut converted_instance_names = BTreeSet::new();
            for wl_name in workload_names {
                if let Some(instance_name) = available_instance_names.get(&wl_name) {
                    converted_instance_names.insert(instance_name.clone());
                } else {
                    return Err(CliError::ExecutionError(format!(
                        "Workload name '{}' does not exist.",
                        wl_name
                    )));
                }
            }

            Ok(converted_instance_names)
        } else {
            Err(CliError::ExecutionError(
                "No workload states available to convert workload names to instance names."
                    .to_string(),
            ))
        }
    }
}

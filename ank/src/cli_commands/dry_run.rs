// Copyright (c) 2026 TRATON AB
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

use std::collections::BTreeMap;

use ankaios_api::ank_base::CompleteState;

use super::{
    CliCommands, cli_table::CliTable, dry_run_table_row::DryRunTableRow,
    wait_list::ParsedUpdateStateSuccess,
};
use crate::{cli_error::CliError, output, output_debug};

const ACTION_ADDED: &str = "added";
const ACTION_UPDATED: &str = "updated";
const ACTION_DELETED: &str = "deleted";
const NO_CHANGES_MESSAGE: &str = "No workloads would be changed.";
const UNSCHEDULED_AGENT: &str = "\"\"";

impl CliCommands {
    // [impl->swdd~cli-sends-update-state-dry-run~1]
    // [impl->swdd~cli-presents-dry-run-error~1]
    pub async fn dry_run_update_state(
        &mut self,
        new_state: CompleteState,
        update_mask: Vec<String>,
    ) -> Result<(), CliError> {
        let update_state_success = self
            .server_connection
            .update_state(new_state, update_mask, true)
            .await?;

        output_debug!("Got dry run result: {:?}", update_state_success);

        let changed_workloads = ParsedUpdateStateSuccess::try_from(update_state_success)
            .map_err(|error| {
                CliError::ExecutionError(format!(
                    "Could not parse UpdateStateSuccess message: {error}"
                ))
            })?;

        output!("{}", create_dry_run_output(changed_workloads));
        Ok(())
    }
}

// [impl->swdd~cli-presents-dry-run-result~1]
// [impl->swdd~cli-presents-dry-run-no-changes~1]
fn create_dry_run_output(changed_workloads: ParsedUpdateStateSuccess) -> String {
    let table_rows = create_dry_run_table_rows(changed_workloads);
    if table_rows.is_empty() {
        return NO_CHANGES_MESSAGE.to_string();
    }

    CliTable::new(&table_rows).create_default_table()
}

fn create_dry_run_table_rows(changed_workloads: ParsedUpdateStateSuccess) -> Vec<DryRunTableRow> {
    let mut deleted_workloads: BTreeMap<String, String> = changed_workloads
        .deleted_workloads
        .iter()
        .map(|instance_name| {
            (
                instance_name.workload_name().to_owned(),
                instance_name.agent_name().to_owned(),
            )
        })
        .collect();

    let mut table_rows = Vec::new();
    for instance_name in &changed_workloads.added_workloads {
        let workload_name = instance_name.workload_name().to_owned();
        let new_agent = instance_name.agent_name().to_owned();

        // Ankaios updates a workload by deleting it and adding it again
        let (agent, action) = match deleted_workloads.remove(&workload_name) {
            Some(previous_agent) if previous_agent != new_agent => (
                format!(
                    "{} -> {}",
                    agent_or_unscheduled(&previous_agent),
                    agent_or_unscheduled(&new_agent)
                ),
                ACTION_UPDATED,
            ),
            Some(_) => (new_agent, ACTION_UPDATED),
            None => (new_agent, ACTION_ADDED),
        };

        table_rows.push(DryRunTableRow {
            workload_name,
            agent,
            action: action.to_string(),
        });
    }

    table_rows.extend(
        deleted_workloads
            .into_iter()
            .map(|(workload_name, agent)| DryRunTableRow {
                workload_name,
                agent,
                action: ACTION_DELETED.to_string(),
            }),
    );

    table_rows.sort_by(|a, b| a.workload_name.cmp(&b.workload_name));
    table_rows
}

fn agent_or_unscheduled(agent: &str) -> &str {
    if agent.is_empty() {
        UNSCHEDULED_AGENT
    } else {
        agent
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
    use super::{NO_CHANGES_MESSAGE, create_dry_run_output};
    use crate::cli_commands::{
        CliCommands, server_connection::MockServerConnection,
        server_connection::ServerConnectionError, wait_list::ParsedUpdateStateSuccess,
    };
    use crate::cli_error::CliError;

    use ankaios_api::ank_base::{CompleteState, UpdateStateSuccess, WorkloadInstanceNameSpec};
    use mockall::predicate::eq;

    fn instance_name(workload_name: &str, agent_name: &str) -> WorkloadInstanceNameSpec {
        WorkloadInstanceNameSpec::new(agent_name, workload_name, "id")
    }

    // [utest->swdd~cli-presents-dry-run-result~1]
    #[test]
    fn utest_create_dry_run_output_presents_changed_workloads_as_sorted_table() {
        let changed_workloads = ParsedUpdateStateSuccess {
            added_workloads: vec![
                instance_name("nginx", "agent_A"),
                instance_name("databroker", "agent_A"),
                instance_name("logger", "agent_B"),
            ],
            deleted_workloads: vec![
                instance_name("old_service", "agent_B"),
                instance_name("logger", "agent_A"),
                instance_name("databroker", "agent_A"),
            ],
        };

        let output = create_dry_run_output(changed_workloads);

        let expected_output = [
            "WORKLOAD NAME   AGENT                ACTION ",
            "databroker      agent_A              updated",
            "logger          agent_A -> agent_B   updated",
            "nginx           agent_A              added  ",
            "old_service     agent_B              deleted",
        ]
        .join("\n");
        assert_eq!(output, expected_output);
    }

    // [utest->swdd~cli-presents-dry-run-result~1]
    #[test]
    fn utest_create_dry_run_output_presents_unscheduled_workloads() {
        let changed_workloads = ParsedUpdateStateSuccess {
            added_workloads: vec![
                instance_name("nginx", ""),
                instance_name("logger", "agent_B"),
            ],
            deleted_workloads: vec![
                instance_name("nginx", "agent_A"),
                instance_name("logger", ""),
            ],
        };

        let output = create_dry_run_output(changed_workloads);

        let expected_output = [
            "WORKLOAD NAME   AGENT           ACTION ",
            "logger          \"\" -> agent_B   updated",
            "nginx           agent_A -> \"\"   updated",
        ]
        .join("\n");
        assert_eq!(output, expected_output);
    }

    // [utest->swdd~cli-presents-dry-run-no-changes~1]
    #[test]
    fn utest_create_dry_run_output_without_changes() {
        let changed_workloads = ParsedUpdateStateSuccess {
            added_workloads: vec![],
            deleted_workloads: vec![],
        };

        let output = create_dry_run_output(changed_workloads);

        assert_eq!(output, NO_CHANGES_MESSAGE);
    }

    // [utest->swdd~cli-sends-update-state-dry-run~1]
    #[tokio::test]
    async fn utest_dry_run_update_state_sends_request_without_watch() {
        let new_state = CompleteState::default();
        let update_mask = vec!["desiredState.workloads.nginx".to_string()];

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(eq(new_state.clone()), eq(update_mask.clone()), eq(true))
            .once()
            .return_once(|_, _, _| Ok(UpdateStateSuccess::default()));
        mock_server_connection.expect_get_complete_state().never();
        mock_server_connection
            .expect_read_next_update_workload_state()
            .never();

        let mut cmd = CliCommands {
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.dry_run_update_state(new_state, update_mask).await;

        assert!(result.is_ok());
    }

    // [utest->swdd~cli-presents-dry-run-error~1]
    #[tokio::test]
    async fn utest_dry_run_update_state_returns_server_error() {
        const ERROR_MESSAGE: &str = "SetState failed with: 'Update rejected'";

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .once()
            .return_once(|_, _, _| {
                Err(ServerConnectionError::ExecutionError(
                    ERROR_MESSAGE.to_string(),
                ))
            });

        let mut cmd = CliCommands {
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd
            .dry_run_update_state(CompleteState::default(), vec![])
            .await;

        assert!(matches!(
            result,
            Err(CliError::ExecutionError(message)) if message == ERROR_MESSAGE
        ));
    }
}

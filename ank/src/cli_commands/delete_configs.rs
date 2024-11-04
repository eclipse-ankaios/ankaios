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

use crate::{cli_commands::DESIRED_STATE_CONFIGS, cli_error::CliError, output_debug};

use super::CliCommands;

impl CliCommands {
    // [impl->swdd~cli-provides-delete-configs~1]
    pub async fn delete_configs(&mut self, config_names: Vec<String>) -> Result<(), CliError> {
        let complete_state_update = CompleteState::default();

        let update_mask = config_names
            .into_iter()
            .map(|name_of_config_to_delete| {
                format!("{}.{}", DESIRED_STATE_CONFIGS, name_of_config_to_delete)
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use crate::cli_commands::{server_connection::MockServerConnection, CliCommands};
    use crate::filtered_complete_state::FilteredCompleteState;
    use api::ank_base::UpdateStateSuccess;
    use common::objects::CompleteState;
    use mockall::predicate::eq;

    const RESPONSE_TIMEOUT_MS: u64 = 3000;
    const CONFIG_1: &str = "config_1";
    const CONFIG_2: &str = "config_2";

    // [utest->swdd~cli-provides-delete-configs~1]
    #[tokio::test]
    async fn utest_delete_configs_two_configs() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_update_state()
            .with(
                eq(CompleteState::default()),
                eq(vec![
                    ["desiredState.configs.", CONFIG_1].join(""),
                    ["desiredState.configs.", CONFIG_2].join(""),
                ]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec![],
                })
            });
        mock_server_connection
            .expect_get_complete_state()
            .once()
            .returning(|_| Ok(FilteredCompleteState::default()));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let delete_result = cmd
            .delete_configs(vec![CONFIG_1.to_string(), CONFIG_2.to_string()])
            .await;
        assert!(delete_result.is_ok());

        // Verify that the deleted configs no longer exist in the desired state
        let get_result = cmd.get_configs().await.unwrap();

        assert!(!get_result.contains(CONFIG_1));
        assert!(!get_result.contains(CONFIG_2));
    }

    // [utest->swdd~cli-provides-delete-configs~1]
    #[tokio::test]
    async fn utest_delete_configs_unknown_config() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let complete_state_update = CompleteState::default();

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(
                eq(complete_state_update),
                eq(vec!["desiredState.configs.unknown_config".to_string()]),
            )
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec![],
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let delete_result = cmd.delete_configs(vec!["unknown_config".to_string()]).await;
        assert!(delete_result.is_ok());
    }
}

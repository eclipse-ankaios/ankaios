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

use std::collections::{BTreeSet, HashMap};

use common::objects::{WorkloadInstanceName, WorkloadState};

use crate::cli::LogsArgs;
use crate::cli_error::CliError;

use super::CliCommands;

impl CliCommands {
    // [impl->swdd~cli-provides-workload-logs~1]
    // [impl->swdd~cli-streams-logs-from-the-server~1]
    pub async fn get_logs_blocking(&mut self, args: LogsArgs) -> Result<(), CliError> {
        let workload_instance_names = self
            .workload_names_to_instance_names(args.workload_name.clone())
            .await?;

        self.server_connection
            .stream_logs(workload_instance_names, args)
            .await
            .map_err(|e| CliError::ExecutionError(format!("Failed to get logs: '{:?}'", e)))
    }

    // [impl->swdd~cli-uses-workload-states-to-sample-workload-to-instance-names~1]
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
            let available_instance_names: HashMap<String, BTreeSet<WorkloadInstanceName>> =
                Vec::<WorkloadState>::from(wl_states).into_iter().fold(
                    HashMap::new(),
                    |mut acc, wl_state| {
                        let instance_name = wl_state.instance_name.clone();
                        let workload_name = instance_name.workload_name();
                        acc.entry(workload_name.to_owned())
                            .or_default()
                            .insert(instance_name);
                        acc
                    },
                );

            let mut converted_instance_names = BTreeSet::new();
            for wl_name in workload_names {
                if let Some(instance_names) = available_instance_names.get(&wl_name) {
                    for workload_instance_name in instance_names {
                        converted_instance_names.insert(workload_instance_name.clone());
                    }
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::cli::LogsArgs;
    use crate::cli_commands::{
        server_connection::{MockServerConnection, ServerConnectionError},
        CliCommands,
    };
    use crate::cli_error::CliError;
    use api::ank_base;
    use common::objects::WorkloadInstanceName;
    use common::{objects::generate_test_workload_spec_with_param, test_utils};
    use mockall::predicate;

    const RESPONSE_TIMEOUT_MS: u64 = 3000;
    const AGENT_A_NAME: &str = "agent_A";
    const AGENT_B_NAME: &str = "agent_B";
    const WORKLOAD_NAME_1: &str = "workload_1";
    const WORKLOAD_NAME_2: &str = "workload_2";
    const RUNTIME_NAME: &str = "runtime";

    // [utest->swdd~cli-provides-workload-logs~1]
    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_get_locks_blocking_success() {
        let log_workload = generate_test_workload_spec_with_param(
            AGENT_A_NAME.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let cloned_log_workload = log_workload.clone();
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .with(predicate::eq(vec!["workloadStates".to_string()]))
            .return_once(|_| {
                Ok(
                    ank_base::CompleteState::from(test_utils::generate_test_complete_state(vec![
                        cloned_log_workload,
                        generate_test_workload_spec_with_param(
                            AGENT_B_NAME.to_string(),
                            WORKLOAD_NAME_2.to_string(),
                            RUNTIME_NAME.to_string(),
                        ),
                    ]))
                    .into(),
                )
            });

        let instance_names: BTreeSet<WorkloadInstanceName> =
            BTreeSet::from([log_workload.instance_name.clone()]);

        let args = LogsArgs {
            workload_name: vec![WORKLOAD_NAME_1.to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
        };

        mock_server_connection
            .expect_stream_logs()
            .with(
                predicate::eq(instance_names),
                predicate::function(|args: &LogsArgs| {
                    args.workload_name == vec![WORKLOAD_NAME_1.to_string()]
                        && !args.follow
                        && args.tail == -1
                        && args.since.is_none()
                        && args.until.is_none()
                }),
            )
            .once()
            .return_once(|_, _| Ok(()));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };
        let result = cmd.get_logs_blocking(args).await;

        assert!(result.is_ok(), "Got result {:?}", result);
    }

    // [utest->swdd~cli-provides-workload-logs~1]
    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_get_locks_blocking_fails_to_convert_workload_names() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| {
                Err(ServerConnectionError::ExecutionError(
                    "Failed to get CompleteState".to_string(),
                ))
            });

        let args = LogsArgs {
            workload_name: vec![WORKLOAD_NAME_1.to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
        };

        mock_server_connection.expect_stream_logs().never();

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };
        let result = cmd.get_logs_blocking(args).await;

        assert_eq!(
            result,
            Err(CliError::ExecutionError(
                "Failed to get CompleteState".to_string()
            ))
        );
    }

    // [utest->swdd~cli-provides-workload-logs~1]
    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_get_locks_blocking_fails_when_streaming_logs() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| {
                Ok(
                    ank_base::CompleteState::from(test_utils::generate_test_complete_state(vec![
                        generate_test_workload_spec_with_param(
                            AGENT_A_NAME.to_string(),
                            WORKLOAD_NAME_1.to_string(),
                            RUNTIME_NAME.to_string(),
                        ),
                    ]))
                    .into(),
                )
            });

        let args = LogsArgs {
            workload_name: vec![WORKLOAD_NAME_1.to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
        };

        mock_server_connection
            .expect_stream_logs()
            .once()
            .return_once(|_, _| {
                Err(ServerConnectionError::ExecutionError(
                    "streaming error".to_string(),
                ))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };
        let result = cmd.get_logs_blocking(args).await;

        assert_eq!(
            result,
            Err(CliError::ExecutionError(format!(
                "Failed to get logs: '{:?}'",
                ServerConnectionError::ExecutionError("streaming error".to_string())
            )))
        );
    }

    // [utest->swdd~cli-uses-workload-states-to-sample-workload-to-instance-names~1]
    #[tokio::test]
    async fn utest_workload_names_to_instance_names_workload_does_not_exist() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| {
                Ok(
                    ank_base::CompleteState::from(test_utils::generate_test_complete_state(vec![
                        generate_test_workload_spec_with_param(
                            AGENT_A_NAME.to_string(),
                            WORKLOAD_NAME_1.to_string(),
                            RUNTIME_NAME.to_string(),
                        ),
                    ]))
                    .into(),
                )
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        const NOT_EXISTING_WORKLOAD_NAME: &str = "non_existing_workload";
        let workload_names = vec![NOT_EXISTING_WORKLOAD_NAME.to_string()];
        let result = cmd.workload_names_to_instance_names(workload_names).await;

        assert_eq!(
            result,
            Err(CliError::ExecutionError(format!(
                "Workload name '{}' does not exist.",
                NOT_EXISTING_WORKLOAD_NAME
            )))
        );
    }

    // [utest->swdd~cli-uses-workload-states-to-sample-workload-to-instance-names~1]
    #[tokio::test]
    async fn utest_workload_names_to_instance_names_no_workload_states_available() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| Ok(ank_base::CompleteState::default().into()));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let workload_names = vec![WORKLOAD_NAME_1.to_string()];
        let result = cmd.workload_names_to_instance_names(workload_names).await;

        assert_eq!(
            result,
            Err(CliError::ExecutionError(
                "No workload states available to convert workload names to instance names."
                    .to_string(),
            ))
        );
    }

    // [utest->swdd~cli-uses-workload-states-to-sample-workload-to-instance-names~1]
    #[tokio::test]
    async fn utest_workload_names_to_instance_names_multiple_instance_names_for_one_workload() {
        let mut mock_server_connection = MockServerConnection::default();
        let workload_1_agent_a = generate_test_workload_spec_with_param(
            AGENT_A_NAME.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let workload_1_agent_b = generate_test_workload_spec_with_param(
            AGENT_B_NAME.to_string(),
            WORKLOAD_NAME_1.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let instance_name_wl_1_agent_a = workload_1_agent_a.instance_name.clone();
        let instance_name_wl_1_agent_b = workload_1_agent_b.instance_name.clone();

        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| {
                Ok(
                    ank_base::CompleteState::from(test_utils::generate_test_complete_state(vec![
                        workload_1_agent_a,
                        workload_1_agent_b,
                    ]))
                    .into(),
                )
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let workload_names = vec![WORKLOAD_NAME_1.to_string()];
        let result = cmd.workload_names_to_instance_names(workload_names).await;

        assert!(result.is_ok(), "Got result {:?}", result);
        let instance_names = result.unwrap();
        let expected_instance_names: BTreeSet<WorkloadInstanceName> =
            BTreeSet::from([instance_name_wl_1_agent_a, instance_name_wl_1_agent_b]);

        assert_eq!(instance_names, expected_instance_names);
    }
}

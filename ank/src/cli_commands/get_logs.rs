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
use crate::cli_error::CliError;
use crate::{cli::LogsArgs, cli_commands::server_connection::CompleteStateRequestDetails};

use ankaios_api::ank_base::{WorkloadInstanceNameSpec, WorkloadState};
use std::collections::{BTreeSet, HashMap};

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
            .map_err(|e| CliError::ExecutionError(format!("Failed to get logs: '{e:?}'")))
    }

    // [impl->swdd~cli-uses-workload-states-to-sample-workload-to-instance-names~1]
    async fn workload_names_to_instance_names(
        &mut self,
        workload_names: Vec<String>,
    ) -> Result<BTreeSet<WorkloadInstanceNameSpec>, CliError> {
        let request_details =
            CompleteStateRequestDetails::new(vec!["workloadStates".to_string()], false);
        let complete_state = self
            .server_connection
            .get_complete_state(request_details)
            .await?;

        if let Some(wl_states) = complete_state.workload_states {
            let available_instance_names: HashMap<String, BTreeSet<WorkloadInstanceNameSpec>> =
                Vec::<WorkloadState>::from(wl_states)
                    .into_iter()
                    .map(|wl_state| {
                        let instance_name: WorkloadInstanceNameSpec = match wl_state.instance_name {
                            Some(instance_name) => instance_name.try_into().map_err(|err| {
                                CliError::ExecutionError(format!(
                                    "Failed to convert instance name: {err}"
                                ))
                            })?,
                            None => {
                                return Err(CliError::ExecutionError(
                                    "Instance name is missing.".to_string(),
                                ));
                            }
                        };
                        let workload_name = instance_name.workload_name();
                        Ok((workload_name.to_owned(), instance_name))
                    })
                    .fold(HashMap::new(), |mut acc, item| {
                        if let Ok((workload_name, instance_name)) = item {
                            acc.entry(workload_name).or_default().insert(instance_name);
                        }
                        acc
                    });

            let mut converted_instance_names = BTreeSet::new();
            for wl_name in workload_names {
                if let Some(instance_names) = available_instance_names.get(&wl_name) {
                    for workload_instance_name in instance_names {
                        converted_instance_names.insert(workload_instance_name.clone());
                    }
                } else {
                    return Err(CliError::ExecutionError(format!(
                        "Workload name '{wl_name}' does not exist."
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
    use crate::cli::LogsArgs;
    use crate::cli_commands::{
        CliCommands,
        server_connection::{MockServerConnection, ServerConnectionError},
    };
    use crate::cli_error::CliError;

    use ankaios_api::ank_base::{CompleteState, WorkloadInstanceNameSpec};
    use ankaios_api::test_utils::{
        fixtures, generate_test_complete_state, generate_test_workload_named_with_params,
    };

    use mockall::predicate;
    use std::collections::BTreeSet;

    const NOT_EXISTING_WORKLOAD_NAME: &str = "non_existing_workload";

    // [utest->swdd~cli-provides-workload-logs~1]
    // [utest->swdd~cli-streams-logs-from-the-server~1]
    #[tokio::test]
    async fn utest_get_locks_blocking_success() {
        let log_workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );
        let cloned_log_workload = log_workload.clone();
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .withf(|request_details| {
                request_details.field_masks == vec!["workloadStates".to_string()]
                    && !request_details.subscribe_for_events
            })
            .return_once(|_| {
                Ok(CompleteState::from(generate_test_complete_state(vec![
                    cloned_log_workload,
                    generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[1],
                        fixtures::AGENT_NAMES[1],
                        fixtures::RUNTIME_NAMES[0],
                    ),
                ])))
            });

        let instance_names: BTreeSet<WorkloadInstanceNameSpec> =
            BTreeSet::from([log_workload.instance_name.clone()]);

        let args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        mock_server_connection
            .expect_stream_logs()
            .with(
                predicate::eq(instance_names),
                predicate::function(|args: &LogsArgs| {
                    args.workload_name == vec![fixtures::WORKLOAD_NAMES[0].to_string()]
                        && !args.follow
                        && args.tail == -1
                        && args.since.is_none()
                        && args.until.is_none()
                }),
            )
            .once()
            .return_once(|_, _| Ok(()));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };
        let result = cmd.get_logs_blocking(args).await;

        assert!(result.is_ok(), "Got result {result:?}");
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
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
        };

        mock_server_connection.expect_stream_logs().never();

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
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
                Ok(CompleteState::from(generate_test_complete_state(vec![
                    generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[0],
                        fixtures::AGENT_NAMES[0],
                        fixtures::RUNTIME_NAMES[0],
                    ),
                ])))
            });

        let args = LogsArgs {
            workload_name: vec![fixtures::WORKLOAD_NAMES[0].to_string()],
            follow: false,
            tail: -1,
            since: None,
            until: None,
            output_names: false,
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
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
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
                Ok(CompleteState::from(generate_test_complete_state(vec![
                    generate_test_workload_named_with_params(
                        fixtures::WORKLOAD_NAMES[0],
                        fixtures::AGENT_NAMES[0],
                        fixtures::RUNTIME_NAMES[0],
                    ),
                ])))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let workload_names = vec![NOT_EXISTING_WORKLOAD_NAME.to_string()];
        let result = cmd.workload_names_to_instance_names(workload_names).await;

        assert_eq!(
            result,
            Err(CliError::ExecutionError(format!(
                "Workload name '{NOT_EXISTING_WORKLOAD_NAME}' does not exist."
            )))
        );
    }

    // [utest->swdd~cli-uses-workload-states-to-sample-workload-to-instance-names~1]
    #[tokio::test]
    async fn utest_workload_names_to_instance_names_no_workload_states_available() {
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| Ok(CompleteState::default()));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let workload_names = vec![fixtures::WORKLOAD_NAMES[0].to_string()];
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
        let workload_1_agent_a = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            fixtures::RUNTIME_NAMES[0],
        );

        let workload_1_agent_b = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[1],
            fixtures::RUNTIME_NAMES[0],
        );

        let instance_name_wl_1_agent_a = workload_1_agent_a.instance_name.clone();
        let instance_name_wl_1_agent_b = workload_1_agent_b.instance_name.clone();

        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| {
                Ok(CompleteState::from(generate_test_complete_state(vec![
                    workload_1_agent_a,
                    workload_1_agent_b,
                ])))
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let workload_names = vec![fixtures::WORKLOAD_NAMES[0].to_string()];
        let result = cmd.workload_names_to_instance_names(workload_names).await;

        assert!(result.is_ok(), "Got result {result:?}");
        let instance_names = result.unwrap();
        let expected_instance_names: BTreeSet<WorkloadInstanceNameSpec> =
            BTreeSet::from([instance_name_wl_1_agent_a, instance_name_wl_1_agent_b]);

        assert_eq!(instance_names, expected_instance_names);
    }
}

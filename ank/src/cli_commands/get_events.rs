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
use crate::filtered_complete_state::{FilteredAlteredFields, FilteredCompleteState, FilteredEvent};
use crate::output;
use crate::output_debug;
use api::ank_base;
use chrono::Utc;

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

        let mut subscription = self
            .server_connection
            .subscribe_and_listen_for_events(object_field_mask)
            .await?;

        while let Some(event) = self
            .server_connection
            .receive_next_event(&mut subscription)
            .await?
        {
            Self::output_event(&event, &output_format)?;
        }

        Ok(())
    }

    fn output_event(
        event: &ank_base::CompleteStateResponse,
        output_format: &OutputFormat,
    ) -> Result<(), CliError> {
        let filtered_state: FilteredCompleteState = (*event).clone().into();

        let timestamp_str = Utc::now().to_rfc3339();

        let altered_fields = event
            .altered_fields
            .as_ref()
            .map(|af| FilteredAlteredFields {
                added_fields: af.added_fields.clone(),
                updated_fields: af.updated_fields.clone(),
                removed_fields: af.removed_fields.clone(),
            });

        let event_output = FilteredEvent {
            timestamp: timestamp_str,
            altered_fields,
            complete_state: Some(filtered_state),
        };

        let output = match output_format {
            OutputFormat::Yaml => serde_yaml::to_string(&event_output)
                .map_err(|err| CliError::ExecutionError(err.to_string()))?,
            OutputFormat::Json => serde_json::to_string_pretty(&event_output)
                .map_err(|err| CliError::ExecutionError(err.to_string()))?,
        };

        output!("{output}");

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
    use api::ank_base;
    use common::test_utils;
    use mockall::predicate::eq;

    use crate::{
        cli::OutputFormat,
        cli_commands::{CliCommands, server_connection::MockServerConnection},
    };

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    #[tokio::test]
    async fn utest_get_events_yaml_output() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_subscribe_and_listen_for_events()
            .with(eq(field_mask.clone()))
            .times(1)
            .returning(|_| {
                Ok(crate::cli_commands::server_connection::EventSubscription {
                    request_id: "test-request-id".to_string(),
                    initial_response_received: false,
                })
            });

        mock_server_connection
            .expect_receive_next_event()
            .times(2)
            .returning(move |_| {
                static mut CALL_COUNT: usize = 0;
                unsafe {
                    CALL_COUNT += 1;
                    if CALL_COUNT == 1 {
                        Ok(Some(ank_base::CompleteStateResponse {
                            complete_state: Some(test_utils::generate_test_proto_complete_state(
                                &[(
                                    "test_workload",
                                    test_utils::generate_test_proto_workload_with_param(
                                        "agent_A", "runtime",
                                    ),
                                )],
                            )),
                            altered_fields: Some(ank_base::AlteredFields {
                                added_fields: vec![
                                    "desiredState.workloads.test_workload".to_string(),
                                ],
                                updated_fields: vec![],
                                removed_fields: vec![],
                            }),
                        }))
                    } else {
                        Ok(None)
                    }
                }
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn utest_get_events_json_output() {
        let field_mask = vec!["workloadStates".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_subscribe_and_listen_for_events()
            .with(eq(field_mask.clone()))
            .times(1)
            .returning(|_| {
                Ok(crate::cli_commands::server_connection::EventSubscription {
                    request_id: "test-request-id".to_string(),
                    initial_response_received: false,
                })
            });

        mock_server_connection
            .expect_receive_next_event()
            .times(1)
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn utest_get_events_empty_field_mask() {
        let field_mask = vec![];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_subscribe_and_listen_for_events()
            .with(eq(field_mask.clone()))
            .times(1)
            .returning(|_| {
                Ok(crate::cli_commands::server_connection::EventSubscription {
                    request_id: "test-request-id".to_string(),
                    initial_response_received: false,
                })
            });

        mock_server_connection
            .expect_receive_next_event()
            .times(1)
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn utest_get_events_subscription_fails() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_subscribe_and_listen_for_events()
            .with(eq(field_mask.clone()))
            .times(1)
            .returning(|_| {
                Err(
                    crate::cli_commands::server_connection::ServerConnectionError::ExecutionError(
                        "Subscription failed".to_string(),
                    ),
                )
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_get_events_receive_event_fails() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_subscribe_and_listen_for_events()
            .with(eq(field_mask.clone()))
            .times(1)
            .returning(|_| {
                Ok(crate::cli_commands::server_connection::EventSubscription {
                    request_id: "test-request-id".to_string(),
                    initial_response_received: false,
                })
            });

        mock_server_connection
            .expect_receive_next_event()
            .times(1)
            .returning(|_| {
                Err(
                    crate::cli_commands::server_connection::ServerConnectionError::ExecutionError(
                        "Connection error".to_string(),
                    ),
                )
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn utest_get_events_multiple_events() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_subscribe_and_listen_for_events()
            .with(eq(field_mask.clone()))
            .times(1)
            .returning(|_| {
                Ok(crate::cli_commands::server_connection::EventSubscription {
                    request_id: "test-request-id".to_string(),
                    initial_response_received: false,
                })
            });

        mock_server_connection
            .expect_receive_next_event()
            .times(4)
            .returning(move |_| {
                static mut CALL_COUNT: usize = 0;
                unsafe {
                    CALL_COUNT += 1;
                    match CALL_COUNT {
                        1 => Ok(Some(ank_base::CompleteStateResponse {
                            complete_state: Some(test_utils::generate_test_proto_complete_state(
                                &[(
                                    "workload1",
                                    test_utils::generate_test_proto_workload_with_param(
                                        "agent_A", "runtime",
                                    ),
                                )],
                            )),
                            altered_fields: Some(ank_base::AlteredFields {
                                added_fields: vec!["desiredState.workloads.workload1".to_string()],
                                updated_fields: vec![],
                                removed_fields: vec![],
                            }),
                        })),
                        2 => Ok(Some(ank_base::CompleteStateResponse {
                            complete_state: Some(test_utils::generate_test_proto_complete_state(
                                &[(
                                    "workload2",
                                    test_utils::generate_test_proto_workload_with_param(
                                        "agent_B", "runtime",
                                    ),
                                )],
                            )),
                            altered_fields: Some(ank_base::AlteredFields {
                                added_fields: vec!["desiredState.workloads.workload2".to_string()],
                                updated_fields: vec![],
                                removed_fields: vec![],
                            }),
                        })),
                        3 => Ok(Some(ank_base::CompleteStateResponse {
                            complete_state: Some(test_utils::generate_test_proto_complete_state(
                                &[],
                            )),
                            altered_fields: Some(ank_base::AlteredFields {
                                added_fields: vec![],
                                updated_fields: vec![],
                                removed_fields: vec![
                                    "desiredState.workloads.workload1".to_string(),
                                ],
                            }),
                        })),
                        _ => Ok(None),
                    }
                }
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_ok());
    }

    #[test]
    fn utest_output_event_yaml_format() {
        let event = ank_base::CompleteStateResponse {
            complete_state: Some(test_utils::generate_test_proto_complete_state(&[(
                "test_workload",
                test_utils::generate_test_proto_workload_with_param("agent_A", "runtime"),
            )])),
            altered_fields: Some(ank_base::AlteredFields {
                added_fields: vec!["desiredState.workloads.test_workload".to_string()],
                updated_fields: vec![],
                removed_fields: vec![],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn utest_output_event_json_format() {
        let event = ank_base::CompleteStateResponse {
            complete_state: Some(test_utils::generate_test_proto_complete_state(&[(
                "test_workload",
                test_utils::generate_test_proto_workload_with_param("agent_A", "runtime"),
            )])),
            altered_fields: Some(ank_base::AlteredFields {
                added_fields: vec![],
                updated_fields: vec!["desiredState.workloads.test_workload.agent".to_string()],
                removed_fields: vec![],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Json);
        assert!(result.is_ok());
    }

    #[test]
    fn utest_output_event_no_altered_fields() {
        let event = ank_base::CompleteStateResponse {
            complete_state: Some(test_utils::generate_test_proto_complete_state(&[(
                "test_workload",
                test_utils::generate_test_proto_workload_with_param("agent_A", "runtime"),
            )])),
            altered_fields: None,
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn utest_output_event_empty_complete_state() {
        let event = ank_base::CompleteStateResponse {
            complete_state: None,
            altered_fields: Some(ank_base::AlteredFields {
                added_fields: vec![],
                updated_fields: vec![],
                removed_fields: vec!["desiredState.workloads.removed_workload".to_string()],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn utest_output_event_all_altered_fields_types() {
        let event = ank_base::CompleteStateResponse {
            complete_state: Some(test_utils::generate_test_proto_complete_state(&[(
                "test_workload",
                test_utils::generate_test_proto_workload_with_param("agent_A", "runtime"),
            )])),
            altered_fields: Some(ank_base::AlteredFields {
                added_fields: vec!["desiredState.workloads.new_workload".to_string()],
                updated_fields: vec!["desiredState.workloads.test_workload.agent".to_string()],
                removed_fields: vec!["desiredState.workloads.old_workload".to_string()],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Json);
        assert!(result.is_ok());
    }
}

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
use crate::output;
use crate::output_debug;

use ankaios_api::ank_base::{AlteredFields, CompleteState, CompleteStateResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct EventOutput {
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, flatten)]
    pub altered_fields: Option<AlteredFields>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub complete_state: Option<CompleteState>,
}

impl From<&CompleteStateResponse> for EventOutput {
    fn from(response: &CompleteStateResponse) -> Self {
        let timestamp_str = Utc::now().to_rfc3339();

        let filtered_state = (*response).clone().complete_state;

        let altered_fields = response.altered_fields.as_ref().map(|af| AlteredFields {
            added_fields: af.added_fields.clone(),
            updated_fields: af.updated_fields.clone(),
            removed_fields: af.removed_fields.clone(),
        });

        EventOutput {
            timestamp: timestamp_str,
            altered_fields,
            complete_state: filtered_state,
        }
    }
}

impl CliCommands {
    // [impl->swdd~cli-provides-get-events-command~1]
    // [impl->swdd~cli-receives-events~1]
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

        // [impl->swdd~cli-subscribes-for-events~1]
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

    // [impl->swdd~cli-outputs-events-with-timestamp~1]
    fn output_event(
        event: &CompleteStateResponse,
        output_format: &OutputFormat,
    ) -> Result<(), CliError> {

        let event_output: EventOutput = event.into();

        // [impl->swdd~cli-supports-multiple-output-types-for-events~1]
        match output_format {
            OutputFormat::Yaml => {
                let yaml_output = serde_yaml::to_string(&event_output)
                    .map_err(|err| CliError::ExecutionError(err.to_string()))?;
                output!("---\n{}", yaml_output.trim_end());
            }
            OutputFormat::Json => {
                let json_output = serde_json::to_string(&event_output)
                    .map_err(|err| CliError::ExecutionError(err.to_string()))?;
                output!("{json_output}");
            }
        };

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
    use crate::{
        cli::OutputFormat,
        cli_commands::{CliCommands, server_connection::MockServerConnection},
    };

    use ankaios_api::ank_base::{AlteredFields, CompleteStateResponse};
    use ankaios_api::test_utils::{
        fixtures, generate_test_complete_state, generate_test_workload_named,
        generate_test_workload_named_with_params,
    };

    use mockall::predicate::eq;

    // [utest->swdd~cli-provides-get-events-command~1]
    // [utest->swdd~cli-receives-events~1]
    // [utest->swdd~cli-supports-multiple-output-types-for-events~1]
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
                    request_id: fixtures::REQUEST_ID.to_string(),
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
                        Ok(Some(CompleteStateResponse {
                            complete_state: Some(
                                generate_test_complete_state(vec![generate_test_workload_named()])
                                    .into(),
                            ),
                            altered_fields: Some(AlteredFields {
                                added_fields: vec![format!(
                                    "desiredState.workloads.{}",
                                    fixtures::WORKLOAD_NAMES[0]
                                )],
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
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-provides-get-events-command~1]
    // [utest->swdd~cli-supports-multiple-output-types-for-events~1]
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
                    request_id: fixtures::REQUEST_ID.to_string(),
                    initial_response_received: false,
                })
            });

        mock_server_connection
            .expect_receive_next_event()
            .times(1)
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Json).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-subscribes-for-events~1]
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
                    request_id: fixtures::REQUEST_ID.to_string(),
                    initial_response_received: false,
                })
            });

        mock_server_connection
            .expect_receive_next_event()
            .times(1)
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-handles-event-subscription-errors~1]
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
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_err());
    }

    // [utest->swdd~cli-handles-event-subscription-errors~1]
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
                    request_id: fixtures::REQUEST_ID.to_string(),
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
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_err());
    }

    // [utest->swdd~cli-receives-events~1]
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
                    request_id: fixtures::REQUEST_ID.to_string(),
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
                        1 => Ok(Some(CompleteStateResponse {
                            complete_state: Some(
                                generate_test_complete_state(vec![
                                    generate_test_workload_named_with_params(
                                        fixtures::WORKLOAD_NAMES[0],
                                        fixtures::AGENT_NAMES[0],
                                        fixtures::RUNTIME_NAMES[0],
                                    ),
                                ])
                                .into(),
                            ),
                            altered_fields: Some(AlteredFields {
                                added_fields: vec![format!(
                                    "desiredState.workloads.{}",
                                    fixtures::WORKLOAD_NAMES[0]
                                )],
                                updated_fields: vec![],
                                removed_fields: vec![],
                            }),
                        })),
                        2 => Ok(Some(CompleteStateResponse {
                            complete_state: Some(
                                generate_test_complete_state(vec![
                                    generate_test_workload_named_with_params(
                                        fixtures::WORKLOAD_NAMES[1],
                                        fixtures::AGENT_NAMES[1],
                                        fixtures::RUNTIME_NAMES[0],
                                    ),
                                ])
                                .into(),
                            ),
                            altered_fields: Some(AlteredFields {
                                added_fields: vec![format!(
                                    "desiredState.workloads.{}",
                                    fixtures::WORKLOAD_NAMES[1]
                                )],
                                updated_fields: vec![],
                                removed_fields: vec![],
                            }),
                        })),
                        3 => Ok(Some(CompleteStateResponse {
                            complete_state: Some(generate_test_complete_state(vec![]).into()),
                            altered_fields: Some(AlteredFields {
                                added_fields: vec![],
                                updated_fields: vec![],
                                removed_fields: vec![format!(
                                    "desiredState.workloads.{}",
                                    fixtures::WORKLOAD_NAMES[0]
                                )],
                            }),
                        })),
                        _ => Ok(None),
                    }
                }
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-outputs-events-with-timestamp~1]
    // [utest->swdd~cli-supports-multiple-output-types-for-events~1]
    #[test]
    fn utest_output_event_yaml_format() {
        let event = CompleteStateResponse {
            complete_state: Some(
                generate_test_complete_state(vec![generate_test_workload_named_with_params(
                    fixtures::WORKLOAD_NAMES[0],
                    fixtures::AGENT_NAMES[0],
                    fixtures::RUNTIME_NAMES[0],
                )])
                .into(),
            ),
            altered_fields: Some(AlteredFields {
                added_fields: vec![format!(
                    "desiredState.workloads.{}",
                    fixtures::WORKLOAD_NAMES[0]
                )],
                updated_fields: vec![],
                removed_fields: vec![],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Yaml);
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-supports-multiple-output-types-for-events~1]
    // [utest->swdd~cli-outputs-events-with-timestamp~1]
    #[test]
    fn utest_output_event_json_format() {
        let event = CompleteStateResponse {
            complete_state: Some(
                generate_test_complete_state(vec![generate_test_workload_named()]).into(),
            ),
            altered_fields: Some(AlteredFields {
                added_fields: vec![],
                updated_fields: vec![format!(
                    "desiredState.workloads.{}.agent",
                    fixtures::WORKLOAD_NAMES[0]
                )],
                removed_fields: vec![],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Json);
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-outputs-events-with-timestamp~1]
    #[test]
    fn utest_output_event_no_altered_fields() {
        let event = CompleteStateResponse {
            complete_state: Some(
                generate_test_complete_state(vec![generate_test_workload_named()]).into(),
            ),
            altered_fields: None,
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Yaml);
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-outputs-events-with-timestamp~1]
    #[test]
    fn utest_output_event_empty_complete_state() {
        let event = CompleteStateResponse {
            complete_state: None,
            altered_fields: Some(AlteredFields {
                added_fields: vec![],
                updated_fields: vec![],
                removed_fields: vec!["desiredState.workloads.removed_workload".to_string()],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Yaml);
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-outputs-events-with-timestamp~1]
    #[test]
    fn utest_output_event_all_altered_fields_types() {
        let event = CompleteStateResponse {
            complete_state: Some(
                generate_test_complete_state(vec![generate_test_workload_named()]).into(),
            ),
            altered_fields: Some(AlteredFields {
                added_fields: vec!["desiredState.workloads.new_workload".to_string()],
                updated_fields: vec![format!(
                    "desiredState.workloads.{}.agent",
                    fixtures::WORKLOAD_NAMES[0]
                )],
                removed_fields: vec!["desiredState.workloads.old_workload".to_string()],
            }),
        };

        let result = CliCommands::output_event(&event, &OutputFormat::Json);
        assert!(result.is_ok());
    }
}

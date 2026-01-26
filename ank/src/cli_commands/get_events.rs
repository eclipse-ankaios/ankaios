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
use crate::output_debug;

use super::server_connection::CompleteStateRequestDetails;

#[cfg_attr(test, mockall_double::double)]
use event_output::EventSerializer;

impl CliCommands {
    // [impl->swdd~cli-provides-get-events-command~1]
    // [impl->swdd~cli-receives-events~1]
    pub async fn get_events(
        &mut self,
        object_field_mask: Vec<String>,
        output_format: OutputFormat,
        detailed: bool,
    ) -> Result<(), CliError> {
        output_debug!(
            "Got: object_field_mask: {:?}, output_format: {:?} ",
            object_field_mask,
            output_format
        );

        let request_details = CompleteStateRequestDetails::new(object_field_mask, true);

        let request_id = request_details.get_request_id().to_owned();

        // [impl->swdd~cli-subscribes-for-events~1]
        let initial_state_response = self
            .server_connection
            .get_complete_state(request_details)
            .await?;

        output_debug!("Received initial state response, subscription active");
        if detailed {
            EventSerializer::serialize(initial_state_response.into(), &output_format)?;
        }

        // [impl->swdd~cli-handles-event-subscription-errors~1]
        while let Some(event) = self
            .server_connection
            .receive_next_event(&request_id)
            .await?
        {
            EventSerializer::serialize(event.into(), &output_format)?;
        }

        Ok(())
    }
}

mod event_output {
    use ankaios_api::ank_base::{AlteredFields, CompleteState, CompleteStateResponse};
    use chrono::Utc;
    #[cfg(test)]
    use mockall::automock;
    use serde::{Deserialize, Serialize};

    use crate::{cli::OutputFormat, cli_error::CliError, output};

    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct EventOutput {
        pub timestamp: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default, flatten)]
        pub altered_fields: Option<AlteredFields>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default)]
        pub complete_state: Option<CompleteState>,
    }

    pub struct EventSerializer;

    #[cfg_attr(test, automock)]
    impl EventSerializer {
        // [impl->swdd~cli-outputs-events-with-timestamp~1]
        // [impl->swdd~cli-supports-multiple-output-types-for-events~1]
        pub fn serialize(
            event: EventOutput,
            output_format: &OutputFormat,
        ) -> Result<(), super::CliError> {
            match output_format {
                OutputFormat::Yaml => {
                    let yaml_output = serde_yaml::to_string(&event)
                        .map_err(|err| CliError::ExecutionError(err.to_string()))?;
                    output!("---\n{}", yaml_output.trim_end());
                }
                OutputFormat::Json => {
                    let json_output = serde_json::to_string(&event)
                        .map_err(|err| CliError::ExecutionError(err.to_string()))?;
                    output!("{json_output}");
                }
            };

            Ok(())
        }
    }

    impl From<CompleteStateResponse> for EventOutput {
        fn from(response: CompleteStateResponse) -> Self {
            let timestamp_str = Utc::now().to_rfc3339();

            let filtered_state = response.complete_state;

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

    impl From<CompleteState> for EventOutput {
        fn from(complete_state: CompleteState) -> Self {
            let timestamp_str = Utc::now().to_rfc3339();
            EventOutput {
                timestamp: timestamp_str,
                altered_fields: None,
                complete_state: Some(complete_state),
            }
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
    use crate::{
        cli::OutputFormat,
        cli_commands::{
            CliCommands,
            get_events::event_output::{EventSerializer, MockEventSerializer},
            server_connection::MockServerConnection,
        },
    };

    use ankaios_api::test_utils::{
        fixtures, generate_test_complete_state, generate_test_workload,
        generate_test_workload_named, generate_test_workload_named_with_params,
    };
    use ankaios_api::{
        ank_base::{AlteredFields, CompleteStateResponse},
        test_utils::generate_test_proto_complete_state,
    };

    // [utest->swdd~cli-provides-get-events-command~1]
    // [utest->swdd~cli-receives-events~1]
    // [utest->swdd~cli-supports-multiple-output-types-for-events~1]
    #[tokio::test]
    async fn utest_get_events_yaml_output() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let field_mask = vec!["desiredState.workloads".to_string()];

        let mock_event_serializer = MockEventSerializer::serialize_context();
        mock_event_serializer
            .expect()
            .once()
            .returning(|_, _| Ok(()));

        let mut mock_server_connection = MockServerConnection::default();

        let clone_field_mask = field_mask.clone();
        mock_server_connection
            .expect_get_complete_state()
            .withf(move |request_details| {
                request_details.field_masks == clone_field_mask
                    && request_details.subscribe_for_events
            })
            .times(1)
            .returning(|_| Ok(generate_test_proto_complete_state(&Vec::default())));

        mock_server_connection
            .expect_receive_next_event()
            .once()
            .returning(move |_| {
                Ok(Some(CompleteStateResponse {
                    complete_state: Some(
                        generate_test_complete_state(vec![generate_test_workload_named()]).into(),
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
            });

        mock_server_connection
            .expect_receive_next_event()
            .once()
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml, false).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-provides-get-events-command~1]
    // [utest->swdd~cli-supports-multiple-output-types-for-events~1]
    #[tokio::test]
    async fn utest_get_events_json_output() {
        let field_mask = vec!["workloadStates".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_get_complete_state()
            .times(1)
            .returning(|_| Ok(generate_test_proto_complete_state(&Vec::default())));

        mock_server_connection
            .expect_receive_next_event()
            .times(1)
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Json, false).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-subscribes-for-events~1]
    #[tokio::test]
    async fn utest_get_events_empty_field_mask() {
        let field_mask = vec![];

        let mut mock_server_connection = MockServerConnection::default();

        let clone_field_mask = field_mask.clone();
        mock_server_connection
            .expect_get_complete_state()
            .withf(move |request_details| {
                request_details.field_masks == clone_field_mask
                    && request_details.subscribe_for_events
            })
            .times(1)
            .returning(|_| {
                Ok(generate_test_proto_complete_state(&[(
                    fixtures::WORKLOAD_NAMES[0],
                    generate_test_workload().into(),
                )]))
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

        let result = cmd.get_events(field_mask, OutputFormat::Yaml, false).await;
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-handles-event-subscription-errors~1]
    #[tokio::test]
    async fn utest_get_events_subscription_fails() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_get_complete_state()
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

        let result = cmd.get_events(field_mask, OutputFormat::Yaml, false).await;
        assert!(result.is_err());
    }

    // [utest->swdd~cli-handles-event-subscription-errors~1]
    #[tokio::test]
    async fn utest_get_events_receive_event_fails() {
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_get_complete_state()
            .times(1)
            .returning(|_| Ok(generate_test_proto_complete_state(&Vec::default())));

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

        let result = cmd.get_events(field_mask, OutputFormat::Yaml, false).await;
        assert!(result.is_err());
    }

    // [utest->swdd~cli-receives-events~1]
    #[tokio::test]
    async fn utest_get_events_multiple_events() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let field_mask = vec!["desiredState.workloads".to_string()];

        let mock_event_serializer = MockEventSerializer::serialize_context();
        mock_event_serializer
            .expect()
            .times(3)
            .returning(|_, _| Ok(()));

        let mut event_seq = mockall::Sequence::new();

        let mut mock_server_connection = MockServerConnection::default();

        mock_server_connection
            .expect_get_complete_state()
            .once()
            .in_sequence(&mut event_seq)
            .returning(|_| Ok(generate_test_proto_complete_state(&Vec::default())));

        mock_server_connection
            .expect_receive_next_event()
            .once()
            .in_sequence(&mut event_seq)
            .returning(move |_| {
                Ok(Some(CompleteStateResponse {
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
                }))
            });

        mock_server_connection
            .expect_receive_next_event()
            .once()
            .in_sequence(&mut event_seq)
            .returning(move |_| {
                Ok(Some(CompleteStateResponse {
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
                }))
            });

        mock_server_connection
            .expect_receive_next_event()
            .once()
            .in_sequence(&mut event_seq)
            .returning(move |_| {
                Ok(Some(CompleteStateResponse {
                    complete_state: Some(generate_test_complete_state(vec![]).into()),
                    altered_fields: Some(AlteredFields {
                        added_fields: vec![],
                        updated_fields: vec![],
                        removed_fields: vec![format!(
                            "desiredState.workloads.{}",
                            fixtures::WORKLOAD_NAMES[0]
                        )],
                    }),
                }))
            });

        mock_server_connection
            .expect_receive_next_event()
            .once()
            .in_sequence(&mut event_seq)
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let result = cmd.get_events(field_mask, OutputFormat::Yaml, false).await;
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

        let result = EventSerializer::serialize(event.into(), &OutputFormat::Yaml);
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

        let result = EventSerializer::serialize(event.into(), &OutputFormat::Json);
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

        let result = EventSerializer::serialize(event.into(), &OutputFormat::Yaml);
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

        let result = EventSerializer::serialize(event.into(), &OutputFormat::Yaml);
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

        let result = EventSerializer::serialize(event.into(), &OutputFormat::Json);
        assert!(result.is_ok());
    }

    // [utest->swdd~cli-provides-get-events-command~1]
    #[tokio::test]
    async fn utest_get_events_output_initial_state_response() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let field_mask = vec!["desiredState.workloads".to_string()];

        let mock_event_serializer = MockEventSerializer::serialize_context();
        mock_event_serializer
            .expect()
            .once()
            .returning(|_, _| Ok(()));

        let mut mock_server_connection = MockServerConnection::default();

        let clone_field_mask = field_mask.clone();
        mock_server_connection
            .expect_get_complete_state()
            .withf(move |request_details| {
                request_details.field_masks == clone_field_mask
                    && request_details.subscribe_for_events
            })
            .times(1)
            .returning(|_| Ok(generate_test_proto_complete_state(&Vec::default())));

        mock_server_connection
            .expect_receive_next_event()
            .once()
            .returning(|_| Ok(None));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let output_initial_state_response = true;

        let result = cmd
            .get_events(
                field_mask,
                OutputFormat::Yaml,
                output_initial_state_response,
            )
            .await;
        assert!(result.is_ok());
    }
}

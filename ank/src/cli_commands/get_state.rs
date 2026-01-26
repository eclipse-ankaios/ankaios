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
    cli::OutputFormat, cli_commands::server_connection::CompleteStateRequestDetails,
    cli_error::CliError, output_debug,
};

use super::CliCommands;

impl CliCommands {
    pub async fn get_state(
        &mut self,
        object_field_mask: Vec<String>,
        output_format: OutputFormat,
    ) -> Result<String, CliError> {
        output_debug!(
            "Got: object_field_mask={:?} output_format={:?}",
            object_field_mask,
            output_format
        );

        // [impl->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
        let request_details = CompleteStateRequestDetails::new(object_field_mask, false);
        let filtered_complete_state = self
            .server_connection
            .get_complete_state(request_details)
            .await?;

        output_debug!("Raw complete state: {:?}", filtered_complete_state);

        match output_format {
            // [impl -> swdd~cli-shall-support-desired-state-yaml~1]
            OutputFormat::Yaml => Ok(serde_yaml::to_string(&filtered_complete_state)?),
            // [impl -> swdd~cli-shall-support-desired-state-json~1]
            OutputFormat::Json => Ok(serde_json::to_string_pretty(&filtered_complete_state)?),
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
        cli_commands::{CliCommands, server_connection::MockServerConnection},
    };

    use ankaios_api::ank_base::{CompleteState, Workload};
    use ankaios_api::test_utils::{
        fixtures, generate_test_proto_complete_state, generate_test_workload_with_params,
    };

    fn generate_test_data() -> CompleteState {
        generate_test_proto_complete_state(&[
            (
                fixtures::WORKLOAD_NAMES[0],
                generate_test_workload_with_params(
                    fixtures::AGENT_NAMES[0],
                    fixtures::RUNTIME_NAMES[0],
                )
                .into(),
            ),
            (
                fixtures::WORKLOAD_NAMES[1],
                generate_test_workload_with_params(
                    fixtures::AGENT_NAMES[1],
                    fixtures::RUNTIME_NAMES[0],
                )
                .into(),
            ),
            (
                fixtures::WORKLOAD_NAMES[2],
                generate_test_workload_with_params(
                    fixtures::AGENT_NAMES[1],
                    fixtures::RUNTIME_NAMES[0],
                )
                .into(),
            ),
        ])
    }

    // [utest -> swdd~cli-returns-desired-state-from-server~1]
    // [utest -> swdd~cli-shall-support-desired-state-yaml~1]
    // [utest->swdd~cli-blocks-until-ankaios-server-responds-get-desired-state~1]
    // [utest->swdd~cli-provides-get-desired-state~1]
    #[tokio::test]
    async fn utest_get_state_complete_desired_state_yaml() {
        let test_data = generate_test_data();

        let mut mock_server_connection = MockServerConnection::default();
        let test_data_clone = test_data.clone();
        mock_server_connection
            .expect_get_complete_state()
            .withf(|request_details| {
                request_details.field_masks.is_empty() && !request_details.subscribe_for_events
            })
            .return_once(|_| Ok(test_data_clone));
        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd.get_state(vec![], OutputFormat::Yaml).await.unwrap();
        let expected_text = serde_yaml::to_string(&test_data).unwrap();
        assert_eq!(cmd_text, expected_text);
    }

    // [utest -> swdd~cli-shall-support-desired-state-json~1]
    #[tokio::test]
    async fn utest_get_state_complete_desired_state_json() {
        let test_data = generate_test_data();

        let mut mock_server_connection = MockServerConnection::default();
        let cloned_test_data = test_data.clone();
        mock_server_connection
            .expect_get_complete_state()
            .return_once(|_| Ok(cloned_test_data));

        let mut cmd = CliCommands {
            _response_timeout_ms: 0,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd.get_state(vec![], OutputFormat::Json).await.unwrap();

        let expected_text = serde_json::to_string_pretty(&test_data).unwrap();
        assert_eq!(cmd_text, expected_text);
    }

    // [utest -> swdd~cli-returns-desired-state-from-server~1]
    // [utest->swdd~cli-returns-api-version-with-desired-state~1]
    #[tokio::test]
    async fn utest_get_state_single_field_of_desired_state() {
        let test_data = generate_test_data();

        let test_data_clone = test_data.clone();
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .withf(|request_details| {
                request_details.field_masks
                    == vec![format!(
                        "desiredState.workloads.{}.runtime",
                        fixtures::WORKLOAD_NAMES[2]
                    )]
                    && !request_details.subscribe_for_events
            })
            .return_once(|_| Ok(test_data_clone));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd
            .get_state(
                vec![format!(
                    "desiredState.workloads.{}.runtime",
                    fixtures::WORKLOAD_NAMES[2]
                )],
                OutputFormat::Yaml,
            )
            .await
            .unwrap();

        let expected_single_field_result_text = serde_yaml::to_string(&test_data).unwrap();

        assert_eq!(cmd_text, expected_single_field_result_text);
    }

    // [utest->swdd~cli-provides-object-field-mask-arg-to-get-partial-desired-state~1]
    // [utest->swdd~cli-returns-compact-state-object-when-object-field-mask-provided~1]
    // [utest->swdd~cli-returns-api-version-with-desired-state~1]
    #[tokio::test]
    async fn utest_get_state_multiple_fields_of_desired_state() {
        let test_data = generate_test_data();

        let test_data_clone = test_data.clone();
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .withf(|request_details| {
                request_details.field_masks
                    == vec![
                        format!(
                            "desiredState.workloads.{}.runtime",
                            fixtures::WORKLOAD_NAMES[0]
                        ),
                        format!(
                            "desiredState.workloads.{}.runtime",
                            fixtures::WORKLOAD_NAMES[1]
                        ),
                    ]
                    && !request_details.subscribe_for_events
            })
            .return_once(|_| Ok(test_data_clone));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd
            .get_state(
                vec![
                    format!(
                        "desiredState.workloads.{}.runtime",
                        fixtures::WORKLOAD_NAMES[0]
                    ),
                    format!(
                        "desiredState.workloads.{}.runtime",
                        fixtures::WORKLOAD_NAMES[1]
                    ),
                ],
                OutputFormat::Yaml,
            )
            .await
            .unwrap();
        let expected_text = serde_yaml::to_string(&test_data).unwrap();
        assert_eq!(cmd_text, expected_text);
    }

    #[tokio::test]
    async fn utest_get_state_single_field_without_api_version() {
        let test_data = generate_test_proto_complete_state(&[("", Workload::default())]);

        let mut mock_server_connection = MockServerConnection::default();
        let test_data_clone = test_data.clone();
        mock_server_connection
            .expect_get_complete_state()
            .withf(|request_details| {
                request_details.field_masks == vec!["workloadStates".to_owned()]
                    && !request_details.subscribe_for_events
            })
            .return_once(|_| Ok(test_data_clone));
        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: false,
            server_connection: mock_server_connection,
        };

        let cmd_text = cmd
            .get_state(vec!["workloadStates".to_owned()], OutputFormat::Yaml)
            .await
            .unwrap();

        let expected_text = serde_yaml::to_string(&test_data).unwrap();
        assert_eq!(cmd_text, expected_text);
    }
}

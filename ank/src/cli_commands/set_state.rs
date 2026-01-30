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

use super::CliCommands;

use ankaios_api::ank_base::CompleteState;
use common::state_manipulation::Object;
use std::io::{self, Read};

#[cfg(not(test))]
fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
use crate::{cli_error::CliError, output_debug};
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

// [impl->swdd~cli-supports-yaml-to-set-desired-state~1]
async fn process_inputs<R: Read>(reader: R, state_object_file: &str) -> Result<Object, CliError> {
    match state_object_file {
        "-" => {
            let stdin = io::read_to_string(reader).map_err(|error| {
                CliError::ExecutionError(format!(
                    "Could not read the state object from stdin.\nError: '{error}'"
                ))
            })?;
            let value: serde_yaml::Value = serde_yaml::from_str(&stdin).map_err(|error| {
                CliError::YamlSerialization(format!(
                    "Could not convert stdin input to yaml.\nError: '{error}'"
                ))
            })?;
            Ok(value.into())
        }
        _ => {
            let state_object_data =
                read_file_to_string(state_object_file.to_string()).map_err(|error| {
                    CliError::ExecutionError(format!(
                        "Could not read the state object file '{state_object_file}'.\nError: '{error}'"
                    ))
                })?;
            let value: serde_yaml::Value =
                serde_yaml::from_str(&state_object_data).map_err(|error| {
                    CliError::YamlSerialization(format!(
                        "Could not convert state object file to yaml.\nError: '{error}'"
                    ))
                })?;
            Ok(value.into())
        }
    }
}

impl CliCommands {
    // [impl->swdd~cli-provides-set-desired-state~1]
    pub async fn set_state(
        &mut self,
        object_field_mask: Vec<String>,
        state_object_file: String,
    ) -> Result<(), CliError> {
        output_debug!(
            "Got: object_field_mask={:?} state_object_file={:?}",
            object_field_mask,
            state_object_file
        );

        let x = process_inputs(io::stdin(), &state_object_file).await?;

        let new_complete_state: CompleteState = x.try_into()?;

        output_debug!(
            "Send UpdateState request with the CompleteState {:?}",
            new_complete_state
        );

        // [impl->swdd~cli-blocks-until-ankaios-server-responds-set-desired-state~2]
        self.update_state_and_wait_for_complete(new_complete_state, object_field_mask)
            .await
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
    use super::{CliCommands, io, process_inputs};
    use crate::cli_commands::server_connection::MockServerConnection;
    use ankaios_api::CURRENT_API_VERSION;
    use ankaios_api::ank_base::{
        CompleteState, RestartPolicy, State, UpdateStateSuccess, Workload, WorkloadMap,
    };
    use ankaios_api::test_utils::fixtures;
    use mockall::predicate::eq;
    use serde_yaml::Value;
    use std::{collections::HashMap, io::Cursor};

    pub fn read_to_string_mock(_file: String) -> io::Result<String> {
        Ok(_file)
    }

    const SAMPLE_CONFIG: &str = r#"desiredState:
        apiVersion: v1
        workloads:
          nginx:
            agent: agent_A
            tags:
                owner: Ankaios team
            dependencies: {}
            restartPolicy: ALWAYS
            runtime: podman
            runtimeConfig: |
              image: docker.io/nginx:latest
              commandOptions: ["-p", "8081:80"]"#;

    // [utest->swdd~cli-supports-yaml-to-set-desired-state~1]
    #[tokio::test]
    async fn utest_process_inputs_stdin() {
        let input = SAMPLE_CONFIG;
        let reader = Cursor::new(input);
        let state_object_file = "-".to_string();

        let temp_obj = process_inputs(reader, &state_object_file).await.unwrap();

        let value: Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let expected_obj = value.into();

        assert_eq!(temp_obj, expected_obj);
    }

    // [utest->swdd~cli-supports-yaml-to-set-desired-state~1]
    #[tokio::test]
    async fn utest_process_inputs_file() {
        let state_object_file = SAMPLE_CONFIG.to_owned();

        let temp_obj = process_inputs(io::empty(), &state_object_file)
            .await
            .unwrap();

        let value: Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let expected_obj = value.into();

        assert_eq!(temp_obj, expected_obj);
    }

    // [utest->swdd~cli-supports-yaml-to-set-desired-state~1]
    #[tokio::test]
    async fn utest_process_inputs_invalid_yaml() {
        let input = "invalid yaml";
        let reader = Cursor::new(input);
        let state_object_file = "-".to_string();

        let temp_obj = process_inputs(reader, &state_object_file).await;

        assert!(temp_obj.is_ok());
    }

    // [utest->swdd~cli-provides-set-desired-state~1]
    #[tokio::test]
    async fn utest_set_state_ok() {
        let workload = Workload {
            restart_policy: Some(RestartPolicy::Always as i32),
            ..Default::default()
        };
        let updated_state = CompleteState {
            desired_state: Some(State {
                api_version: CURRENT_API_VERSION.to_string(),
                workloads: Some(WorkloadMap {
                    workloads: HashMap::from([("nginx".to_string(), workload)]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let update_mask = vec!["desiredState.workloads.nginx.restartPolicy".to_string()];
        let state_object_file = serde_yaml::to_string(&updated_state).unwrap();

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .returning(|_| Ok(CompleteState::default()));
        mock_server_connection
            .expect_update_state()
            .with(eq(updated_state), eq(update_mask.clone()))
            .return_once(|_, _| Ok(UpdateStateSuccess::default()));

        let mut cmd = CliCommands {
            _response_timeout_ms: fixtures::RESPONSE_TIMEOUT_MS,
            no_wait: true,
            server_connection: mock_server_connection,
        };

        cmd.set_state(update_mask, state_object_file).await.unwrap();
    }
}

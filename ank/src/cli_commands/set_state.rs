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

use common::{
    objects::{CompleteState, StoredWorkloadSpec},
    state_manipulation::{Object, Path},
};
use std::io::{self, Read};

#[cfg(not(test))]
fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
use crate::{cli_error::CliError, output_debug};
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

use super::CliCommands;

fn create_state_with_default_workload_specs(update_mask: &[String]) -> CompleteState {
    let mut complete_state = CompleteState::default();
    const WORKLOAD_ATTRIBUTE_LEVEL: usize = 4;
    let workload_level_mask_parts = ["desiredState".to_string(), "workloads".to_string()];
    const WORKLOAD_NAME_POSITION: usize = 2;

    for field_mask in update_mask {
        let path: Path = field_mask.into();

        // if we want to set an attribute of a workload create a default object for the workload
        let mask_parts = path.parts();
        if mask_parts.len() >= WORKLOAD_ATTRIBUTE_LEVEL
            && mask_parts.starts_with(&workload_level_mask_parts)
        {
            complete_state.desired_state.workloads.insert(
                mask_parts[WORKLOAD_NAME_POSITION].to_string(),
                StoredWorkloadSpec::default(),
            );
        }
    }

    complete_state
}

// [impl->swdd~cli-supports-yaml-to-set-desired-state~1]
async fn process_inputs<R: Read>(reader: R, state_object_file: &str) -> Result<Object, CliError> {
    match state_object_file {
        "-" => {
            let stdin = io::read_to_string(reader).map_err(|error| {
                CliError::ExecutionError(format!(
                    "Could not read the state object from stdin.\nError: '{}'",
                    error
                ))
            })?;
            let value: serde_yaml::Value = serde_yaml::from_str(&stdin).map_err(|error| {
                CliError::YamlSerialization(format!(
                    "Could not convert stdin input to yaml.\nError: '{}'",
                    error
                ))
            })?;
            Ok(Object::try_from(&value)?)
        }
        _ => {
            let state_object_data =
                read_file_to_string(state_object_file.to_string()).map_err(|error| {
                    CliError::ExecutionError(format!(
                        "Could not read the state object file '{}'.\nError: '{}'",
                        state_object_file, error
                    ))
                })?;
            let value: serde_yaml::Value =
                serde_yaml::from_str(&state_object_data).map_err(|error| {
                    CliError::YamlSerialization(format!(
                        "Could not convert state object file to yaml.\nError: '{}'",
                        error
                    ))
                })?;
            Ok(Object::try_from(&value)?)
        }
    }
}

fn overwrite_using_field_mask(
    mut complete_state_object: Object,
    object_field_mask: &Vec<String>,
    temp_obj: &Object,
) -> Result<Object, CliError> {
    for field_mask in object_field_mask {
        let path: Path = field_mask.into();
        if let Some(value) = temp_obj.get(&path) {
            complete_state_object
                .set(&path, value.clone())
                .map_err(|err| CliError::ExecutionError(err.to_string()))?;
        }
    }

    Ok(complete_state_object)
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

        let temp_obj = process_inputs(io::stdin(), &state_object_file).await?;
        let default_complete_state = create_state_with_default_workload_specs(&object_field_mask);

        // now overwrite with the values from the field mask
        let mut complete_state_object: Object = default_complete_state.try_into()?;
        complete_state_object =
            overwrite_using_field_mask(complete_state_object, &object_field_mask, &temp_obj)?;
        let new_complete_state = complete_state_object.try_into()?;

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
    use super::{
        create_state_with_default_workload_specs, io, overwrite_using_field_mask, process_inputs,
        CliCommands, StoredWorkloadSpec,
    };
    use crate::{
        cli_commands::server_connection::MockServerConnection,
        filtered_complete_state::FilteredCompleteState,
    };
    use api::ank_base::UpdateStateSuccess;
    use common::{
        objects::{CompleteState, RestartPolicy, State},
        state_manipulation::Object,
    };
    use mockall::predicate::eq;
    use serde_yaml::Value;
    use std::{collections::HashMap, io::Cursor};

    pub fn read_to_string_mock(_file: String) -> io::Result<String> {
        Ok(_file)
    }

    const RESPONSE_TIMEOUT_MS: u64 = 3000;

    const SAMPLE_CONFIG: &str = r#"desiredState:
        workloads:
          nginx:
            agent: agent_A
            tags:
            - key: owner
              value: Ankaios team
            dependencies: {}
            restartPolicy: ALWAYS
            runtime: podman
            runtimeConfig: |
              image: docker.io/nginx:latest
              commandOptions: ["-p", "8081:80"]"#;

    // [utest->swdd~cli-provides-set-desired-state~1]
    #[test]
    fn utest_create_state_with_default_workload_specs_empty_update_mask() {
        let update_mask = vec![];

        let complete_state = create_state_with_default_workload_specs(&update_mask);

        assert!(complete_state.desired_state.workloads.is_empty());
    }

    // [utest->swdd~cli-provides-set-desired-state~1]
    #[test]
    fn utest_create_state_with_default_workload_specs_with_update_mask() {
        let update_mask = vec![
            "desiredState.workloads.nginx.restartPolicy".to_string(),
            "desiredState.workloads.nginx2.restartPolicy".to_string(),
            "desiredState.workloads.nginx3".to_string(),
        ];

        let complete_state = create_state_with_default_workload_specs(&update_mask);

        assert_eq!(
            complete_state.desired_state.workloads.get("nginx"),
            Some(&StoredWorkloadSpec::default())
        );

        assert_eq!(
            complete_state.desired_state.workloads.get("nginx2"),
            Some(&StoredWorkloadSpec::default())
        );
        assert!(!complete_state
            .desired_state
            .workloads
            .contains_key("nginx3"));
    }

    // [utest->swdd~cli-provides-set-desired-state~1]
    #[test]
    fn utest_create_state_with_default_workload_specs_invalid_path() {
        let update_mask = vec!["invalid.path".to_string()];

        let complete_state = create_state_with_default_workload_specs(&update_mask);

        assert!(complete_state.desired_state.workloads.is_empty());
    }

    // [utest->swdd~cli-provides-set-desired-state~1]
    #[test]
    fn utest_overwrite_using_field_mask() {
        let workload_spec = StoredWorkloadSpec::default();
        let mut complete_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([("nginx".to_string(), workload_spec)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut complete_state_object: Object = complete_state.try_into().unwrap();
        let value: serde_yaml::Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let temp_object = Object::try_from(&value).unwrap();
        let update_mask = vec!["desiredState.workloads.nginx".to_string()];

        complete_state_object =
            overwrite_using_field_mask(complete_state_object, &update_mask, &temp_object).unwrap();

        complete_state = complete_state_object.try_into().unwrap();

        assert!(complete_state.desired_state.workloads.contains_key("nginx"));
        assert_eq!(
            complete_state
                .desired_state
                .workloads
                .get("nginx")
                .unwrap()
                .restart_policy,
            RestartPolicy::Always
        )
    }

    // [utest->swdd~cli-supports-yaml-to-set-desired-state~1]
    #[tokio::test]
    async fn utest_process_inputs_stdin() {
        let input = SAMPLE_CONFIG;
        let reader = Cursor::new(input);
        let state_object_file = "-".to_string();

        let temp_obj = process_inputs(reader, &state_object_file).await.unwrap();

        let value: Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let expected_obj = Object::try_from(&value).unwrap();

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
        let expected_obj = Object::try_from(&value).unwrap();

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
        let update_mask = vec!["desiredState.workloads.nginx.restartPolicy".to_string()];
        let state_object_file = SAMPLE_CONFIG.to_owned();

        let workload_spec = StoredWorkloadSpec {
            restart_policy: RestartPolicy::Always,
            ..Default::default()
        };
        let updated_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([("nginx".to_string(), workload_spec)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_get_complete_state()
            .returning(|_| Ok(FilteredCompleteState::default()));
        mock_server_connection
            .expect_update_state()
            .with(eq(updated_state), eq(update_mask.clone()))
            .return_once(|_, _| Ok(UpdateStateSuccess::default()));

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: true,
            server_connection: mock_server_connection,
        };

        let set_state_result = cmd.set_state(update_mask, state_object_file).await;
        assert!(set_state_result.is_ok());
    }
}

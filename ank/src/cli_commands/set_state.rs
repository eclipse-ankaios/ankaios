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
async fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}

#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

use crate::{cli_error::CliError, output_and_error, output_debug};

use super::CliCommands;

fn add_default_workload_spec_per_update_mask(
    update_mask: &Vec<String>,
    complete_state: &mut CompleteState,
) {
    for field_mask in update_mask {
        let path: Path = field_mask.into();

        // if we want to set an attribute of a workload create a default object for the workload
        if path.parts().len() >= 4
            && path.parts()[0] == "desiredState"
            && path.parts()[1] == "workloads"
        {
            let stored_workload = StoredWorkloadSpec {
                agent: "".to_string(),
                runtime: "".to_string(),
                runtime_config: "".to_string(),
                ..Default::default()
            };

            complete_state
                .desired_state
                .workloads
                .insert(path.parts()[2].to_string(), stored_workload);
        }
    }
}

async fn process_inputs<R: Read>(reader: R, state_object_file: &String, temp_obj: &mut Object) {
    match state_object_file.as_str() {
        "-" => {
            let stdin = io::read_to_string(reader).unwrap_or_else(|error| {
                output_and_error!("Could not read the state object file.\nError: {}", error)
            });
            let value: serde_yaml::Value = serde_yaml::from_str(&stdin).unwrap();
            *temp_obj = Object::try_from(&value).unwrap();
        }
        _ => {
            let state_object_data = read_file_to_string(state_object_file.clone())
                .await
                .unwrap_or_else(|error| {
                    output_and_error!("Could not read the state object file.\nError: {}", error)
                });
            let value: serde_yaml::Value = serde_yaml::from_str(&state_object_data).unwrap();
            *temp_obj = Object::try_from(&value).unwrap();
        }
    }
}

fn overwrite_using_field_mask(
    complete_state_object: &mut Object,
    object_field_mask: &Vec<String>,
    temp_obj: &Object,
) {
    for field_mask in object_field_mask {
        let path: Path = field_mask.into();

        complete_state_object
            .set(
                &path,
                temp_obj
                    .get(&path)
                    .ok_or(CliError::ExecutionError(format!(
                        "Specified update mask '{field_mask}' not found in the input config.",
                    )))
                    .unwrap()
                    .clone(),
            )
            .map_err(|err| CliError::ExecutionError(err.to_string()))
            .unwrap();
    }
}

impl CliCommands {
    pub async fn set_state(
        &mut self,
        object_field_mask: Vec<String>,
        state_object_file: Option<String>,
    ) -> Result<(), CliError> {
        output_debug!(
            "Got: object_field_mask={:?} state_object_file={:?}",
            object_field_mask,
            state_object_file
        );

        let mut complete_state = CompleteState::default();
        let mut temp_obj: Object = Object::default();

        if let Some(state_object_file) = state_object_file {
            process_inputs(io::stdin(), &state_object_file, &mut temp_obj).await;

            // This here is a workaround for the default workload specs
            add_default_workload_spec_per_update_mask(&object_field_mask, &mut complete_state);

            // now overwrite with the values from the field mask
            let mut complete_state_object: Object = complete_state.try_into()?;
            overwrite_using_field_mask(&mut complete_state_object, &object_field_mask, &temp_obj);
            complete_state = complete_state_object.try_into()?;
        }

        output_debug!(
            "Send UpdateState request with the CompleteState {:?}",
            complete_state
        );

        // [impl->swdd~cli-blocks-until-ankaios-server-responds-set-desired-state~2]
        self.update_state_and_wait_for_complete(complete_state, object_field_mask)
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
    use super::*;
    use crate::cli_commands::server_connection::MockServerConnection;
    use common::{
        commands::UpdateStateSuccess,
        objects::{CompleteState, RestartPolicy, State},
        state_manipulation::Object,
    };
    use mockall::predicate::eq;
    use serde_yaml::Value;
    use std::{collections::HashMap, io::Cursor};

    pub async fn read_to_string_mock(_file: String) -> io::Result<String> {
        Ok(_file.into())
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

    #[test]
    fn utest_add_default_workload_spec_empty_update_mask() {
        let update_mask = vec![];
        let mut complete_state = CompleteState {
            desired_state: Default::default(),
            ..Default::default()
        };

        add_default_workload_spec_per_update_mask(&update_mask, &mut complete_state);

        assert!(complete_state.desired_state.workloads.is_empty());
    }

    #[test]
    fn utest_add_default_workload_spec_with_update_mask() {
        let update_mask = vec![
            "desiredState.workloads.nginx.restartPolicy".to_string(),
            "desiredState.workloads.nginx2.restartPolicy".to_string(),
            "desiredState.workloads.nginx2".to_string(),
        ];
        let mut complete_state = CompleteState {
            ..Default::default()
        };

        add_default_workload_spec_per_update_mask(&update_mask, &mut complete_state);

        assert!(complete_state.desired_state.workloads.contains_key("nginx"));
        assert!(complete_state
            .desired_state
            .workloads
            .contains_key("nginx2"));
        assert!(!complete_state
            .desired_state
            .workloads
            .contains_key("nginx3"));
    }

    #[test]
    fn utest_add_default_workload_spec_invalid_path() {
        let update_mask = vec!["invalid.path".to_string()];
        let mut complete_state = CompleteState {
            desired_state: Default::default(),
            ..Default::default()
        };

        add_default_workload_spec_per_update_mask(&update_mask, &mut complete_state);

        assert!(complete_state.desired_state.workloads.is_empty());
    }

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

        overwrite_using_field_mask(&mut complete_state_object, &update_mask, &temp_object);

        complete_state = complete_state_object.try_into().unwrap();

        assert!(complete_state.desired_state.workloads.contains_key("nginx"));
        assert!(
            complete_state
                .desired_state
                .workloads
                .get("nginx")
                .unwrap()
                .restart_policy
                == RestartPolicy::Always
        )
    }

    #[tokio::test]
    async fn utest_process_inputs_stdin() {
        let input = SAMPLE_CONFIG;
        let reader = Cursor::new(input);
        let state_object_file = "-".to_string();
        let mut temp_obj = Object::default();

        process_inputs(reader, &state_object_file, &mut temp_obj).await;

        let value: Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let expected_obj = Object::try_from(&value).unwrap();

        assert_eq!(temp_obj, expected_obj);
    }

    #[tokio::test]
    async fn utest_process_inputs_file() {
        let state_object_file = SAMPLE_CONFIG.to_owned();
        let mut temp_obj = Object::default();

        process_inputs(io::empty(), &state_object_file, &mut temp_obj).await;

        let value: Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let expected_obj = Object::try_from(&value).unwrap();

        assert_eq!(temp_obj, expected_obj);
    }

    #[tokio::test]
    async fn utest_process_inputs_invalid_yaml() {
        let input = "invalid yaml";
        let reader = Cursor::new(input);
        let state_object_file = "-".to_string();
        let mut temp_obj = Object::default();

        process_inputs(reader, &state_object_file, &mut temp_obj).await;
    }

    /// I NEED SOME HELP ON THIS UTEST
    #[tokio::test]
    async fn utest_set_state_ok() {
        let update_mask = vec!["desiredState.workloads.nginx.restartPolicy".to_string()];
        let state_object_file = Some(SAMPLE_CONFIG.to_owned());

        let mut workload_spec = StoredWorkloadSpec::default();
        workload_spec.restart_policy = RestartPolicy::Always;
        let updated_state = CompleteState {
            desired_state: State {
                workloads: HashMap::from([("nginx".to_string(), workload_spec)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .with(eq(updated_state), eq(update_mask.clone()))
            .return_once(|_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![],
                    deleted_workloads: vec![],
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: true,
            server_connection: mock_server_connection,
        };

        let set_state_result = cmd.set_state(update_mask, state_object_file).await;
        assert!(set_state_result.is_ok());
    }
}

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

#[cfg(not(test))]
fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
use crate::{cli_error::CliError, output_and_error, output_debug};
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

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
        if let Some(state_object_file) = state_object_file {
            let state_object_data =
                read_file_to_string(state_object_file).unwrap_or_else(|error| {
                    output_and_error!("Could not read the state object file.\nError: {}", error)
                });
            let value: serde_yaml::Value = serde_yaml::from_str(&state_object_data)?;
            let x = Object::try_from(&value)?;

            // This here is a workaround for the default workload specs
            add_default_workload_spec_per_update_mask(&object_field_mask, &mut complete_state);

            // now overwrite with the values from the field mask
            let mut complete_state_object: Object = complete_state.try_into()?;
            for field_mask in &object_field_mask {
                let path: Path = field_mask.into();

                complete_state_object
                    .set(
                        &path,
                        x.get(&path)
                            .ok_or(CliError::ExecutionError(format!(
                                "Specified update mask '{field_mask}' not found in the input config.",
                            )))?
                            .clone(),
                    )
                    .map_err(|err| CliError::ExecutionError(err.to_string()))?;
            }
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
    use std::io;

    use common::{
        commands::UpdateStateSuccess, objects::generate_test_workload_spec_with_param,
        test_utils::generate_test_complete_state,
    };

    const RESPONSE_TIMEOUT_MS: u64 = 100;

    use super::*;
    use crate::cli_commands::server_connection::MockServerConnection;

    pub fn read_to_string_mock(_file: String) -> io::Result<String> {
        let new_workload = generate_test_workload_spec_with_param(
            "agent_A".to_string(),
            "name1".to_string(),
            "runtime".to_string(),
        );
        let new_complete_state = generate_test_complete_state(vec![new_workload]);

        let complete_state_file_content =
            serde_yaml::to_string(&new_complete_state).unwrap_or_default();
        Ok(complete_state_file_content)
    }

    // [utest->swdd~cli-provides-set-desired-state~1]
    #[tokio::test]
    async fn utest_set_desired_state() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_workload = generate_test_workload_spec_with_param(
            "agent_A".to_string(),
            "name1".to_string(),
            "runtime".to_string(),
        );
        let new_workload_instance_name = new_workload.instance_name.clone();

        let mut mock_server_connection = MockServerConnection::default();
        mock_server_connection
            .expect_update_state()
            .once()
            .returning(move |_, _| {
                Ok(UpdateStateSuccess {
                    added_workloads: vec![new_workload_instance_name.to_string()],
                    ..Default::default()
                })
            });

        let mut cmd = CliCommands {
            _response_timeout_ms: RESPONSE_TIMEOUT_MS,
            no_wait: true, // disable wait
            server_connection: mock_server_connection,
        };

        let object_field_mask = vec!["desiredState.workloads.name1".to_string()];
        let state_object_file = Some("some/path/to/newState.yaml".to_string());

        let set_state_result = cmd.set_state(object_field_mask, state_object_file).await;

        assert!(set_state_result.is_ok());
    }
}

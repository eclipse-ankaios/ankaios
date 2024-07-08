use std::io::{self, Read};

use common::{
    objects::{CompleteState, StoredWorkloadSpec},
    state_manipulation::{Object, Path},
};

#[cfg(not(test))]
async fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
// #[cfg(test)]
// use tests::read_to_string_mock as read_file_to_string;

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
    // let mut complete_state_object: Object = *complete_state.try_into();
    for field_mask in object_field_mask {
        let path: Path = field_mask.into();

        println!("{:?}", path);
        println!("{:?}", complete_state_object);

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
    // *complete_state = complete_state_object.try_into()?;
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
    use common::{
        objects::{CompleteState, StoredWorkloadSpec},
        state_manipulation::{Object, Path},
    };
    use serde_yaml::Value;
    use std::io::Cursor;

    const SAMPLE_CONFIG: &str = r#"desiredState:
        workloads:
          nginx:
            agent: agent_A
            tags:
            - key: owner
              value: Ankaios team
            dependencies: {}
            restartPolicy: NEVER
            runtime: podman
            runtimeConfig: |
              image: docker.io/nginx:latest
              commandOptions: ["-p", "8081:80"]"#;

    #[test]
    fn test_add_default_workload_spec_empty_update_mask() {
        let update_mask = vec![];
        let mut complete_state = CompleteState {
            desired_state: Default::default(),
            ..Default::default()
        };

        add_default_workload_spec_per_update_mask(&update_mask, &mut complete_state);

        assert!(complete_state.desired_state.workloads.is_empty());
    }

    #[test]
    fn test_add_default_workload_spec_with_update_mask() {
        let update_mask = vec!["desiredState.workloads.nginx".to_string()];
        let mut complete_state = CompleteState {
            desired_state: Default::default(),
            ..Default::default()
        };

        add_default_workload_spec_per_update_mask(&update_mask, &mut complete_state);

        assert!(complete_state.desired_state.workloads.contains_key("nginx"));
    }

    #[test]
    fn test_add_default_workload_spec_invalid_path() {
        let update_mask = vec!["invalid.path".to_string()];
        let mut complete_state = CompleteState {
            desired_state: Default::default(),
            ..Default::default()
        };

        add_default_workload_spec_per_update_mask(&update_mask, &mut complete_state);

        assert!(complete_state.desired_state.workloads.is_empty());
    }

    #[tokio::test]
    async fn test_process_inputs_stdin() {
        let input = SAMPLE_CONFIG;
        let reader = Cursor::new(input);
        let state_object_file = "-".to_string();
        let mut temp_obj = Object::default();

        process_inputs(reader, &state_object_file, &mut temp_obj).await;

        let value: Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let expected_obj = Object::try_from(&value).unwrap();

        assert_eq!(temp_obj, expected_obj);
    }

    // #[tokio::test]
    // async fn test_process_inputs_file() {
    //     // Mock the read_file_to_string function
    //     let mut mock = MockReadFileToString::new();
    //     mock.expect_read_to_string()
    //         .with(eq("state_file.yaml".to_string()))
    //         .returning(|_| Ok(SAMPLE_CONFIG.to_string()));

    //     let state_object_file = "state_file.yaml".to_string();
    //     let mut temp_obj = Object::default();

    //     process_inputs(io::empty(), &state_object_file, &mut temp_obj).await;

    //     let value: Value = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
    //     let expected_obj = Object::try_from(&value).unwrap();

    //     assert_eq!(temp_obj, expected_obj);
    // }

    #[tokio::test]
    async fn test_process_inputs_invalid_yaml() {
        let input = "invalid yaml";
        let reader = Cursor::new(input);
        let state_object_file = "-".to_string();
        let mut temp_obj = Object::default();

        process_inputs(reader, &state_object_file, &mut temp_obj).await;
    }
}

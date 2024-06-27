use std::io;

use common::{
    objects::{CompleteState, StoredWorkloadSpec},
    state_manipulation::{Object, Path},
};

#[cfg(not(test))]
async fn read_file_to_string(file: String) -> std::io::Result<String> {
    std::fs::read_to_string(file)
}
#[cfg(test)]
use tests::read_to_string_mock as read_file_to_string;

use crate::{cli_error::CliError, output_debug};

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
        let mut x: Object = Object::default();

        // I should start from here

        if let Some(state_object_file) = state_object_file {
            match state_object_file.as_str() {
                "-" => {
                    let stdin = io::read_to_string(io::stdin()).unwrap_or_else(|error| {
                        panic!("Could not read the state object file.\nError: {}", error)
                    });
                    let value: serde_yaml::Value = serde_yaml::from_str(&stdin)?;
                    x = Object::try_from(&value)?;
                }
                _ => {
                    let state_object_data = read_file_to_string(state_object_file)
                        .await
                        .unwrap_or_else(|error| {
                            panic!("Could not read the state object file.\nError: {}", error)
                        });
                    let value: serde_yaml::Value = serde_yaml::from_str(&state_object_data)?;
                    x = Object::try_from(&value)?;
                }
            }

            // if let Some(state_object_file) = state_object_file {
            //     let state_object_data =
            //         read_file_to_string(state_object_file)
            //             .await
            //             .unwrap_or_else(|error| {
            //                 panic!("Could not read the state object file.\nError: {}", error)
            //             });
            //     let value: serde_yaml::Value = serde_yaml::from_str(&state_object_data)?;
            //     let x: Object = Object::try_from(&value)?;

            // and I should end it here

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

    pub async fn read_to_string_mock(_file: String) -> io::Result<String> {
        Ok("".into())
    }
}

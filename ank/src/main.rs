// Copyright (c) 2023 Elektrobit Automotive GmbH
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

mod cli;
mod cli_commands;
use std::{collections::HashSet, env};

use cli_commands::CliCommands;
use common::{objects::State, state_manipulation::Object};
mod log;

#[cfg(test)]
pub mod test_helper;

// [impl->swdd~cli-standalone-application~1]
#[tokio::main]
async fn main() {
    let args = cli::parse();

    let cli_name = "ank-cli";
    env::set_var(log::VERBOSITY_KEY, args.verbose.to_string());

    output_debug!(
        "Started '{}' with the following parameters: '{:?}'",
        cli_name,
        args
    );

    let mut cmd = CliCommands::init(
        args.response_timeout_ms,
        cli_name.to_string(),
        args.server_url,
    );

    match args.command {
        cli::Commands::Get(get_args) => match get_args.command {
            // [impl->swdd~cli-provides-get-current-state~1]
            // [impl->swdd~cli-provides-object-field-mask-arg-to-get-partial-current-state~1]
            Some(cli::GetCommands::State {
                object_field_mask,
                output_format,
            }) => {
                // [impl -> swdd~cli-provides-get-current-state~1]
                // [impl -> swdd~cli-blocks-until-ankaios-server-responds-get-current-state~1]
                if let Ok(out_text) = cmd.get_state(object_field_mask, output_format).await {
                    // [impl -> swdd~cli-returns-current-state-from-server~1]
                    output_and_exit!("{}", out_text);
                } else {
                    output_and_error!("Could not retrieve state.");
                }
            }

            Some(cli::GetCommands::Workload {
                workload_name,
                agent_name,
                state,
            }) => {
                output_debug!(
                    "Received get workload with workload_name='{:?}', agent_name='{:?}', state='{:?}'",
                    workload_name,
                    agent_name,
                    state,
                );
                match cmd.get_workloads(agent_name, state, workload_name).await {
                    Ok(out_text) => output_and_exit!("{}", out_text),
                    Err(error) => output_and_error!("Failed to get workloads: '{}'", error),
                }
            }
            None => unreachable!("Unreachable code."),
        },
        cli::Commands::Set(set_args) => match set_args.command {
            // [impl->swdd~cli-provides-set-current-state~1]
            Some(cli::SetCommands::State {
                object_field_mask,
                state_object_file,
            }) => {
                output_debug!(
                    "Received set with object_field_mask='{:?}' and state_object_file='{:?}'",
                    object_field_mask,
                    state_object_file
                );
                // [impl -> swdd~cli-provides-set-current-state~1]
                // [impl -> swdd~cli-blocks-until-ankaios-server-responds-set-current-state~1]
                cmd.set_state(object_field_mask, state_object_file).await;
            }
            None => unreachable!("Unreachable code."),
        },
        cli::Commands::Delete(delete_args) => match delete_args.command {
            Some(cli::DeleteCommands::Workload { workload_name }) => {
                output_debug!(
                    "Received delete workload with workload_name = '{:?}'",
                    workload_name
                );
                if let Err(error) = cmd.delete_workloads(workload_name).await {
                    output_and_error!("Failed to delete workloads: '{}'", error);
                }
            }
            None => unreachable!("Unreachable code."),
        },
        cli::Commands::Run(run_args) => match run_args.command {
            Some(cli::RunCommands::Workload {
                workload_name,
                runtime_name,
                runtime_config,
                agent_name,
                tags,
            }) => {
                output_debug!(
                    "Received run workload with workload_name='{:?}', runtime='{:?}', runtime_config='{:?}', agent_name='{:?}', tags='{:?}'",
                    workload_name,
                    runtime_name,
                    runtime_config,
                    agent_name,
                    tags,
                );
                if let Err(error) = cmd
                    .run_workload(
                        workload_name,
                        runtime_name,
                        runtime_config,
                        agent_name,
                        tags,
                    )
                    .await
                {
                    output_and_error!("Failed to run workloads: '{}'", error);
                }
            }
            None => unreachable!("Unreachable code."),
        },
        cli::Commands::Apply(apply_args) => {
            println!("{:?}", apply_args.manifest_files);
            let mut req_obj: Object = State::default().try_into().unwrap();
            match apply_args.get_input_sources() {
                Ok(mut manifests) => {
                    for manifest in manifests.iter_mut() {
                        let mut data = "".to_owned();
                        let _ = manifest.1.read_to_string(&mut data);
                        let yaml_nodes: serde_yaml::Value = serde_yaml::from_str(&data)
                            .unwrap_or_else(|error| {
                                panic!("Error while parsing the state object data.\nError: {error}")
                            });

                        let cur_obj: Object = Object::try_from(&yaml_nodes).unwrap();
                        let paths = common::state_manipulation::get_paths_from_yaml_node(
                            &yaml_nodes,
                            false,
                        );
                        // println!("\npaths:\n{:?}", paths);

                        let mut workload_paths: HashSet<common::state_manipulation::Path> =
                            HashSet::new();
                        for path in paths {
                            let parts = path.parts();
                            let _ =
                                &mut workload_paths.insert(common::state_manipulation::Path::from(
                                    format!("{}.{}", parts[0], parts[1]),
                                ));
                        }

                        print!("Processing manifest: '{}' - workloads: {{ ", manifest.0);
                        workload_paths.iter().for_each(|workload_path| {
                            if req_obj.get(workload_path).is_none() {
                                print!("'{}'", workload_path.parts()[1]);
                                let _ = req_obj.set(
                                    workload_path,
                                    cur_obj.get(workload_path).unwrap().clone(),
                                );
                            } else {
                                output_and_exit!(
                                    "Error: Multiple workloads with the same name '{}' found, last detected in '{}'!",
                                    workload_path.parts()[1],
                                    manifest.0
                                );
                            }
                        });
                        print!(" }}\n");
                        println!("\nreq_obj: {:?}\n", req_obj);
                    }

                    let update_state_req_obj: State = req_obj.try_into().unwrap();
                    println!("\n update_obj: {:?} \n", update_state_req_obj);
                }
                Err(err) => output_and_exit!("{:?}", err),
            }
        }
    }

    cmd.shut_down().await;
}

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

use std::env;
use std::path::PathBuf;

mod ank_config;
mod cli;
mod cli_commands;
use ank_config::{AnkConfig, DEFAULT_ANK_CONFIG_FILE_PATH};
use cli_commands::CliCommands;
use common::std_extensions::GracefulExitResult;
use grpc::security::TLSConfig;
mod cli_error;
mod filtered_complete_state;
mod log;

#[cfg(test)]
pub mod test_helper;

// [impl->swdd~cli-loads-config-file~1]
fn handle_ank_config(config_path: &Option<String>, default_path: &str) -> AnkConfig {
    match config_path {
        Some(config_path) => {
            let config_path = PathBuf::from(config_path);
            output_debug!(
                "Loading server config from user provided path '{}'",
                config_path.display()
            );
            AnkConfig::from_file(config_path).unwrap_or_exit("Config file could not be parsed")
        }
        None => {
            let default_path = PathBuf::from(default_path.as_ref() as &std::path::Path);
            if !default_path.try_exists().unwrap_or(false) {
                output_debug!("No config file found at default path '{}'. Using cli arguments and environment variables only.", default_path.display());
                AnkConfig::default()
            } else {
                output_debug!(
                    "Loading server config from default path '{}'",
                    default_path.display()
                );
                AnkConfig::from_file(default_path).expect("Config file could not be parsed")
            }
        }
    }
}

// [impl->swdd~cli-standalone-application~1]
#[tokio::main]
async fn main() {
    let args = cli::parse();

    // [impl->swdd~cli-loads-config-file~1]
    let mut ank_config = handle_ank_config(&args.config_path, &DEFAULT_ANK_CONFIG_FILE_PATH);
    ank_config.update_with_args(&args);

    let cli_name = "ank-cli";
    env::set_var(log::VERBOSITY_KEY, ank_config.verbose.to_string());
    env::set_var(log::QUIET_KEY, ank_config.quiet.to_string());

    output_debug!(
        "Started '{}' with the following parameters: '{:?}'",
        cli_name,
        ank_config
    );

    if let Err(err_message) = TLSConfig::is_config_conflicting(
        ank_config.insecure,
        &ank_config.ca_pem_content,
        &ank_config.crt_pem_content,
        &ank_config.key_pem_content,
    ) {
        output_warn!("{}", err_message);
    }

    // [impl->swdd~cli-provides-file-paths-to-communication-middleware~1]
    // [impl->swdd~cli-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
    // [impl->swdd~cli-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
    let tls_config = TLSConfig::new(
        ank_config.insecure,
        ank_config.ca_pem_content.clone(),
        ank_config.crt_pem_content.clone(),
        ank_config.key_pem_content.clone(),
    );

    let mut cmd = CliCommands::init(
        ank_config.response_timeout,
        cli_name.to_string(),
        ank_config.server_url.clone(),
        ank_config.no_wait,
        // [impl->swdd~cli-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
        tls_config.unwrap_or_exit_func(
            |err| output_and_error!("Missing certificate files: {}", err),
            -1,
        ),
    )
    .unwrap_or_else(|err| {
        output_and_error!("Cannot connect to server: '{}'", err);
    });

    match args.command {
        cli::Commands::Get(get_args) => match get_args.command {
            // [impl->swdd~cli-provides-get-desired-state~1]
            // [impl->swdd~cli-provides-object-field-mask-arg-to-get-partial-desired-state~1]
            Some(cli::GetCommands::State {
                object_field_mask,
                output_format,
            }) => {
                // [impl->swdd~cli-provides-get-desired-state~1]
                // [impl->swdd~cli-blocks-until-ankaios-server-responds-get-desired-state~1]
                if let Ok(out_text) = cmd.get_state(object_field_mask, output_format).await {
                    // [impl -> swdd~cli-returns-desired-state-from-server~1]
                    output_and_exit!("{}", out_text);
                } else {
                    output_and_error!("Could not retrieve state.");
                }
            }

            // [impl->swdd~cli-provides-list-of-workloads~1]
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

                match cmd
                    .get_workloads_table(agent_name, state, workload_name)
                    .await
                {
                    Ok(out_text) => output_and_exit!("{}", out_text),
                    Err(error) => output_and_error!("Failed to get workloads: '{}'", error),
                }
            }
            // [impl->swdd~cli-provides-list-of-agents~1]
            Some(cli::GetCommands::Agent {}) => {
                output_debug!("Received get agent.");

                match cmd.get_agents().await {
                    Ok(out_text) => output_and_exit!("{}", out_text),
                    Err(error) => output_and_error!("Failed to get agents: '{}'", error),
                }
            }
            // [impl->swdd~cli-provides-list-of-configs~1]
            Some(cli::GetCommands::Config {}) => {
                output_debug!("Received get config.");

                match cmd.get_configs().await {
                    Ok(out_text) => output_and_exit!("{}", out_text),
                    Err(error) => output_and_error!("Failed to get configs: '{}'", error),
                }
            }
            None => unreachable!("Unreachable code."),
        },
        cli::Commands::Set(set_args) => match set_args.command {
            // [impl->swdd~cli-provides-set-desired-state~1]
            Some(cli::SetCommands::State {
                object_field_mask,
                state_object_file,
            }) => {
                output_debug!(
                    "Received set with object_field_mask='{:?}' and state_object_file='{:?}'",
                    object_field_mask,
                    state_object_file
                );

                // [impl->swdd~cli-blocks-until-ankaios-server-responds-set-desired-state~2]
                if let Err(err) = cmd.set_state(object_field_mask, state_object_file).await {
                    output_and_error!("Failed to set state: '{}'", err)
                }
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
            // [impl->swdd~cli-provides-delete-configs~1]]
            Some(cli::DeleteCommands::Config { config_name }) => {
                output_debug!(
                    "Received delete config with config_name = '{:?}'",
                    config_name
                );
                if let Err(error) = cmd.delete_configs(config_name).await {
                    output_and_error!("Failed to delete configs: '{}'", error);
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
            if let Err(err) = cmd.apply_manifests(apply_args).await {
                output_and_error!("{}", err);
            }
        }
        cli::Commands::Logs(logs_args) => {
            if logs_args.follow {
                cmd.follow_logs(logs_args).await.unwrap_or_else(|err| {
                    output_and_error!("Failed to follow logs: '{}'", err);
                });
            } else {
                cmd.fetch_logs(logs_args).await.unwrap_or_else(|err| {
                    output_and_error!("Failed to get logs: '{}'", err);
                });
            }
        }
    }
    cmd.shut_down().await;
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
    use crate::{ank_config::DEFAULT_ANK_CONFIG_FILE_PATH, handle_ank_config, AnkConfig};
    use std::io::Write;
    use tempfile::NamedTempFile;

    const VALID_ANK_CONFIG_CONTENT: &str = r"#
    version = 'v1'
    response_timeout = 2500
    [default]
    #";

    #[test]
    fn utest_handle_ank_config_valid_config() {
        let mut tmp_config_file = NamedTempFile::new().expect("could not create temp file");
        write!(tmp_config_file, "{}", VALID_ANK_CONFIG_CONTENT)
            .expect("could not write to temp file");

        let ank_config = handle_ank_config(
            &Some(
                tmp_config_file
                    .into_temp_path()
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
            &DEFAULT_ANK_CONFIG_FILE_PATH,
        );

        assert_eq!(ank_config.response_timeout, 2500);
    }

    #[test]
    fn utest_handle_ank_config_default_path() {
        let mut file = tempfile::NamedTempFile::new().expect("Failed to create file");
        writeln!(file, "{}", VALID_ANK_CONFIG_CONTENT).expect("Failed to write to file");

        let ank_config = handle_ank_config(&None, file.path().to_str().unwrap());

        assert_eq!(ank_config.response_timeout, 2500);
    }

    #[test]
    fn utest_handle_ank_config_default() {
        let ank_config = handle_ank_config(&None, "/a/very/invalid/path/to/config/file");

        assert_eq!(ank_config, AnkConfig::default());
    }
}

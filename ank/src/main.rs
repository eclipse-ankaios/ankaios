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

mod cli;
mod cli_commands;
use cli_commands::CliCommands;
use common::std_extensions::GracefulExitResult;
use grpc::security::TLSConfig;
mod cli_error;
mod filtered_complete_state;
mod log;

#[cfg(test)]
pub mod test_helper;

// [impl->swdd~cli-standalone-application~1]
#[tokio::main]
async fn main() {
    let args = cli::parse();

    let cli_name = "ank-cli";
    env::set_var(log::VERBOSITY_KEY, args.verbose.to_string());
    env::set_var(log::QUIET_KEY, args.quiet.to_string());

    output_debug!(
        "Started '{}' with the following parameters: '{:?}'",
        cli_name,
        args
    );

    let server_url = match args.insecure {
        true => args.server_url.replace("http[s]", "http"),
        false => args.server_url.replace("http[s]", "https"),
    };

    if let Err(err_message) =
        TLSConfig::is_config_conflicting(args.insecure, &args.ca_pem, &args.crt_pem, &args.key_pem)
    {
        output_warn!("{}", err_message);
    }

    // [impl->swdd~cli-provides-file-paths-to-communication-middleware~1]
    // [impl->swdd~cli-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
    // [impl->swdd~cli-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
    let tls_config = TLSConfig::new(args.insecure, args.ca_pem, args.crt_pem, args.key_pem);

    let mut cmd = CliCommands::init(
        args.response_timeout_ms,
        cli_name.to_string(),
        server_url,
        args.no_wait,
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
                // [impl -> swdd~cli-provides-set-desired-state~1]
                // [impl -> swdd~cli-blocks-until-ankaios-server-responds-set-desired-state~2]
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
    }

    cmd.shut_down().await;
}

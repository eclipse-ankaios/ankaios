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
#[cfg(test)]
pub mod test_helper;

use cli_commands::CliCommands;

// [impl->swdd~cli-standalone-application~1]
#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("error"));
    let args = cli::parse();

    let cli_name = "ank-cli";

    match args.command {
        cli::Commands::Get(get_args) => match get_args.command {
            // [impl->swdd~cli-provides-get-current-state~1]
            // [impl->swdd~cli-provides-object-field-mask-arg-to-get-partial-current-state~1]
            Some(cli::GetCommands::State {
                object_field_mask,
                output_format,
            }) => {
                let mut cmd = CliCommands::init(
                    args.response_timeout_ms,
                    cli_name.to_string(),
                    args.server_url,
                );
                // [impl -> swdd~cli-provides-get-current-state~1]
                // [impl -> swdd~cli-blocks-until-ankaios-server-responds-get-current-state~1]
                if let Some(out_text) = cmd.get_state(object_field_mask, output_format).await {
                    // [impl -> swdd~cli-returns-current-state-from-server~1]
                    println!("{}", out_text);
                }
            }

            Some(cli::GetCommands::Workload {
                workload_name,
                agent_name,
                state,
            }) => {
                log::info!(
                    "Got get workload with: workload name {:?}, agent_name={:?}, state={:?}",
                    workload_name,
                    agent_name,
                    state,
                );
                let mut cmd = CliCommands::init(
                    args.response_timeout_ms,
                    cli_name.to_string(),
                    args.server_url,
                );
                match cmd.get_workloads(agent_name, state, workload_name).await {
                    Ok(out_text) => println!("{}", out_text),
                    Err(error) => log::error!("Failed to get workloads: '{}'", error),
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
                log::info!(
                    "Got: object_field_mask={:?} state_object_file={:?}",
                    object_field_mask,
                    state_object_file
                );
                let mut cmd = CliCommands::init(
                    args.response_timeout_ms,
                    cli_name.to_string(),
                    args.server_url,
                );
                // [impl -> swdd~cli-provides-set-current-state~1]
                // [impl -> swdd~cli-blocks-until-ankaios-server-responds-set-current-state~1]
                cmd.set_state(
                    object_field_mask,
                    state_object_file,
                    args.response_timeout_ms,
                )
                .await;
            }
            None => unreachable!("Unreachable code."),
        },
        cli::Commands::Delete(delete_args) => match delete_args.command {
            Some(cli::DeleteCommands::Workload { workload_name }) => {
                log::info!(
                    "Got delete workload with: workload_name = {:?}",
                    workload_name
                );
                let mut cmd = CliCommands::init(
                    args.response_timeout_ms,
                    cli_name.to_string(),
                    args.server_url,
                );
                if let Err(error) = cmd.delete_workloads(workload_name).await {
                    log::error!("Failed to delete workloads: '{}'", error);
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
                log::info!(
                    "Got run workload with: workload_name={:?}, runtime={:?}, runtime_config={:?}, agent_name={:?}, tags={:?}",
                    workload_name,
                    runtime_name,
                    runtime_config,
                    agent_name,
                    tags,
                );
                let mut cmd = CliCommands::init(
                    args.response_timeout_ms,
                    cli_name.to_string(),
                    args.server_url,
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
                    log::error!("Failed to run workloads: '{}'", error);
                }
            }
            None => unreachable!("Unreachable code."),
        },
    }
}

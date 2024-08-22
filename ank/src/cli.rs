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

use std::error::Error;

use clap::{command, Parser, Subcommand, ValueHint};

use clap_complete::{ArgValueCompleter, CompletionCandidate};
use common::DEFAULT_SERVER_ADDRESS;
use serde_json::Value;

const ANK_SERVER_URL_ENV_KEY: &str = "ANK_SERVER_URL";

const DESIRED_STATE: &str = "desiredState";
const WORKLOADS: &str = "workloads";
const WORKLOAD_STATES: &str = "workloadStates";

fn get_completions_workloads() -> Vec<CompletionCandidate> {
    let mut result = Vec::new();

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("ank get state -o json desiredState.workloads")
        .output()
        .expect("failed to execute process");

    let v: Value = serde_json::from_slice(&output.stdout).unwrap();

    if let Value::Object(workloads) = &v[DESIRED_STATE][WORKLOADS] {
        for workload_name in workloads.keys() {
            result.push(CompletionCandidate::new(workload_name));
        }
    }

    result
}

fn get_completions_object_field_mask() -> Vec<CompletionCandidate> {
    let mut result = Vec::new();

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("ank get state -o json")
        .output()
        .expect("failed to execute process");

    let v: Value = serde_json::from_slice(&output.stdout).unwrap();

    if let Value::Object(workloads) = &v[DESIRED_STATE][WORKLOADS] {
        result.push(CompletionCandidate::new(format!(
            "{}.{}",
            DESIRED_STATE, WORKLOADS
        )));
        for workload in workloads.keys() {
            result.push(CompletionCandidate::new(format!(
                "desiredState.workloads.{}",
                workload
            )));
        }
    }

    if let Value::Object(workload_states) = &v[WORKLOAD_STATES] {
        result.push(CompletionCandidate::new(WORKLOAD_STATES));
        for agent in workload_states.keys() {
            result.push(CompletionCandidate::new(format!(
                "{}.{}",
                WORKLOAD_STATES, agent
            )));
            if let Value::Object(workloads) = &v[WORKLOAD_STATES][agent] {
                for workload in workloads.keys() {
                    result.push(CompletionCandidate::new(format!(
                        "{}.{}.{}",
                        WORKLOAD_STATES, agent, workload
                    )));
                }
            }
        }
    }

    result
}

// [impl->swdd~cli-supports-server-url-cli-argument~1]
// [impl->swdd~cli-supports-pem-file-paths-as-cli-arguments~1]
// [impl->swdd~cli-supports-cli-argument-for-insecure-communication~1]
#[derive(Parser, Debug)] // requires `derive` feature
#[command(name = "ank")]
#[command(bin_name = "ank")]
#[command(version)]
/// Manage the Ankaios system
pub struct AnkCli {
    #[command(subcommand)]
    pub command: Commands,
    #[clap(short = 's', long = "server-url", default_value_t = DEFAULT_SERVER_ADDRESS.to_string(), env = ANK_SERVER_URL_ENV_KEY)]
    /// The url to Ankaios server.
    pub server_url: String,
    #[clap(long = "response-timeout", default_value_t = 3000)]
    /// The timeout in milliseconds to wait for a response.
    pub response_timeout_ms: u64,
    #[clap(short = 'v', long = "verbose")]
    /// Enable debug traces
    pub verbose: bool,
    #[clap(short = 'q', long = "quiet")]
    /// Disable all output
    pub quiet: bool,
    #[clap(long = "no-wait")]
    /// Do not wait for workloads to be created/deleted
    pub no_wait: bool,
    #[clap(
        short = 'k',
        long = "insecure",
        env = "ANK_INSECURE",
        default_value_t = false
    )]
    /// Flag to disable TLS communication between ank CLI and Ankaios server.
    pub insecure: bool,
    #[clap(long = "ca_pem", env = "ANK_CA_PEM")]
    /// Path to cli ca pem file.
    pub ca_pem: Option<String>,
    #[clap(long = "crt_pem", env = "ANK_CRT_PEM")]
    /// Path to cli certificate pem file.
    pub crt_pem: Option<String>,
    #[clap(long = "key_pem", env = "ANK_KEY_PEM")]
    /// Path to cli key pem file.
    pub key_pem: Option<String>,
}

/// Supported actions
#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(arg_required_else_help = true)]
    Get(GetArgs),
    #[command(arg_required_else_help = true)]
    Set(SetArgs),
    #[command(arg_required_else_help = true)]
    Delete(DeleteArgs),
    #[command(arg_required_else_help = true)]
    Run(RunArgs),
    #[command(arg_required_else_help = true)]
    Apply(ApplyArgs),
}

/// Retrieve information about the current Ankaios system
#[derive(clap::Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct GetArgs {
    #[command(subcommand)]
    pub command: Option<GetCommands>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum OutputFormat {
    Yaml,
    Json,
}

/// Get commands
#[derive(Debug, Subcommand)]
pub enum GetCommands {
    /// State information of Ankaios system
    State {
        /// Specify the output format
        #[arg(short = 'o', value_enum, default_value_t = OutputFormat::Yaml)]
        output_format: OutputFormat,
        /// Select which parts of the state object shall be output e.g. 'desiredState.workloads.nginx' [default: empty = the complete state]
        #[arg(add = ArgValueCompleter::new(|| get_completions_object_field_mask()))]
        object_field_mask: Vec<String>,
    },
    /// Information about workloads of the Ankaios system
    /// For automation use "ank get state -o json" and process the workloadStates
    #[clap(visible_alias("workloads"), verbatim_doc_comment)]
    Workload {
        /// Only workloads of the given agent shall be output
        #[arg(short = 'a', long = "agent", required = false)]
        agent_name: Option<String>,
        /// Only workloads in the given state shall be output
        #[arg(short = 's', long = "state", required = false)]
        state: Option<String>,
        /// Select which workload(s) shall be returned [default: empty = all workloads]
        #[arg(add = ArgValueCompleter::new(|| get_completions_workloads() ))]
        workload_name: Vec<String>,
    },
}

/// Update the state of Ankaios system
#[derive(clap::Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct SetArgs {
    #[command(subcommand)]
    pub command: Option<SetCommands>,
}

/// Set commands
#[derive(Debug, Subcommand)]
pub enum SetCommands {
    /// State information of Ankaios system
    State {
        /// Select which parts of the state object shall be updated e.g. 'desiredState.workloads.nginx'
        #[arg(required = true, add = ArgValueCompleter::new(|| get_completions_object_field_mask()))]
        object_field_mask: Vec<String>,
        /// A file containing the new State Object Description in yaml format
        #[arg(required = true, value_hint = ValueHint::FilePath)]
        state_object_file: String,
    },
}

/// Delete the workload
#[derive(clap::Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct DeleteArgs {
    #[command(subcommand)]
    pub command: Option<DeleteCommands>,
}

#[derive(Debug, Subcommand)]
pub enum DeleteCommands {
    /// Delete a workload(s)
    #[clap(visible_alias("workloads"))]
    Workload {
        /// One or more workload(s) to be deleted
        #[arg(required = true, add = ArgValueCompleter::new(|| get_completions_workloads()))]
        workload_name: Vec<String>,
    },
}

/// Run the workload
#[derive(clap::Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub struct RunArgs {
    #[command(subcommand)]
    pub command: Option<RunCommands>,
}

#[derive(Debug, Subcommand)]
pub enum RunCommands {
    /// Run the workload
    Workload {
        /// Name of the workload to run
        #[arg(required = true)]
        workload_name: String,
        /// Name of the runtime. For example "--runtime podman"
        #[arg(long = "runtime")]
        runtime_name: String,
        /// A string with the runtime configuration for the configured runtime.
        /// For example to run the nginx server as the parameter as follows:
        ///
        /// CFG=$'image: docker.io/nginx:latest\ncommandOptions: ["-p", "8081:80"]'
        ///
        /// --config "$CFG"
        #[arg(long = "config")]
        runtime_config: String,
        /// Name of the agent where the workload is supposed to run
        #[arg(long = "agent")]
        agent_name: String,
        ///Tags formatted as: "--tags key1=value1 --tags key2=value2"
        #[arg(long = "tags", value_parser = parse_key_val::<String, String>)]
        tags: Vec<(String, String)>,
    },
}

/// Apply Ankaios manifest content or file(s)
#[derive(clap::Args, Debug)]
pub struct ApplyArgs {
    #[arg(value_name = "Ankaios manifest file(s) or '-' for stdin", value_hint = ValueHint::FilePath)]
    pub manifest_files: Vec<String>,
    /// Specify on which agent to apply the Ankaios manifests.
    /// If not specified, the agent(s) must be specified in the Ankaios manifest(s)
    #[arg(long = "agent")]
    pub agent_name: Option<String>,
    /// Delete mode activated
    #[arg(short)]
    pub delete_mode: bool,
}

fn parse_key_val<K, V>(s: &str) -> Result<(K, V), Box<dyn Error + Send + Sync + 'static>>
where
    K: std::str::FromStr,
    K::Err: Error + Send + Sync + 'static,
    V: std::str::FromStr,
    V::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

pub fn parse() -> AnkCli {
    AnkCli::parse()
}

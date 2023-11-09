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

use std::{env, error::Error, str::FromStr};

use clap::{
    command,
    error::{ContextKind, ContextValue, ErrorKind},
    Parser, Subcommand,
};

use common::DEFAULT_SERVER_ADDRESS;
use url::Url;

const ANK_SERVER_URL_ENV_KEY: &str = "ANK_SERVER_URL";

fn create_error_context(
    cmd: &clap::Command,
    env_key: &str,
    arg: Option<&clap::Arg>,
    arg_value: &String,
) -> clap::Error {
    let mut err = clap::Error::new(ErrorKind::ValueValidation).with_cmd(cmd); // the order in which the errors are inserted is important
    if let Some(arg) = arg {
        if let Ok(env_value) = env::var(env_key) {
            if env_value == *arg_value {
                err.insert(
                    ContextKind::InvalidArg,
                    ContextValue::String(format!("environment variable '{}'", env_key)),
                );
            } else {
                err.insert(
                    ContextKind::InvalidArg,
                    ContextValue::String(arg.to_string()),
                );
            }
        } else {
            err.insert(
                ContextKind::InvalidArg,
                ContextValue::String(arg.to_string()),
            );
        }
    }

    err.insert(
        ContextKind::InvalidValue,
        ContextValue::String(arg_value.clone()),
    );
    err
}

#[derive(Clone)]
/// Custom url parser as a workaround to Clap's bug about
/// outputting the wrong error message context
/// when using the environment variable and not the cli argument.
/// When using a wrong value inside environment variable
/// Clap still outputs that the cli argument was wrongly set,
/// but not the environment variable. This is poor use-ability.
/// An issue for this bug is already opened (https://github.com/clap-rs/clap/issues/5202).
/// The code will be removed if the bug in Clap is fixed.
pub struct ServerUrlParser;

impl clap::builder::TypedValueParser for ServerUrlParser {
    type Value = Url;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let arg_value = value.to_string_lossy().to_string();
        let err = create_error_context(cmd, ANK_SERVER_URL_ENV_KEY, arg, &arg_value);
        let url = Url::from_str(&arg_value).map_err(|_| err)?;
        Ok(url)
    }
}

// [impl->swdd~cli-shall-support-environment-variables~1]
// [impl->swdd~cli-prioritizes-cli-argument-over-environment-variable~1]
#[derive(Parser)] // requires `derive` feature
#[command(name = "ank")]
#[command(bin_name = "ank")]
#[command(version)]
/// Manage the Ankaios system
pub struct AnkCli {
    #[command(subcommand)]
    pub command: Commands,
    #[clap(short = 's', long = "server-url", default_value_t = DEFAULT_SERVER_ADDRESS.parse().unwrap(), env = ANK_SERVER_URL_ENV_KEY, value_parser = ServerUrlParser)]
    /// The url to Ankaios server.
    pub server_url: Url,
    #[clap(long = "response-timeout", default_value_t = 3000)]
    /// The timeout in milliseconds to wait for a response.
    pub response_timeout_ms: u64,
    #[clap(short = 'v', long = "verbose")]
    /// Enable debug traces
    pub verbose: bool,
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
        /// Select which parts of the state object shall be output e.g. 'currentState.workloads.nginx' [default: empty = the complete state]
        object_field_mask: Vec<String>,
    },
    /// Information about workloads of the Ankaios system
    #[clap(visible_alias("workloads"))]
    Workload {
        /// Only workloads of the given agent shall be output
        #[arg(short = 'a', long = "agent", required = false)]
        agent_name: Option<String>,
        /// Only workloads in the given state shall be output
        #[arg(short = 's', long = "state", required = false)]
        state: Option<String>,
        /// Select which workload(s) shall be returned [default: empty = all workloads]
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
        /// Select which parts of the state object shall be updated e.g. 'currentState.workloads.nginx'
        #[arg(required = true)]
        object_field_mask: Vec<String>,
        /// A file containing the new State Object Description in yaml format
        #[arg(short = 'f', long = "file")]
        state_object_file: Option<String>,
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
        #[arg(required = true)]
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
        /// CFG=$'image: docker.io/nginx:latest\nports:\n- containerPort: 80\n  hostPort: 8081'
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
    use clap::builder::TypedValueParser;
    use std::env;
    use std::ffi::OsStr;
    const INVALID_VALUE: &str = "invalid-value";
    const PROGRAM_NAME: &str = "some program";
    const ARG_NAME: &str = "arg";
    const EXAMPLE_URL: &str = "http://0.0.0.0:11111";

    struct CleanupEnv;

    impl Drop for CleanupEnv {
        fn drop(&mut self) {
            env::remove_var(ANK_SERVER_URL_ENV_KEY);
        }
    }

    // [utest->swdd~cli-shall-support-environment-variables~1]
    #[test]
    fn utest_cli_argument_server_url_use_cli_arg() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        let actual_url = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(common::DEFAULT_SERVER_ADDRESS))
            .unwrap();
        let expected_url = Url::from_str(common::DEFAULT_SERVER_ADDRESS).unwrap();
        assert_eq!(actual_url, expected_url);
    }

    // [utest->swdd~cli-shall-support-environment-variables~1]
    #[test]
    fn utest_cli_argument_server_url_use_env_var() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(ANK_SERVER_URL_ENV_KEY, EXAMPLE_URL);
        let actual_url = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(EXAMPLE_URL))
            .unwrap();

        let expected_url = Url::from_str(EXAMPLE_URL).unwrap();
        assert_eq!(actual_url, expected_url);
    }

    // [utest->swdd~cli-shall-support-environment-variables~1]
    // [utest->swdd~cli-prioritizes-cli-argument-over-environment-variable~1]
    #[test]
    fn utest_cli_argument_server_url_prioritize_cli_arg() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(ANK_SERVER_URL_ENV_KEY, EXAMPLE_URL);
        let actual_url = url_parser
            .parse_ref(&cmd, Some(&arg), OsStr::new(common::DEFAULT_SERVER_ADDRESS))
            .unwrap();

        let expected_url = Url::from_str(common::DEFAULT_SERVER_ADDRESS).unwrap();
        assert_eq!(actual_url, expected_url);
    }

    // [utest->swdd~cli-shall-support-environment-variables~1]
    #[test]
    fn utest_cli_argument_server_url_use_env_var_error_context() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        std::env::set_var(ANK_SERVER_URL_ENV_KEY, INVALID_VALUE);
        let parsing_result = url_parser.parse_ref(&cmd, Some(&arg), OsStr::new(INVALID_VALUE));

        assert!(parsing_result
            .err()
            .unwrap()
            .to_string()
            .contains("environment variable"));
    }

    // [utest->swdd~cli-shall-support-environment-variables~1]
    #[test]
    fn utest_cli_argument_server_url_use_cli_arg_error_context() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let arg = clap::Arg::new(ARG_NAME);
        let parsing_result = url_parser.parse_ref(&cmd, Some(&arg), OsStr::new(INVALID_VALUE));
        assert!(parsing_result.err().unwrap().to_string().contains(ARG_NAME));
    }

    // [utest->swdd~cli-shall-support-environment-variables~1]
    #[test]
    fn utest_cli_argument_server_url_none_arg_error_context() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();
        let _cleanup = CleanupEnv;
        let url_parser = ServerUrlParser;
        let cmd = clap::Command::new(PROGRAM_NAME);
        let parsing_result = url_parser.parse_ref(&cmd, None, OsStr::new(INVALID_VALUE));
        let err: String = parsing_result.err().unwrap().to_string();
        assert!(err.contains("invalid value for one of the arguments"));
    }
}

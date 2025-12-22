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

use clap::{ArgAction, Parser};

// [impl->swdd~agent-supports-cli-argument-for-insecure-communication~1]
// [impl->swdd~agent-supports-pem-file-paths-as-cli-arguments~1]
#[derive(Parser, Debug)]
#[command( author="The Ankaios team",
           version=env!("CARGO_PKG_VERSION"),
           about="Ankaios - your friendly automotive workload orchestrator.\nWhat can the agent do for you?")
]
pub struct Arguments {
    #[arg(required = false, short = 'x', long = "agent-config")]
    /// The path to the agent config file.
    /// The default path is /etc/ankaios/ank-agent.conf
    pub config_path: Option<String>,
    #[arg(short = 'n', long = "name", required = false)]
    /// The name to use for the registration with the server. Every agent has to register with a unique name.
    /// Agent name shall contain only regular upper and lowercase characters (a-z and A-Z), numbers and the symbols "-" and "_".
    pub agent_name: Option<String>,
    #[arg(short = 's', long = "server-url", required = false)]
    /// The server url.
    pub server_url: Option<String>,
    /// An existing directory where agent specific runtime files will be stored. If not specified, a default folder is created.
    #[arg(short = 'r', long = "run-folder", required = false)]
    pub run_folder: Option<String>,
    #[arg(short = 'k', long = "insecure", action=ArgAction::Set, num_args=0, default_missing_value="true", env = "ANKAGENT_INSECURE")]
    /// Flag to disable TLS communication between Ankaios agent and server.
    pub insecure: Option<bool>,
    #[arg(short = 't', long = "tag", value_parser = parse_key_val, action = ArgAction::Append)]
    /// Agent tags as key=value pairs. Can be specified multiple times.
    /// Example: --tag cpu=x86_64 --tag location=cloud
    pub tags: Option<Vec<(String, String)>>,
    #[arg(long = "ca_pem", env = "ANKAGENT_CA_PEM")]
    /// Path to agent ca pem file.
    pub ca_pem: Option<String>,
    #[arg(long = "crt_pem", env = "ANKAGENT_CRT_PEM")]
    /// Path to agent certificate pem file.
    pub crt_pem: Option<String>,
    #[arg(long = "key_pem", env = "ANKAGENT_KEY_PEM")]
    /// Path to agent key pem file.
    pub key_pem: Option<String>,
}

/// Parse a single key-value pair
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

pub fn parse() -> Arguments {
    Arguments::parse()
}

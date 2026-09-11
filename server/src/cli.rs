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
use std::env;

pub fn parse() -> Arguments {
    Arguments::parse()
}

// [impl->swdd~server-supports-pem-file-paths-as-cli-arguments~1]
// [impl->swdd~server-supports-cli-argument-for-insecure-communication~1]
#[derive(Parser, Debug)]
#[command( author="The Ankaios team",
           version=env!("CARGO_PKG_VERSION"),
           about="Ankaios - your friendly automotive workload orchestrator.\nWhat can the server do for you?")]
// default values for the server are set in server-config.rs
pub struct Arguments {
    #[arg(
        short = 'm',
        short_alias = 'c',
        long = "startup-manifest",
        alias = "startup-config"
    )]
    /// The path to the startup manifest yaml.
    pub manifest_path: Option<String>,
    #[arg(required = false, short = 'x', long = "server-config")]
    /// The path to the server config file.
    /// The default path is /etc/ankaios/ank-server.conf
    pub config_path: Option<String>,
    #[arg(required = false, short = 'a', long = "address", env = "ANKSERVER_SERVER_URL")]
    /// The endpoint the server shall listen at [default: 127.0.0.1:25551].
    /// Supported values are host:port (TCP) and unix:///path/to/socket (Unix domain socket).
    pub addr: Option<String>,
    #[arg(long = "socket-group", env = "ANKSERVER_SOCKET_GROUP")]
    /// Group name assigned to the Unix domain socket file.
    /// This option is only valid with unix:// server addresses.
    pub socket_group: Option<String>,
    #[arg(short = 'k', long = "insecure", action=ArgAction::Set, num_args=0, default_missing_value="true", env = "ANKSERVER_INSECURE")]
    /// Flag to disable TLS communication between Ankaios server, agent and ank CLI.
    pub insecure: Option<bool>,
    #[arg(long = "ca_pem", env = "ANKSERVER_CA_PEM")]
    /// Path to server ca certificate pem file.
    pub ca_pem: Option<String>,
    #[arg(long = "crt_pem", env = "ANKSERVER_CRT_PEM")]
    /// Path to server certificate pem file.
    pub crt_pem: Option<String>,
    #[arg(long = "key_pem", env = "ANKSERVER_KEY_PEM")]
    /// Path to server key pem file.
    pub key_pem: Option<String>,
}
// Note: this code is intentionally without unit tests.
// There is no business logic which can be tested, here we have only a config and a call of "clap" crate.

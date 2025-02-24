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

mod ankaios_server;
mod cli;
mod server_config;

use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

use common::objects::CompleteState;

use common::communications_server::CommunicationsServer;
use common::objects::State;
use common::std_extensions::GracefulExitResult;

use ankaios_server::{create_from_server_channel, create_to_server_channel, AnkaiosServer};
use server_config::{ServerConfig, DEFAULT_SERVER_CONFIG_FILE_PATH};

use grpc::{security::TLSConfig, server::GRPCCommunicationsServer};

fn prepare_server_config(config_path_arg: &Option<String>) -> Result<ServerConfig, Error> {
    if let Some(config_path_str) = config_path_arg {
        let config_path = PathBuf::from(&config_path_str);

        if config_path.try_exists()? {
            fs::read_to_string(&config_path)?;
            Ok(ServerConfig::from_file(config_path)
                .map_err(|err| Error::new(ErrorKind::Other, format!("{}", err)))?)
        } else {
            Err(Error::new(
                ErrorKind::NotFound,
                format!("Config file not found: {}", config_path_str),
            ))
        }
    } else {
        let default_path = PathBuf::from(DEFAULT_SERVER_CONFIG_FILE_PATH);
        if !default_path.try_exists()? {
            Ok(ServerConfig::default())
        } else {
            fs::read_to_string(DEFAULT_SERVER_CONFIG_FILE_PATH)?;
            Ok(ServerConfig::from_file(default_path)
                .map_err(|err| Error::new(ErrorKind::Other, format!("{}", err)))?)
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    // [impl->swdd~server-loads-config-file~1]
    let mut server_config = prepare_server_config(&args.config_path)
        .unwrap_or_exit("Error evaluating server config file path");

    server_config.update_with_args(&args);

    log::debug!(
        "Starting the Ankaios server with \n\tserver address: '{}', \n\tstartup manifest path: '{}'",
        server_config.address,
        server_config
            .startup_manifest
            .clone()
            .unwrap_or("[no config file provided]".to_string()),
    );

    let startup_state = match &server_config.startup_manifest {
        Some(config_path) => {
            let data =
                fs::read_to_string(config_path).unwrap_or_exit("Could not read the startup config");
            // [impl->swdd~server-state-in-memory~1]
            // [impl->swdd~server-loads-startup-state-file~3]
            let state: State = serde_yaml::from_str(&data)
                .unwrap_or_exit("Parsing start config failed with error");
            log::trace!(
                "The state is initialized with the following workloads: {:?}",
                state.workloads
            );
            Some(CompleteState {
                desired_state: state,
                ..Default::default()
            })
        }
        // [impl->swdd~server-starts-without-startup-config~1]
        _ => None,
    };

    let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
    let (to_agents, agents_receiver) = create_from_server_channel(common::CHANNEL_CAPACITY);

    if let Err(err_message) = TLSConfig::is_config_conflicting(
        server_config.insecure.unwrap(),
        &server_config.ca_pem_content,
        &server_config.crt_pem_content,
        &server_config.key_pem_content,
    ) {
        log::warn!("{}", err_message);
    }

    // [impl->swdd~server-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
    // [impl->swdd~server-provides-file-paths-to-communication-middleware~1]
    // [impl->swdd~server-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
    let tls_config = TLSConfig::new(
        server_config.insecure.unwrap_or(true),
        server_config.ca_pem_content,
        server_config.crt_pem_content,
        server_config.key_pem_content,
    );

    let mut communications_server = GRPCCommunicationsServer::new(
        to_server.clone(),
        // [impl->swdd~server-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
        tls_config.unwrap_or_exit("Missing certificates files"),
    );
    let mut server = AnkaiosServer::new(server_receiver, to_agents.clone());

    tokio::select! {
        // [impl->swdd~server-default-communication-grpc~1]
        communication_result = communications_server.start(agents_receiver, server_config.address) => {
            communication_result.unwrap_or_exit("server error")
        }

        server_result = server.start(startup_state) => {
            server_result.unwrap_or_exit("server error")
        }
    }
}

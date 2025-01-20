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

use common::objects::CompleteState;

use common::communications_server::CommunicationsServer;
use common::objects::State;
use common::std_extensions::GracefulExitResult;

use ankaios_server::{create_from_server_channel, create_to_server_channel, AnkaiosServer};
use server_config::ServerConfig;

use grpc::{security::TLSConfig, server::GRPCCommunicationsServer};

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // TODO: needs to be replaced with /etc/ankaios/ank-server.conf
    let mut server_config =
        ServerConfig::from_file("/workspaces/ankaios/server/config/ank-server.conf")
            .unwrap_or_default();

    let args = cli::parse();

    server_config.update_with_args(&args);

    log::debug!(
        "Starting the Ankaios server with \n\tserver address: '{}', \n\tstartup config path: '{}'",
        server_config.address.unwrap(),
        server_config
            .startup_config
            .clone()
            .unwrap_or("[no config file provided]".to_string()),
    );

    log::debug!("server config 1: {:?}", server_config);

    let startup_state = match &server_config.startup_config {
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

    log::debug!("server config 2: {:?}", server_config);

    let (to_server, server_receiver) = create_to_server_channel(common::CHANNEL_CAPACITY);
    let (to_agents, agents_receiver) = create_from_server_channel(common::CHANNEL_CAPACITY);

    log::debug!("server config 3: {:?}", server_config);

    if let Err(err_message) = TLSConfig::is_config_conflicting(
        server_config.insecure.unwrap(),
        &server_config.ca_pem,
        &server_config.crt_pem,
        &server_config.key_pem,
    ) {
        log::warn!("{}", err_message);
    }

    log::debug!("server config 4: {:?}", server_config);

    if let Err(err_message) = TLSConfig::is_config_conflicting(
        server_config.insecure.unwrap(),
        &server_config.ca_pem,
        &server_config.crt_pem,
        &server_config.key_pem,
    ) {
        log::warn!("{}", err_message);
    }

    log::debug!("server config 5: {:?}", server_config);

    // [impl->swdd~server-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
    // [impl->swdd~server-provides-file-paths-to-communication-middleware~1]
    // [impl->swdd~server-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
    log::debug!("ca_pem: {:?}", server_config.ca_pem);
    log::debug!("crt_pem: {:?}", server_config.crt_pem);
    log::debug!("key_pem: {:?}", server_config.ca_pem);

    let tls_config: Result<Option<TLSConfig>, String> = if server_config.ca_pem.is_some()
        || server_config.crt_pem.is_some()
        || server_config.key_pem.is_some()
    // && !server_config.insecure.unwrap()
    {
        log::debug!("TLS CONFIG CHECKING GOT IN HERE 1");
        TLSConfig::new(
            server_config.insecure.unwrap(),
            server_config.ca_pem,
            server_config.crt_pem,
            server_config.key_pem,
        )
    } else if server_config.ca_pem_content.is_some()
        || server_config.crt_pem_content.is_some()
        || server_config.key_pem_content.is_some()
    {
        log::debug!("TLS CONFIG CHECKING GOT IN HERE 2");
        TLSConfig::new(
            server_config.insecure.unwrap(),
            server_config.ca_pem_content,
            server_config.crt_pem_content,
            server_config.key_pem_content,
        )
    } else {
        // Err("Unable to build mTLS configuration!".to_string())
        log::debug!("TLS CONFIG CHECKING GOT IN HERE 3");
        TLSConfig::new(
            server_config.insecure.unwrap(),
            server_config.ca_pem,
            server_config.crt_pem,
            server_config.key_pem,
        )
    };

    let mut communications_server = GRPCCommunicationsServer::new(
        to_server.clone(),
        // [impl->swdd~server-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
        tls_config.unwrap_or_exit("Missing certificates files"),
    );
    let mut server = AnkaiosServer::new(server_receiver, to_agents.clone());

    tokio::select! {
        // [impl->swdd~server-default-communication-grpc~1]
        communication_result = communications_server.start(agents_receiver, server_config.address.unwrap()) => {
            communication_result.unwrap_or_exit("server error")
        }

        server_result = server.start(startup_state) => {
            server_result.unwrap_or_exit("server error")
        }
    }
}

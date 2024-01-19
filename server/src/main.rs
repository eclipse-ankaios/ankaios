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
mod state_manipulation;
mod state_parser;
mod workload_state_db;

use std::fs;
use tokio::try_join;

use common::objects::State;
use common::std_extensions::GracefulExitResult;
use common::to_server_interface::ToServerInterface;
use common::{communications_server::CommunicationsServer, std_extensions::IllegalStateResult};

use ankaios_server::{create_execution_channels, create_state_change_channels, AnkaiosServer};

use grpc::server::GRPCCommunicationsServer;

type BoxedStdError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), BoxedStdError> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    log::debug!(
        "Starting the Ankaios server with \n\tserver address: '{}', \n\tstartup config path: '{}'",
        args.addr,
        args.path
            .clone()
            .unwrap_or("[no config file provided]".to_string()),
    );

    let state = match args.path {
        Some(config_path) => {
            let data =
                fs::read_to_string(config_path).unwrap_or_exit("Could not read the startup config");
            // [impl->swdd~server-state-in-memory~1]
            // [impl->swdd~server-loads-startup-state-file~2]
            let state: State =
                state_parser::parse(data).unwrap_or_exit("Parsing start config failed with error");
            log::trace!(
                "The state is initialized with the following workloads: {:?}",
                state.workloads
            );
            Some(state)
        }
        // [impl->swdd~server-starts-without-startup-config~1]
        _ => None,
    };

    let (to_server, server_receiver) = create_state_change_channels(common::CHANNEL_CAPACITY);
    let (to_agents, agents_receiver) = create_execution_channels(common::CHANNEL_CAPACITY);

    let mut server = AnkaiosServer::new(server_receiver, to_agents.clone());
    let mut communications_server = GRPCCommunicationsServer::new(to_server.clone());

    let server_task = tokio::spawn(async move { server.start().await });
    // [impl->swdd~server-default-communication-grpc~1]
    let communications_task = tokio::spawn(async move {
        communications_server
            .start(agents_receiver, args.addr)
            .await
            .unwrap_or_exit("Server startup error");
    });

    // This simulates the state handling.
    // Once the StartupStateLoader is there, it will be started by the main here and it will send the startup state
    if let Some(state) = state {
        to_server
            .update_state(
                "".to_owned(),
                common::commands::CompleteState {
                    startup_state: State::default(),
                    current_state: state,
                    workload_states: vec![],
                },
                vec![],
            )
            .await
            .unwrap_or_illegal_state();
    } else {
        // [impl->swdd~server-starts-without-startup-config~1]
        log::info!("No startup state provided -> waiting for new workloads from the CLI");
    }

    try_join!(communications_task, server_task).unwrap_or_illegal_state();

    Ok(())
}

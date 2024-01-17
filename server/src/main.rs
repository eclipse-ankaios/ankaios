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

use common::commands::CompleteState;
use std::fs;

use common::communications_server::CommunicationsServer;
use common::objects::State;
use common::std_extensions::GracefulExitResult;

use ankaios_server::{create_execution_channels, create_state_change_channels, AnkaiosServer};

use grpc::server::GRPCCommunicationsServer;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    log::debug!(
        "Starting the Ankaios server with \n\tserver address: {}, \n\tstartup config path: {}",
        args.addr,
        args.path,
    );

    let data = fs::read_to_string(args.path).unwrap_or_exit("Could not read the startup config");
    // [impl->swdd~server-state-in-memory~1]
    // [impl->swdd~server-loads-startup-state-file~1]
    let state: State =
        state_parser::parse(data).unwrap_or_exit("Parsing start config failed with error");
    log::trace!(
        "The state is initialized with the following workloads: {:?}",
        state.workloads
    );

    let (to_server, server_receiver) = create_state_change_channels(common::CHANNEL_CAPACITY);
    let (to_agents, agents_receiver) = create_execution_channels(common::CHANNEL_CAPACITY);

    let mut communications_server = GRPCCommunicationsServer::new(to_server.clone());
    let mut server = AnkaiosServer::new(server_receiver, to_agents.clone());

    tokio::select! {
        // [impl->swdd~server-default-communication-grpc~1]
        communication_result = communications_server.start(agents_receiver, args.addr) => {
            communication_result.unwrap_or_exit("server error")
        }
        server_result = server.start(CompleteState {
            current_state: state,
            ..Default::default()
        }) => {
            server_result.unwrap_or_exit("server error")
        }
    }
}

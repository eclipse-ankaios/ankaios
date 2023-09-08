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

use common::communications_server::CommunicationsServer;
use common::graceful_exit::ExitGracefully;
use common::objects::State;
use common::state_change_interface::StateChangeInterface;

use ankaios_server::{create_execution_channels, create_state_change_channels, AnkaiosServer};

use grpc::server::GRPCCommunicationsServer;

type BoxedStdError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), BoxedStdError> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let args = cli::parse();

    let data = fs::read_to_string(args.path).unwrap_or_exit("Could not read the startup state");
    // [impl->swdd~server-state-in-memory~1]
    // [impl->swdd~server-loads-startup-state-file~1]
    let state: State =
        state_parser::parse(data).unwrap_or_exit("Parsing start config failed with error");
    log::debug!(
        "The state is initialized with the following workloads: {:?}",
        state.workloads
    );

    let (to_server, server_receiver) = create_state_change_channels(common::CHANNEL_CAPACITY);
    let (to_agents, mut agents_receiver) = create_execution_channels(common::CHANNEL_CAPACITY);

    let mut server = AnkaiosServer::new(server_receiver, to_agents.clone());
    let mut communications_server = GRPCCommunicationsServer::new(to_server.clone());

    let server_task = tokio::spawn(async move { server.start().await });
    // [impl->swdd~server-default-communication-grpc~1]
    let communications_task = tokio::spawn(async move {
        communications_server
            .start(&mut agents_receiver, args.addr)
            .await
    });

    // This simulates the state handling.
    // Once the StartupStateLoader is there, it will be started by the main here and it will send the startup state
    let test_task = tokio::spawn(async move {
        to_server
            .update_state(
                common::commands::CompleteState {
                    request_id: "".to_owned(),
                    startup_state: State::default(),
                    current_state: state,
                    workload_states: vec![],
                },
                vec![],
            )
            .await
    });

    try_join!(communications_task, server_task, test_task).unwrap();

    Ok(())
}

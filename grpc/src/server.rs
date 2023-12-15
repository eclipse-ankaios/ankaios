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

use api::proto::cli_connection_server::CliConnectionServer;
use common::communications_error::CommunicationMiddlewareError;
use common::communications_server::CommunicationsServer;

use tonic::transport::Server;

use tokio::sync::mpsc::{Receiver, Sender};

use std::net::SocketAddr;

use crate::agent_senders_map::AgentSendersMap;
use crate::grpc_cli_connection::GRPCCliConnection;
use api::proto::agent_connection_server::AgentConnectionServer;

use crate::from_server_proxy;
use crate::grpc_agent_connection::GRPCAgentConnection;

use common::execution_interface::FromServer;
use common::state_change_interface::StateChangeCommand;

use async_trait::async_trait;

#[derive(Debug)]
pub struct GRPCCommunicationsServer {
    sender: Sender<StateChangeCommand>,
    agent_senders: AgentSendersMap,
}

#[async_trait]
impl CommunicationsServer for GRPCCommunicationsServer {
    async fn start(
        &mut self,
        receiver: &mut Receiver<FromServer>,
        addr: SocketAddr,
    ) -> Result<(), CommunicationMiddlewareError> {
        // [impl->swdd~grpc-server-creates-agent-connection~1]
        let my_connection =
            GRPCAgentConnection::new(self.agent_senders.clone(), self.sender.clone());

        // [impl->swdd~grpc-server-creates-cli-connection~1]
        let my_cli_connection =
            GRPCCliConnection::new(self.agent_senders.clone(), self.sender.clone());

        // [impl->swdd~grpc-server-spawns-tonic-service~1]
        // [impl->swdd~grpc-delegate-workflow-to-external-library~1]
        let grpc_task = tokio::spawn(async move {
            Server::builder()
                .add_service(AgentConnectionServer::new(my_connection))
                // [impl->swdd~grpc-server-provides-endpoint-for-cli-connection-handling~1]
                .add_service(CliConnectionServer::new(my_cli_connection))
                .serve(addr)
                .await
                .map_err(|err| {
                    CommunicationMiddlewareError(format!(
                        "Could not start the gRPC service: '{}'",
                        err
                    ))
                })?;
            Ok::<(), CommunicationMiddlewareError>(())
        });

        // TODO these two awaits one after the other do not seem correct ...
        // [impl->swdd~grpc-server-forwards-commands-to-grpc-client~1]
        from_server_proxy::forward_from_ankaios_to_proto(&self.agent_senders, receiver).await;

        grpc_task.await??;
        Ok(())
    }
}

impl GRPCCommunicationsServer {
    pub fn new(sender: Sender<StateChangeCommand>) -> Self {
        GRPCCommunicationsServer {
            agent_senders: AgentSendersMap::new(),
            sender,
        }
    }
}

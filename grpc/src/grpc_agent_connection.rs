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

use std::pin::Pin;

use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use tonic::codegen::futures_core::Stream;
use tonic::{Request, Response, Status};

use crate::agent_senders_map::AgentSendersMap;
use crate::to_server_proxy::{forward_from_proto_to_ankaios, GRPCToServerStreaming};
use common::to_server_interface::{self, ToServerInterface};
use crate::grpc_api::{self, agent_connection_server::AgentConnection, to_server::ToServerEnum};

#[derive(Debug)]
pub struct GRPCAgentConnection {
    agent_senders: AgentSendersMap,
    to_ankaios_server: Sender<to_server_interface::ToServer>,
}

impl GRPCAgentConnection {
    pub fn new(
        agent_senders: AgentSendersMap,
        to_ankaios_server: Sender<to_server_interface::ToServer>,
    ) -> Self {
        Self {
            agent_senders,
            to_ankaios_server,
        }
    }
}

#[tonic::async_trait]
impl AgentConnection for GRPCAgentConnection {
    type ConnectAgentStream =
        Pin<Box<dyn Stream<Item = Result<grpc_api::FromServer, Status>> + Send + 'static>>;

    // [impl->swdd~grpc-client-connects-with-agent-hello~1]
    async fn connect_agent(
        &self,
        request: Request<tonic::Streaming<grpc_api::ToServer>>,
    ) -> Result<Response<Self::ConnectAgentStream>, Status> {
        let mut stream = request.into_inner();

        // [impl->swdd~grpc-agent-connection-creates-from-server-channel~1]
        let (new_agent_sender, new_agent_receiver) = tokio::sync::mpsc::channel::<
            Result<grpc_api::FromServer, tonic::Status>,
        >(common::CHANNEL_CAPACITY);

        let ankaios_tx = self.to_ankaios_server.clone();
        let agent_senders = self.agent_senders.clone();

        // The first_message must be an agent hello
        match stream
            .message()
            .await?
            .ok_or_else(invalid_argument_empty)?
            .to_server_enum
            .ok_or_else(invalid_argument_empty)?
        {
            ToServerEnum::AgentHello(grpc_api::AgentHello { agent_name }) => {
                log::trace!("Received a hello from '{}'", agent_name);

                // [impl->swdd~grpc-agent-connection-stores-from-server-channel-tx~1]
                self.agent_senders
                    .insert(&agent_name, new_agent_sender.to_owned());
                // [impl->swdd~grpc-agent-connection-forwards-hello-to-ankaios-server~1]
                if let Err(error) = self.to_ankaios_server.agent_hello(agent_name.clone()).await {
                    log::error!("Could not send agent hello: '{error}'");
                }

                // [impl->swdd~grpc-agent-connection-forwards-commands-to-server~1]
                let _x = tokio::spawn(async move {
                    let mut stream = GRPCToServerStreaming::new(stream);
                    if let Err(error) = forward_from_proto_to_ankaios(
                        agent_name.clone(),
                        &mut stream,
                        ankaios_tx.clone(),
                    )
                    .await
                    {
                        log::warn!(
                            "Connection to agent {} interrupted with error: {}",
                            agent_name,
                            error
                        );

                        agent_senders.remove(&agent_name);
                        log::trace!(
                            "The connection is interrupted or has been closed. Deleting the agent sender '{}'",
                            agent_name
                        );
                        // inform also the server that the agent is gone
                        // [impl->swdd~grpc-agent-connection-sends-agent-gone~1]
                        if let Err(error) = ankaios_tx.agent_gone(agent_name).await {
                            log::error!("Could not inform server about gone agent: '{}'", error);
                        }
                    }
                });
            }
            _ => {
                panic!("No AgentHello received.");
            }
        }

        // [impl->swdd~grpc-agent-connection-responds-with-from-server-channel-rx~1]
        Ok(Response::new(Box::pin(ReceiverStream::new(
            new_agent_receiver,
        ))))
    }
}

fn invalid_argument_empty() -> Status {
    Status::invalid_argument("Empty")
}

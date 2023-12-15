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
use crate::state_change_proxy::{forward_from_proto_to_ankaios, GRPCStateChangeRequestStreaming};
use api::proto::agent_connection_server::AgentConnection;

use api::proto::to_server::ToServerEnum;
use api::proto::{AgentHello, FromServer, ToServer};

use common::state_change_interface::{StateChangeCommand, StateChangeInterface};

#[derive(Debug)]
pub struct GRPCAgentConnection {
    agent_senders: AgentSendersMap,
    to_ankaios_server: Sender<StateChangeCommand>,
}

impl GRPCAgentConnection {
    pub fn new(
        agent_senders: AgentSendersMap,
        to_ankaios_server: Sender<StateChangeCommand>,
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
        Pin<Box<dyn Stream<Item = Result<FromServer, Status>> + Send + 'static>>;

    // [impl->swdd~grpc-client-connects-with-agent-hello~1]
    async fn connect_agent(
        &self,
        request: Request<tonic::Streaming<ToServer>>,
    ) -> Result<Response<Self::ConnectAgentStream>, Status> {
        let mut stream = request.into_inner();

        // [impl->swdd~grpc-agent-connection-creates-execution-command-channel~1]
        let (new_agent_sender, new_agent_receiver) = tokio::sync::mpsc::channel::<
            Result<FromServer, tonic::Status>,
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
            ToServerEnum::AgentHello(AgentHello { agent_name }) => {
                log::trace!("Received a hello from '{}'", agent_name);

                // [impl->swdd~grpc-agent-connection-stores-execution-channel-tx~1]
                self.agent_senders
                    .insert(&agent_name, new_agent_sender.to_owned());
                // [impl->swdd~grpc-agent-connection-forwards-hello-to-ankaios-server~1]
                if let Err(error) = self.to_ankaios_server.agent_hello(agent_name.clone()).await {
                    log::error!("Could not send agent hello: '{error}'");
                }

                // [impl->swdd~grpc-agent-connection-forwards-commands-to-server~1]
                let _x = tokio::spawn(async move {
                    let mut stream = GRPCStateChangeRequestStreaming::new(stream);
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

        // [impl->swdd~grpc-agent-connection-responds-with-execution-channel-rx~1]
        Ok(Response::new(Box::pin(ReceiverStream::new(
            new_agent_receiver,
        ))))
    }
}

fn invalid_argument_empty() -> Status {
    Status::invalid_argument("Empty")
}

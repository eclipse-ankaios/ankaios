// Copyright (c) 2026 Elektrobit Automotive GmbH
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

use crate::client_senders_map::ClientSendersMap;
use crate::grpc_api::{self, command_connection_server::CommandConnection};
use crate::to_server::ToServerEnum;
use crate::to_server_proxy::{GRPCToServerStreaming, forward_from_proto_to_ankaios};
use common::{check_version_compatibility, to_server_interface};

use futures_core::Stream;
use std::pin::Pin;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

#[derive(Debug)]
pub struct GRPCCommanderConnection {
    commander_senders: ClientSendersMap,
    to_ankaios_server: Sender<to_server_interface::ToServer>,
}

impl GRPCCommanderConnection {
    pub fn new(
        commander_senders: ClientSendersMap,
        to_ankaios_server: Sender<to_server_interface::ToServer>,
    ) -> Self {
        Self {
            commander_senders,
            to_ankaios_server,
        }
    }
}

#[tonic::async_trait]
impl CommandConnection for GRPCCommanderConnection {
    type ConnectCommandStream =
        Pin<Box<dyn Stream<Item = Result<grpc_api::FromServer, Status>> + Send + 'static>>;

    // [impl->swdd~grpc-client-connects-with-unique-commander-connection-name~1]
    async fn connect_command(
        &self,
        request: Request<tonic::Streaming<grpc_api::ToServer>>,
    ) -> Result<Response<Self::ConnectCommandStream>, Status> {
        let mut stream = request.into_inner();

        // [impl->swdd~grpc-commander-connection-creates-from-server-channel~1]
        let (new_sender, new_receiver) = tokio::sync::mpsc::channel::<
            Result<grpc_api::FromServer, tonic::Status>,
        >(common::CHANNEL_CAPACITY);

        let commander_connection_name = format!("commander-conn-{}", uuid::Uuid::new_v4());
        log::debug!("Connection to commander (name={commander_connection_name}) open.");

        let ankaios_tx = self.to_ankaios_server.clone();
        let commander_senders = self.commander_senders.clone();

        // The first_message must be a commander hello
        match stream
            .message()
            .await?
            .ok_or(Status::invalid_argument("Empty"))?
            .to_server_enum
            .ok_or(Status::invalid_argument("Empty"))?
        {
            ToServerEnum::CommanderHello(grpc_api::CommanderHello { protocol_version }) => {
                log::trace!("Received a hello from a commander application.");

                // [impl->swdd~grpc-commander-connection-checks-version-compatibility~1]
                check_version_compatibility(&protocol_version).map_err(|err| {
                    log::warn!("Refused commander connection due to unsupported version: '{protocol_version}'");
                    Status::failed_precondition(err)})?;

                // [impl->swdd~grpc-commander-connection-stores-from-server-channel-tx~1]
                self.commander_senders
                    .insert(&commander_connection_name, new_sender);
                // [impl->swdd~grpc-commander-connection-forwards-commands-to-server~1]
                let _x = tokio::spawn(async move {
                    let mut stream = GRPCToServerStreaming::new(stream);
                    let result = forward_from_proto_to_ankaios(
                        commander_connection_name.clone(),
                        &mut stream,
                        ankaios_tx.clone(),
                    )
                    .await;
                    if result.is_err() {
                        log::debug!(
                            "Connection to commander (name={commander_connection_name}) failed with {result:?}."
                        );
                    }
                    commander_senders.remove(&commander_connection_name);
                    log::debug!(
                        "Connection to commander (name={commander_connection_name}) has been closed."
                    );
                });
                // [impl->swdd~grpc-commander-connection-responds-with-from-server-channel-rx~1]
                Ok(Response::new(Box::pin(ReceiverStream::new(new_receiver))))
            }
            _ => Err(Status::invalid_argument("No CommanderHello received.")),
        }
    }
}

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

use common::std_extensions::GracefulExitResult;
use common::{check_version_compatibility, to_server_interface};
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use tonic::codegen::futures_core::Stream;
use tonic::{Request, Response, Status};

use crate::agent_senders_map::AgentSendersMap;
use crate::to_server::ToServerEnum;
use crate::to_server_proxy::{forward_from_proto_to_ankaios, GRPCToServerStreaming};
use grpc_api::cli_connection_server::CliConnection;

use crate::grpc_api;

#[derive(Debug)]
pub struct GRPCCliConnection {
    cli_senders: AgentSendersMap,
    to_ankaios_server: Sender<to_server_interface::ToServer>,
}

impl GRPCCliConnection {
    pub fn new(
        cli_senders: AgentSendersMap,
        to_ankaios_server: Sender<to_server_interface::ToServer>,
    ) -> Self {
        Self {
            cli_senders,
            to_ankaios_server,
        }
    }
}

#[tonic::async_trait]
impl CliConnection for GRPCCliConnection {
    type ConnectCliStream =
        Pin<Box<dyn Stream<Item = Result<grpc_api::FromServer, Status>> + Send + 'static>>;

    // [impl->swdd~grpc-client-connects-with-unique-cli-connection-name~1]
    async fn connect_cli(
        &self,
        request: Request<tonic::Streaming<grpc_api::ToServer>>,
    ) -> Result<Response<Self::ConnectCliStream>, Status> {
        let mut stream = request.into_inner();

        // [impl->swdd~grpc-commander-connection-creates-from-server-channel~1]
        let (new_sender, new_receiver) = tokio::sync::mpsc::channel::<
            Result<grpc_api::FromServer, tonic::Status>,
        >(common::CHANNEL_CAPACITY);

        let cli_connection_name = format!("cli-conn-{}", uuid::Uuid::new_v4());
        log::debug!("Connection to CLI (name={}) open.", cli_connection_name);

        let ankaios_tx = self.to_ankaios_server.clone();
        let cli_senders = self.cli_senders.clone();

        // The first_message must be a commander hello
        match stream
            .message()
            .await?
            .ok_or(Status::invalid_argument("Empty"))?
            .to_server_enum
            .ok_or(Status::invalid_argument("Empty"))?
        {
            ToServerEnum::CommanderHello(grpc_api::CommanderHello { protocol_version }) => {
                log::trace!("Received a hello from a cli/commander application.");

                // [impl->swdd~grpc-commander-connection-checks-version-compatibility~1]
                check_version_compatibility(&protocol_version).map_err(|err| {
                    log::warn!("Refused cli/commander connection due to unsupported version: '{protocol_version}'");
                    Status::failed_precondition(err)})?;

                // [impl->swdd~grpc-commander-connection-stores-from-server-channel-tx~1]
                self.cli_senders.insert(&cli_connection_name, new_sender);
                // [impl->swdd~grpc-commander-connection-forwards-commands-to-server~1]
                let _x = tokio::spawn(async move {
                    let mut stream = GRPCToServerStreaming::new(stream);
                    let result = forward_from_proto_to_ankaios(
                        cli_connection_name.clone(),
                        &mut stream,
                        ankaios_tx.clone(),
                    )
                    .await;
                    if result.is_err() {
                        log::debug!(
                            "Connection to CLI (name={}) failed with {:?}.",
                            cli_connection_name,
                            result
                        );
                    }
                    cli_senders.remove(&cli_connection_name);
                    log::debug!(
                        "Connection to CLI (name={}) has been closed.",
                        cli_connection_name
                    );
                });
            }
            _ => Err::<(), &str>("No CommanderHello received.").unwrap_or_exit("Protocol error."),
        }

        // [impl->swdd~grpc-commander-connection-responds-with-from-server-channel-rx~1]
        Ok(Response::new(Box::pin(ReceiverStream::new(new_receiver))))
    }
}

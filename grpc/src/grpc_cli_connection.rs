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

use common::to_server_interface;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;

use tonic::codegen::futures_core::Stream;
use tonic::{Request, Response, Status};

use crate::agent_senders_map::AgentSendersMap;
use crate::to_server_proxy::{forward_from_proto_to_ankaios, GRPCToServerStreaming};
use api::proto::cli_connection_server::CliConnection;

use api::proto;

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
        Pin<Box<dyn Stream<Item = Result<proto::FromServer, Status>> + Send + 'static>>;

    // [impl->swdd~grpc-client-connects-with-unique-cli-connection-name~1]
    async fn connect_cli(
        &self,
        request: Request<tonic::Streaming<proto::ToServer>>,
    ) -> Result<Response<Self::ConnectCliStream>, Status> {
        let stream = request.into_inner();

        let (new_sender, new_receiver) = tokio::sync::mpsc::channel::<
            Result<proto::FromServer, tonic::Status>,
        >(common::CHANNEL_CAPACITY);

        let cli_connection_name = format!("cli-conn-{}", uuid::Uuid::new_v4());
        log::debug!("Connection to CLI (name={}) open.", cli_connection_name);

        let ankaios_tx = self.to_ankaios_server.clone();
        let cli_senders = self.cli_senders.clone();
        self.cli_senders.insert(&cli_connection_name, new_sender);
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

        Ok(Response::new(Box::pin(ReceiverStream::new(new_receiver))))
    }
}

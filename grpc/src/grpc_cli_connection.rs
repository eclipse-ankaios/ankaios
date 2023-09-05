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
use api::proto::cli_connection_server::CliConnection;

use api::proto::{ExecutionRequest, StateChangeRequest};

use common::state_change_interface::StateChangeCommand;

#[derive(Debug)]
pub struct GRPCCliConnection {
    cli_senders: AgentSendersMap,
    to_ankaios_server: Sender<StateChangeCommand>,
}

impl GRPCCliConnection {
    pub fn new(
        cli_senders: AgentSendersMap,
        to_ankaios_server: Sender<StateChangeCommand>,
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
        Pin<Box<dyn Stream<Item = Result<ExecutionRequest, Status>> + Send + 'static>>;

    // [impl->swdd~grpc-client-connects-with-unique-cli-connection-name~1]
    async fn connect_cli(
        &self,
        request: Request<tonic::Streaming<StateChangeRequest>>,
    ) -> Result<Response<Self::ConnectCliStream>, Status> {
        let stream = request.into_inner();

        let (new_sender, new_receiver) = tokio::sync::mpsc::channel::<
            Result<ExecutionRequest, tonic::Status>,
        >(common::CHANNEL_CAPACITY);

        let cli_connection_name = format!("cli-conn-{}", uuid::Uuid::new_v4());
        log::info!("Connection to CLI (name={}) open.", cli_connection_name);

        let ankaios_tx = self.to_ankaios_server.clone();
        let cli_senders = self.cli_senders.clone();
        self.cli_senders.insert(&cli_connection_name, new_sender);
        let _x = tokio::spawn(async move {
            let mut stream = GRPCStateChangeRequestStreaming::new(stream);
            if (forward_from_proto_to_ankaios(
                cli_connection_name.clone(),
                &mut stream,
                ankaios_tx.clone(),
            )
            .await)
                .is_err()
            {
                cli_senders.remove(&cli_connection_name);
                log::info!("Connection to CLI (name={}) closed.", cli_connection_name);
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(new_receiver))))
    }
}

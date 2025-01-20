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

use crate::grpc_api::cli_connection_server::CliConnectionServer;
use common::communications_error::CommunicationMiddlewareError;
use common::communications_server::CommunicationsServer;

use tonic::transport::{Certificate, Identity, Server};

use std::net::SocketAddr;

use crate::agent_senders_map::AgentSendersMap;
use crate::grpc_api::agent_connection_server::AgentConnectionServer;
use crate::grpc_cli_connection::GRPCCliConnection;
use crate::grpc_middleware_error::GrpcMiddlewareError;

use crate::security::{read_pem_file, TLSConfig};

use crate::from_server_proxy;
use crate::grpc_agent_connection::GRPCAgentConnection;

use common::from_server_interface::FromServerReceiver;
use common::to_server_interface::ToServerSender;

use async_trait::async_trait;

#[derive(Debug)]
pub struct GRPCCommunicationsServer {
    sender: ToServerSender,
    agent_senders: AgentSendersMap,
    tls_config: Option<TLSConfig>,
}

#[async_trait]
impl CommunicationsServer for GRPCCommunicationsServer {
    async fn start(
        &mut self,
        mut receiver: FromServerReceiver,
        addr: SocketAddr,
    ) -> Result<(), CommunicationMiddlewareError> {
        // [impl->swdd~grpc-server-creates-agent-connection~1]
        let my_connection =
            GRPCAgentConnection::new(self.agent_senders.clone(), self.sender.clone());

        // [impl->swdd~grpc-server-creates-cli-connection~1]
        let my_cli_connection =
            GRPCCliConnection::new(self.agent_senders.clone(), self.sender.clone());

        let agent_senders_clone = self.agent_senders.clone();

        match &self.tls_config {
            // [impl->swdd~grpc-server-activate-mtls-when-certificates-and-key-provided-upon-start~1]
            Some(tls_config) => {
                let ca_pem = &tls_config.path_to_ca_pem;
                let crt_pem = &tls_config.path_to_crt_pem;
                let key_pem = &tls_config.path_to_key_pem;

                // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
                let ca = read_pem_file(ca_pem, false)
                    .map_err(|err| CommunicationMiddlewareError(err.to_string()))?;
                // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
                let cert = read_pem_file(crt_pem, false)
                    .map_err(|err| CommunicationMiddlewareError(err.to_string()))?;
                let key = read_pem_file(key_pem, true)
                    .map_err(|err| CommunicationMiddlewareError(err.to_string()))?;

                let server_identity = Identity::from_pem(cert, key);
                let tls = tonic::transport::ServerTlsConfig::new()
                    .client_ca_root(Certificate::from_pem(ca))
                    .identity(server_identity);
                tokio::select! {
                    // [impl->swdd~grpc-server-spawns-tonic-service~1]
                    // [impl->swdd~grpc-delegate-workflow-to-external-library~1]
                    result = Server::builder()
                        .tls_config(tls).map_err(|err| CommunicationMiddlewareError(err.to_string()))?
                        .add_service(AgentConnectionServer::new(my_connection))
                        // [impl->swdd~grpc-server-provides-endpoint-for-cli-connection-handling~1]
                        .add_service(CliConnectionServer::new(my_cli_connection))
                        .serve(addr) => {
                            result.map_err(|err| {
                                GrpcMiddlewareError::StartError(format!("{err:?}"))
                            })?
                        }
                    // [impl->swdd~grpc-server-forwards-from-server-messages-to-grpc-client~1]
                    _ = from_server_proxy::forward_from_ankaios_to_proto(
                        &agent_senders_clone,
                        &mut receiver,
                    ) => {
                        Err(GrpcMiddlewareError::ConnectionInterrupted(
                            "Connection between Ankaios server and the communication middleware dropped.".into())
                        )?
                    }
                }
            }
            // [impl->swdd~grpc-server-deactivate-mtls-when-no-certificates-and-no-key-provided-upon-start~1]
            None => {
                log::warn!(
                            "!!!ANKSERVER IS STARTED IN INSECURE MODE (-k, --insecure) -> TLS is disabled!!!"
                        );
                tokio::select! {
                    // [impl->swdd~grpc-server-spawns-tonic-service~1]
                    // [impl->swdd~grpc-delegate-workflow-to-external-library~1]
                    result = Server::builder()
                        .add_service(AgentConnectionServer::new(my_connection))
                        // [impl->swdd~grpc-server-provides-endpoint-for-cli-connection-handling~1]
                        .add_service(CliConnectionServer::new(my_cli_connection))
                        .serve(addr) => {
                            result.map_err(|err| {
                                GrpcMiddlewareError::StartError(format!("{err:?}"))
                            })?
                        }
                    // [impl->swdd~grpc-server-forwards-from-server-messages-to-grpc-client~1]
                    _ = from_server_proxy::forward_from_ankaios_to_proto(
                        &agent_senders_clone,
                        &mut receiver,
                    ) => {
                        Err(GrpcMiddlewareError::ConnectionInterrupted(
                            "Connection between Ankaios server and the communication middleware dropped.".into())
                        )?
                    }

                }
            }
        }
        Ok(())
    }
}

impl GRPCCommunicationsServer {
    pub fn new(sender: ToServerSender, tls_config: Option<TLSConfig>) -> Self {
        GRPCCommunicationsServer {
            agent_senders: AgentSendersMap::new(),
            sender,
            tls_config,
        }
    }
}

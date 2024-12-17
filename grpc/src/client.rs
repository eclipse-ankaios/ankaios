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

use std::path::Path;

use crate::from_server_proxy::GRPCFromServerStreaming;
use crate::grpc_api::{
    self, agent_connection_client::AgentConnectionClient,
    cli_connection_client::CliConnectionClient, to_server::ToServerEnum, AgentHello,
};
use crate::grpc_middleware_error::GrpcMiddlewareError;
use crate::security::{read_pem_file, TLSConfig};
use crate::to_server_proxy;
use crate::{from_server_proxy, CommanderHello};

use common::communications_client::CommunicationsClient;
use common::communications_error::CommunicationMiddlewareError;
use common::from_server_interface::FromServerSender;

use common::std_extensions::IllegalStateResult;
use common::to_server_interface::ToServerReceiver;

use regex::Regex;
use tokio::select;
use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;

use async_trait::async_trait;

use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

const RECONNECT_TIMEOUT_SECONDS: u64 = 1;

enum ConnectionType {
    Agent,
    Cli,
}

pub struct GRPCCommunicationsClient {
    name: String,
    server_address: String,
    connection_type: ConnectionType,
    tls_config: Option<TLSConfig>,
}

fn get_server_url(server_address: &str, tls_config: &Option<TLSConfig>) -> String {
    if tls_config.is_none() {
        server_address.replace("https:", "http:")
    } else {
        server_address.to_owned()
    }
}

fn verify_address_format(server_address: &String) -> Result<(), CommunicationMiddlewareError> {
    let re = Regex::new(r"^https?:\/\/.+").unwrap_or_illegal_state();
    if !re.is_match(server_address) {
        return Err(CommunicationMiddlewareError(format!(
            "Wrong server address format: '{}'.",
            server_address
        )));
    }
    Ok(())
}

impl GRPCCommunicationsClient {
    pub fn new_agent_communication(
        name: String,
        server_address: String,
        tls_config: Option<TLSConfig>,
    ) -> Result<Self, CommunicationMiddlewareError> {
        verify_address_format(&server_address)?;

        Ok(Self {
            name,
            server_address: get_server_url(&server_address, &tls_config),
            connection_type: ConnectionType::Agent,
            tls_config,
        })
    }

    pub fn new_cli_communication(
        name: String,
        server_address: String,
        tls_config: Option<TLSConfig>,
    ) -> Result<Self, CommunicationMiddlewareError> {
        verify_address_format(&server_address)?;

        Ok(Self {
            name,
            server_address: get_server_url(&server_address, &tls_config),
            connection_type: ConnectionType::Cli,
            tls_config,
        })
    }
}

#[async_trait]
impl CommunicationsClient for GRPCCommunicationsClient {
    async fn run(
        &mut self,
        mut server_rx: ToServerReceiver,
        agent_tx: FromServerSender,
    ) -> Result<(), CommunicationMiddlewareError> {
        log::debug!("gRPC Communication Client starts.");

        // [impl->swdd~grpc-client-retries-connection~2]
        loop {
            let result = self.run_internal(&mut server_rx, &agent_tx).await;

            // Take care of general errors
            if let Err(GrpcMiddlewareError::VersionMismatch(err)) = result {
                return Err(CommunicationMiddlewareError(format!(
                    "Ankaios version mismatch: '{}'.",
                    err
                )));
            }

            match self.connection_type {
                ConnectionType::Agent => {
                    log::warn!("Connection to server interrupted: '{:?}'", result);

                    use tokio::time::{sleep, Duration};
                    sleep(Duration::from_secs(RECONNECT_TIMEOUT_SECONDS)).await;
                }
                ConnectionType::Cli => {
                    match result {
                        // [impl->swdd~grpc-client-outputs-error-server-unavailability-for-cli-connection~1]
                        Err(GrpcMiddlewareError::ServerNotAvailable(err)) => {
                            log::debug!("No connection to the server: '{err}'");
                            return Err(CommunicationMiddlewareError(format!(
                                "Could not connect to Ankaios server on '{}'.",
                                self.server_address
                            )));
                        }
                        // [impl->swdd~grpc-client-outputs-error-server-connection-loss-for-cli-connection~1]
                        Err(GrpcMiddlewareError::ConnectionInterrupted(err)) => {
                            log::debug!(
                                "The connection to the Ankaios Server was interrupted: '{err}'"
                            );
                        }
                        Err(GrpcMiddlewareError::CertificateError(err)) => {
                            return Err(CommunicationMiddlewareError(format!(
                                "Certificate error: '{}'.",
                                err
                            )));
                        }
                        _ => {
                            log::debug!("The connection to the Ankaios Server was closed.");
                        }
                    }
                    // [impl->swdd~grpc-client-never-retries-cli-connection~1]
                    break; // no retry of cli connection
                }
            }
        }

        Ok(())
    }
}

impl GRPCCommunicationsClient {
    /// This functions establishes the connection to the gRPC server and starts listening and forwarding messages
    /// on the two communications channels. The method returns only if the connection could not be established or
    /// is interrupted.
    async fn run_internal(
        &self,
        server_rx: &mut ToServerReceiver,
        agent_tx: &FromServerSender,
    ) -> Result<(), GrpcMiddlewareError> {
        // [impl->swdd~grpc-client-creates-to-server-channel~1]
        let (grpc_tx, grpc_rx) =
            tokio::sync::mpsc::channel::<grpc_api::ToServer>(common::CHANNEL_CAPACITY);

        // [impl->swdd~grpc-client-sends-supported-version~1]
        match self.connection_type {
            ConnectionType::Agent => {
                grpc_tx
                    .send(grpc_api::ToServer {
                        to_server_enum: Some(ToServerEnum::AgentHello(AgentHello::new(&self.name))),
                    })
                    .await?;
            }
            ConnectionType::Cli => {
                grpc_tx
                    .send(grpc_api::ToServer {
                        to_server_enum: Some(ToServerEnum::CommanderHello(CommanderHello::new())),
                    })
                    .await?;
            }
        }

        // [impl->swdd~grpc-client-connects-with-agent-hello~1]
        let mut grpc_to_server_streaming =
            GRPCFromServerStreaming::new(self.connect_to_server(grpc_rx).await?);

        // [impl->swdd~grpc-client-forwards-from-server-messages-to-agent~1]
        let forward_exec_from_proto_task = from_server_proxy::forward_from_proto_to_ankaios(
            &mut grpc_to_server_streaming,
            agent_tx,
        );

        // [impl->swdd~grpc-client-forwards-commands-to-grpc-agent-connection~1]
        let forward_to_server_from_ank_task =
            to_server_proxy::forward_from_ankaios_to_proto(grpc_tx, server_rx);

        select! {
            _ = forward_exec_from_proto_task => {log::debug!("Forward from server message from proto to Ankaios task completed");}
            _ = forward_to_server_from_ank_task => {log::debug!("Forward from server message from Ankaios to proto task completed");}
        };

        Ok(())
    }

    async fn connect_to_server(
        &self,
        grpc_rx: Receiver<grpc_api::ToServer>,
    ) -> Result<tonic::Streaming<grpc_api::FromServer>, GrpcMiddlewareError> {
        match self.connection_type {
            ConnectionType::Agent => match &self.tls_config {
                // [impl->swdd~grpc-agent-activate-mtls-when-certificates-and-key-provided-upon-start~1]
                Some(tls_config) => {
                    // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
                    let ca_pem = read_pem_file(Path::new(&tls_config.path_to_ca_pem), false)?;
                    let ca = Certificate::from_pem(ca_pem);
                    // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
                    let client_cert_pem =
                        read_pem_file(Path::new(&tls_config.path_to_crt_pem), false)?;
                    let client_cert = Certificate::from_pem(client_cert_pem);

                    // [impl->swdd~grpc-supports-pem-file-format-for-keys~1]
                    let client_key_pem =
                        read_pem_file(Path::new(&tls_config.path_to_key_pem), true)?;
                    let client_key = Certificate::from_pem(client_key_pem);
                    let client_identity = Identity::from_pem(client_cert, client_key);

                    let tls = ClientTlsConfig::new()
                        .domain_name("ank-server")
                        .ca_certificate(ca)
                        .identity(client_identity);

                    let channel = Channel::from_shared(self.server_address.to_string())
                        .map_err(|err| GrpcMiddlewareError::TLSError(err.to_string()))?
                        .tls_config(tls)?
                        .connect()
                        .await?;
                    let mut client = AgentConnectionClient::new(channel);

                    let res = client
                        .connect_agent(ReceiverStream::new(grpc_rx))
                        .await?
                        .into_inner();
                    Ok(res)
                }
                // [impl->swdd~grpc-agent-deactivate-mtls-when-no-certificates-and-no-key-provided-upon-start~1]
                None => {
                    let mut client =
                        AgentConnectionClient::connect(self.server_address.to_string()).await?;

                    let res = client
                        .connect_agent(ReceiverStream::new(grpc_rx))
                        .await?
                        .into_inner();
                    Ok(res)
                }
            },
            ConnectionType::Cli => match &self.tls_config {
                // [impl->swdd~grpc-cli-activate-mtls-when-certificates-and-key-provided-upon-start~1]
                Some(tls_config) => {
                    // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
                    let ca_pem = read_pem_file(Path::new(&tls_config.path_to_ca_pem), false)?;
                    let ca = Certificate::from_pem(ca_pem);
                    // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
                    let client_cert_pem =
                        read_pem_file(Path::new(&tls_config.path_to_crt_pem), false)?;
                    let client_cert = Certificate::from_pem(client_cert_pem);

                    // [impl->swdd~grpc-supports-pem-file-format-for-keys~1]
                    let client_key_pem =
                        read_pem_file(Path::new(&tls_config.path_to_key_pem), true)?;
                    let client_key = Certificate::from_pem(client_key_pem);
                    let client_identity = Identity::from_pem(client_cert, client_key);

                    let tls = ClientTlsConfig::new()
                        .domain_name("ank-server")
                        .ca_certificate(ca)
                        .identity(client_identity);

                    let channel = Channel::from_shared(self.server_address.to_string())
                        .map_err(|err| GrpcMiddlewareError::TLSError(err.to_string()))?
                        .tls_config(tls)?
                        .connect()
                        .await?;
                    let mut client = CliConnectionClient::new(channel);

                    let res = client
                        .connect_cli(ReceiverStream::new(grpc_rx))
                        .await?
                        .into_inner();
                    Ok(res)
                }
                // [impl->swdd~grpc-cli-deactivate-mtls-when-no-certificates-and-no-key-provided-upon-start~1]
                None => {
                    let mut client =
                        CliConnectionClient::connect(self.server_address.to_string()).await?;

                    let res = client
                        .connect_cli(ReceiverStream::new(grpc_rx))
                        .await?
                        .into_inner();
                    Ok(res)
                }
            },
        }
    }
}

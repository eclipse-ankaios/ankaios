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

use crate::client_senders_map::ClientSendersMap;
use crate::from_server_proxy;
use crate::grpc_agent_connection::GRPCAgentConnection;
use crate::grpc_api::agent_connection_server::AgentConnectionServer;
use crate::grpc_api::cli_connection_server::CliConnectionServer;
use crate::grpc_api::command_connection_server::CommandConnectionServer;
use crate::grpc_cli_connection::GRPCCliConnection;
use crate::grpc_commander_connection::GRPCCommanderConnection;
use crate::grpc_middleware_error::GrpcMiddlewareError;
use crate::security::TLSConfig;

use common::communications_error::CommunicationMiddlewareError;
use common::communications_server::{CommunicationsServer, ServerConnection};
use common::from_server_interface::FromServerReceiver;
use common::to_server_interface::ToServerSender;

use async_trait::async_trait;
use nix::unistd::{Group, chown};
use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};

#[derive(Debug)]
pub struct GRPCCommunicationsServer {
    sender: ToServerSender,
    agent_senders: ClientSendersMap,
    commander_senders: ClientSendersMap,
    tls_config: Option<TLSConfig>,
    unix_socket_group: Option<String>,
}

#[async_trait]
impl CommunicationsServer for GRPCCommunicationsServer {
    async fn start(
        &mut self,
        mut receiver: FromServerReceiver,
        addr: ServerConnection,
    ) -> Result<(), CommunicationMiddlewareError> {
        // [impl->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
        // [impl->swdd~grpc-server-creates-agent-connection~1]
        let my_connection =
            GRPCAgentConnection::new(self.agent_senders.clone(), self.sender.clone());

        // [impl->swdd~grpc-server-creates-cli-connection~1]
        let my_cli_connection =
            GRPCCliConnection::new(self.agent_senders.clone(), self.sender.clone());

        // [impl->swdd~grpc-server-creates-commander-connection~1]
        let my_commander_connection =
            GRPCCommanderConnection::new(self.commander_senders.clone(), self.sender.clone());

        let agent_senders_clone = self.agent_senders.clone();
        let commander_senders_clone = self.commander_senders.clone();

        match (&self.tls_config, addr) {
            // [impl->swdd~grpc-server-activate-mtls-when-certificates-and-key-provided-upon-start~1]
            (Some(tls_config), ServerConnection::Tcp(tcp_addr)) => {
                let ca = &tls_config.ca_pem;
                let cert = &tls_config.crt_pem;
                let key = &tls_config.key_pem;

                let server_identity = Identity::from_pem(cert, key);
                let tls = ServerTlsConfig::new()
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
                        // [impl->swdd~grpc-commander-uses-dedicated-server-endpoint~1]
                        // [impl->swdd~grpc-server-provides-endpoint-for-commander-connection-handling~1]
                        .add_service(CommandConnectionServer::new(my_commander_connection))
                        .serve(tcp_addr) => {
                            result.map_err(|err| {
                                GrpcMiddlewareError::StartError(format!("{err:?}"))
                            })?
                        }
                    // [impl->swdd~grpc-server-forwards-from-server-messages-to-grpc-client~1]
                    _ = from_server_proxy::forward_from_ankaios_to_proto(
                        &agent_senders_clone,
                        &commander_senders_clone,
                        &mut receiver,
                    ) => {
                        Err(GrpcMiddlewareError::ConnectionInterrupted(
                            "Connection between Ankaios server and the communication middleware dropped.".into())
                        )?
                    }
                }
            }
            // [impl->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
            (Some(_), ServerConnection::Unix(_)) => Err(CommunicationMiddlewareError(
                "Invalid runtime config: TLS is not supported for unix:// endpoints".to_string(),
            ))?,
            // [impl->swdd~grpc-server-deactivate-mtls-when-no-certificates-and-no-key-provided-upon-start~1]
            (None, ServerConnection::Tcp(tcp_addr)) => {
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
                        // [impl->swdd~grpc-server-provides-endpoint-for-commander-connection-handling~1]
                        .add_service(CommandConnectionServer::new(my_commander_connection))
                        .serve(tcp_addr) => {
                            result.map_err(|err| {
                                GrpcMiddlewareError::StartError(format!("{err:?}"))
                            })?
                        }
                    // [impl->swdd~grpc-server-forwards-from-server-messages-to-grpc-client~1]
                    _ = from_server_proxy::forward_from_ankaios_to_proto(
                        &agent_senders_clone,
                        &commander_senders_clone,
                        &mut receiver,
                    ) => {
                        Err(GrpcMiddlewareError::ConnectionInterrupted(
                            "Connection between Ankaios server and the communication middleware dropped.".into())
                        )?
                    }

                }
            }
            // [impl->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
            (None, ServerConnection::Unix(unix_socket_path)) => {
                let listener =
                    prepare_unix_listener(&unix_socket_path, self.unix_socket_group.as_deref())?;
                let incoming = UnixListenerStream::new(listener);

                tokio::select! {
                    result = Server::builder()
                        .add_service(AgentConnectionServer::new(my_connection))
                        .add_service(CliConnectionServer::new(my_cli_connection))
                        .add_service(CommandConnectionServer::new(my_commander_connection))
                        .serve_with_incoming(incoming) => {
                            result.map_err(|err| {
                                GrpcMiddlewareError::StartError(format!("{err:?}"))
                            })?
                        }
                    _ = from_server_proxy::forward_from_ankaios_to_proto(
                        &agent_senders_clone,
                        &commander_senders_clone,
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

fn prepare_unix_listener(
    socket_path: &Path,
    socket_group: Option<&str>,
) -> Result<UnixListener, CommunicationMiddlewareError> {
    if socket_path.exists() {
        let metadata = fs::metadata(socket_path).map_err(|err| {
            CommunicationMiddlewareError(format!(
                "Could not access existing unix socket path '{}': {err}",
                socket_path.display()
            ))
        })?;

        if metadata.file_type().is_socket() {
            fs::remove_file(socket_path).map_err(|err| {
                CommunicationMiddlewareError(format!(
                    "Could not remove stale unix socket '{}': {err}",
                    socket_path.display()
                ))
            })?;
        } else {
            return Err(CommunicationMiddlewareError(format!(
                "Unix socket path '{}' exists and is not a socket file",
                socket_path.display()
            )));
        }
    }

    let listener = UnixListener::bind(socket_path).map_err(|err| {
        CommunicationMiddlewareError(format!(
            "Could not bind unix socket '{}': {err}",
            socket_path.display()
        ))
    })?;

    if let Some(group_name) = socket_group {
        // [impl->swdd~server-configures-unix-domain-socket-group~1]
        let group = Group::from_name(group_name)
            .map_err(|err| {
                CommunicationMiddlewareError(format!(
                    "Could not resolve unix socket group '{}': {err}",
                    group_name
                ))
            })?
            .ok_or_else(|| {
                CommunicationMiddlewareError(format!(
                    "Could not resolve unix socket group '{}': group does not exist",
                    group_name
                ))
            })?;

        chown(socket_path, None, Some(group.gid)).map_err(|err| {
            CommunicationMiddlewareError(format!(
                "Could not set unix socket group '{}' on '{}': {err}",
                group_name,
                socket_path.display()
            ))
        })?;

        fs::set_permissions(socket_path, fs::Permissions::from_mode(0o660)).map_err(|err| {
            CommunicationMiddlewareError(format!(
                "Could not set unix socket permissions on '{}': {err}",
                socket_path.display()
            ))
        })?;
    }

    Ok(listener)
}

impl GRPCCommunicationsServer {
    pub fn new(sender: ToServerSender, tls_config: Option<TLSConfig>) -> Self {
        GRPCCommunicationsServer {
            agent_senders: ClientSendersMap::new(),
            commander_senders: ClientSendersMap::new(),
            sender,
            tls_config,
            unix_socket_group: None,
        }
    }

    pub fn with_unix_socket_group(mut self, unix_socket_group: Option<String>) -> Self {
        // [impl->swdd~server-configures-unix-domain-socket-group~1]
        self.unix_socket_group = unix_socket_group;
        self
    }
}

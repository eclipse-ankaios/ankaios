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

pub mod ankaios_streaming {
    use tonic::async_trait;

    #[async_trait]
    pub trait GRPCStreaming<T> {
        async fn message(&mut self) -> Result<Option<T>, tonic::Status>;
    }
}

pub mod grpc_middleware_error;

pub mod security {
    use crate::grpc_middleware_error::GrpcMiddlewareError;
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    #[derive(Debug, Default, Clone)]
    pub struct TLSConfig {
        pub path_to_ca_pem: String,
        pub path_to_crt_pem: String,
        pub path_to_key_pem: String,
    }

    pub fn check_and_read_pem_file(path_of_pem_file: &Path) -> Result<String, GrpcMiddlewareError> {
        let mut file = File::open(path_of_pem_file).map_err(|err| {
            GrpcMiddlewareError::CertificateError(format!(
                "Error during opening the given file {:?}: {}",
                path_of_pem_file, err
            ))
        })?;
        let permissions = file
            .metadata()
            .map_err(|err| {
                GrpcMiddlewareError::CertificateError(format!(
                    "Error during getting permissions of the given file {:?}: {}",
                    path_of_pem_file, err
                ))
            })?
            .permissions();
        let owner_readable = (permissions.mode() & 0o400) == 0o400; // read for the owner
        let group_not_readable = (permissions.mode() & 0o040) != 0o040; // not read for the group
        let others_not_readable = (permissions.mode() & 0o004) != 0o004; // not read for others

        if owner_readable && group_not_readable && others_not_readable {
            let mut buffer = String::new();
            file.read_to_string(&mut buffer).map_err(|err| {
                GrpcMiddlewareError::CertificateError(format!(
                    "Error during reading the given file {:?}: {:?}",
                    path_of_pem_file, err
                ))
            })?;
            Ok(buffer)
        } else {
            Err(GrpcMiddlewareError::CertificateError(format!(
                "The given certificate file {:?} has incorrect permissions!",
                path_of_pem_file
            )))
        }
    }
}

mod agent_senders_map;
pub mod client;
mod from_server_proxy;
mod grpc_agent_connection;
mod grpc_cli_connection;
pub mod server;
mod to_server_proxy;

use api::ank_base;
pub mod grpc_api;
pub use crate::grpc_api::*;

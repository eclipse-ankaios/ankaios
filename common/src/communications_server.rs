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

use crate::{
    communications_error::CommunicationMiddlewareError, from_server_interface::FromServerReceiver,
};

use async_trait::async_trait;
use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerConnection {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl fmt::Display for ServerConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerConnection::Tcp(addr) => write!(f, "{addr}"),
            ServerConnection::Unix(path) => write!(f, "unix://{}", path.display()),
        }
    }
}

// [impl->swdd~common-interface-definitions~1]
#[async_trait]
pub trait CommunicationsServer {
    async fn start(
        &mut self,
        mut receiver: FromServerReceiver,
        addr: ServerConnection,
    ) -> Result<(), CommunicationMiddlewareError>;
}

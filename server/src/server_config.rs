// Copyright (c) 2025 Elektrobit Automotive GmbH
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

use serde::{Deserialize, Deserializer, Serialize};
use std::fs::read_to_string;
use std::net::SocketAddr;
use std::str::FromStr;
use toml::from_str;

use crate::cli::Arguments;
use common::DEFAULT_SOCKET_ADDRESS;

const CONFIG_VERSION: &str = "v1";

pub fn get_default_address() -> Option<SocketAddr> {
    DEFAULT_SOCKET_ADDRESS.parse().ok()
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConversionErrors {
    WrongVersion(String),
    ConflictingCertificates(String),
}

fn convert_to_socket_address<'de, D>(deserializer: D) -> Result<Option<SocketAddr>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;

    Ok(s.parse().ok())
}

#[derive(Deserialize, Debug, Serialize)]
pub struct ServerConfig {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_config: Option<String>,
    #[serde(
        // the default att is useless
        default = "get_default_address",
        // deserialize_with = "convert_to_socket_address"
    )]
    pub address: Option<SocketAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub insecure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_pem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crt_pem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_pem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_pem_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crt_pem_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_pem_content: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            version: "v1".to_string(),
            startup_config: None,
            address: get_default_address(),
            insecure: Some(true),
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
            ca_pem_content: None,
            crt_pem_content: None,
            key_pem_content: None,
        }
    }
}

impl ServerConfig {
    pub fn from_file(file_path: &str) -> Result<ServerConfig, ConversionErrors> {
        let config_file_content = read_to_string(file_path).unwrap_or_default();
        let server_config: ServerConfig = from_str(&config_file_content).unwrap_or_default();

        if server_config.version != CONFIG_VERSION {
            return Err(ConversionErrors::WrongVersion(server_config.version));
        }

        if (server_config.ca_pem.is_some()
            || server_config.crt_pem.is_some()
            || server_config.key_pem.is_some())
            && (server_config.ca_pem_content.is_some()
                || server_config.crt_pem_content.is_some()
                || server_config.key_pem_content.is_some())
        {
            return Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string(),
            ));
        }

        Ok(server_config)
    }

    pub fn update_with_args(&mut self, args: &Arguments) {
        if let Some(path) = &args.path {
            self.startup_config = Some(path.to_string());
        }

        if let Some(addr) = &args.addr {
            self.address = Some(*addr);
        }

        self.insecure = Some(args.insecure);

        self.ca_pem = if let Some(ca_pem) = &args.ca_pem {
            Some(ca_pem.to_string())
        } else {
            self.ca_pem_content.clone()
        };
        self.crt_pem = if let Some(crt_pem) = &args.crt_pem {
            Some(crt_pem.to_string())
        } else {
            self.crt_pem_content.clone()
        };
        self.key_pem = args.key_pem.clone();
        self.key_pem = if let Some(key_pem) = &args.key_pem {
            Some(key_pem.to_string())
        } else {
            self.key_pem_content.clone()
        };
    }
}

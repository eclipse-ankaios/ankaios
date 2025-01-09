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

use serde::{de::Error, Deserialize, Deserializer};
use std::fmt;
use std::fs::read_to_string;
use std::net::SocketAddr;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use toml::from_str;
use toml::Value;

use crate::cli::Arguments;
use common::DEFAULT_SOCKET_ADDRESS;

// #[derive(Deserialize)]
// pub enum PemEnum {
//     Pem(Box<Path>),
//     PemContent(String),
// }

const CONFIG_VERSION: &str = "v1";

fn get_default_address() -> Option<SocketAddr> {
    Some(DEFAULT_SOCKET_ADDRESS.parse().unwrap())
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct InvalidAddressError;

impl fmt::Display for InvalidAddressError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid address provided")
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ConflictingArgumentsError;

impl fmt::Display for ConflictingArgumentsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "You can either provide crt_pem or crt_pem_content, but not both."
        )
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MissingVersionError;

impl fmt::Display for MissingVersionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Missing configuration version")
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConversionErrors {
    MissingVersion(MissingVersionError),
    WrongVersion(String),
    ConflictingArguments(ConflictingArgumentsError),
    InvalidAddressError(InvalidAddressError),
}

fn convert_to_socket_address<'de, D>(deserializer: D) -> Result<Option<SocketAddr>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;

    Ok(s.parse().ok())
}

#[derive(Deserialize, Debug, Default)]
pub struct ServerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(
        default = "get_default_address",
        deserialize_with = "convert_to_socket_address"
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

impl TryFrom<Value> for ServerConfig {
    type Error = ConversionErrors;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if let Some(config_version) = value["version"].as_str() {
            if config_version != CONFIG_VERSION {
                return Err(ConversionErrors::WrongVersion(format!(
                    "The configuration file version {} does not match the expected version {}",
                    config_version, CONFIG_VERSION
                )));
            }
        } else {
            return Err(ConversionErrors::MissingVersion(MissingVersionError));
        };

        if value.get("ca_pem").is_some() && value.get("ca_pem_content").is_some() {
            return Err(ConversionErrors::ConflictingArguments(
                ConflictingArgumentsError,
            ));
        }

        Ok(ServerConfig::default())
    }
}

impl ServerConfig {
    pub fn from_file(file_path: &str) -> Result<ServerConfig, ConversionErrors> {
        let config_file_content = read_to_string(file_path).unwrap();
        let server_config: Value = from_str(&config_file_content).unwrap();

        println!("{:?}", server_config);

        Ok(ServerConfig::try_from(server_config)?)
    }

    pub fn update_with_args(&mut self, args: &Arguments) {
        println!("ARGS: {:?}", args);

        if let Some(path) = &args.path {
            self.startup_config = Some(path.to_string());
        }
        if let Some(addr) = &args.addr {
            self.address = Some(*addr);
        }
        if let Some(insecure) = args.insecure {
            self.insecure = Some(insecure);
            if !insecure {
                self.ca_pem = args.ca_pem.clone();
                self.crt_pem = args.crt_pem.clone();
                self.key_pem = args.key_pem.clone();
            }
        }
    }
}

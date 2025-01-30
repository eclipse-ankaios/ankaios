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

use crate::cli::Arguments;
use common::DEFAULT_SOCKET_ADDRESS;
use serde::{Deserialize, Deserializer, Serialize};
use std::net::SocketAddr;
use toml::from_str;

const CONFIG_VERSION: &str = "v1";
pub const DEFAULT_SERVER_CONFIG_FILE_PATH: &str = "/etc/ankaios/ank-server.conf";

#[cfg(not(test))]
use std::fs::read_to_string;

#[cfg(test)]
fn read_to_string(file_path_content: &str) -> std::io::Result<String> {
    Ok(file_path_content.to_string())
}

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

    Ok(s.parse::<SocketAddr>().ok())
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct ServerConfig {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_config: Option<String>,
    #[serde(
        deserialize_with = "convert_to_socket_address",
        default = "get_default_address"
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

        println!("FROM_FILE SERVER CONFIG: {:?}", server_config);

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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use common::DEFAULT_SOCKET_ADDRESS;

    use crate::{cli::Arguments, server_config::ConversionErrors};

    use super::ServerConfig;

    #[test]
    fn utest_default_server_config() {
        let default_server_config = ServerConfig::default();

        assert_eq!(
            default_server_config.address,
            DEFAULT_SOCKET_ADDRESS.parse::<SocketAddr>().ok()
        );
        assert_eq!(default_server_config.insecure, Some(true));
        assert_eq!(default_server_config.version, "v1");
    }

    #[test]
    fn utest_server_config_wrong_version() {
        let server_config_content: &str = r"#
        version = 'v2'
        #";

        let server_config = ServerConfig::from_file(server_config_content);

        assert_eq!(
            server_config,
            Err(ConversionErrors::WrongVersion("v2".to_string()))
        );
    }

    #[test]
    fn utest_server_config_conflicting_certificates() {
        let server_config_content: &str = r"#
        version = 'v1'
        ca_pem = '''some_path_to_a_file/ca.pem'''
        crt_pem_content = '''the content of the
        crt.pem file is stored in here'''
        #";

        let server_config = ServerConfig::from_file(server_config_content);

        assert_eq!(
            server_config,
            Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string()
            ))
        );
    }

    #[test]
    fn utest_server_config_update_with_args() {
        let mut server_config = ServerConfig::default();
        let args = Arguments {
            path: Some("some_path_to_a_config_file/config_file.yaml".to_string()),
            config_file_path: None,
            addr: "127.0.0.1:3333".parse::<SocketAddr>().ok(),
            insecure: false,
            ca_pem: Some("some_path_to_ca_pem/ca.pem".to_string()),
            crt_pem: Some("some_path_to_crt_pem/crt.pem".to_string()),
            key_pem: Some("some_path_to_key_pem/key.pem".to_string()),
        };

        server_config.update_with_args(&args);

        assert_eq!(
            server_config.startup_config,
            Some("some_path_to_a_config_file/config_file.yaml".to_string())
        );
        assert_eq!(
            server_config.address,
            "127.0.0.1:3333".parse::<SocketAddr>().ok()
        );
        assert_eq!(server_config.insecure, Some(false));
        assert_eq!(
            server_config.ca_pem,
            Some("some_path_to_ca_pem/ca.pem".to_string())
        );
        assert_eq!(
            server_config.crt_pem,
            Some("some_path_to_crt_pem/crt.pem".to_string())
        );
        assert_eq!(
            server_config.key_pem,
            Some("some_path_to_key_pem/key.pem".to_string())
        );
    }

    #[test]
    fn utest_server_config_update_with_args_certificates_content() {
        let server_config_content: &str = r"#
        version = 'v1'
        ca_pem_content = '''the content of the
        ca.pem file is stored in here'''
        crt_pem_content = '''the content of the
        crt.pem file is stored in here'''
        key_pem_content = '''the content of the
        key.pem file is stored in here'''
        #";

        let mut server_config = ServerConfig::from_file(server_config_content).unwrap();
        let args = Arguments {
            path: Some("some_path_to_a_config_file/config_file.yaml".to_string()),
            config_file_path: None,
            addr: "127.0.0.1:3333".parse::<SocketAddr>().ok(),
            insecure: false,
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        server_config.update_with_args(&args);

        assert_eq!(
            server_config.ca_pem,
            Some(
                r"the content of the
        ca.pem file is stored in here"
                    .to_string()
            )
        );
        assert_eq!(
            server_config.crt_pem,
            Some(
                r"the content of the
        crt.pem file is stored in here"
                    .to_string()
            )
        );
        assert_eq!(
            server_config.key_pem,
            Some(
                r"the content of the
        key.pem file is stored in here"
                    .to_string()
            )
        );
    }

    #[test]
    fn utest_server_config_from_file_successful() {
        let server_config_content: &str = r"#
        version = 'v1'
        ca_pem_content = '''the content of the
        ca.pem file is stored in here'''
        crt_pem_content = '''the content of the
        crt.pem file is stored in here'''
        key_pem_content = '''the content of the
        key.pem file is stored in here'''
        #";

        let server_config = ServerConfig::from_file(server_config_content);

        assert!(server_config.is_ok())
    }
}

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
use common::std_extensions::{UnreachableOption, UnreachableResult};
use common::DEFAULT_SOCKET_ADDRESS;
use grpc::security::read_pem_file;
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use toml::from_str;

const CONFIG_VERSION: &str = "v1";
pub const DEFAULT_SERVER_CONFIG_FILE_PATH: &str = "/etc/ankaios/ank-server.conf";

#[cfg(not(test))]
use std::fs::read_to_string;

// This function is used in order to facilitate testing
#[cfg(test)]
fn read_to_string(file_path_content: &str) -> std::io::Result<String> {
    Ok(file_path_content.to_string())
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConversionErrors {
    WrongVersion(String),
    ConflictingCertificates(String),
    InvalidServerConfig(String),
    InvalidCertificate(String),
}

impl fmt::Display for ConversionErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionErrors::WrongVersion(msg) => {
                write!(f, "Wrong version: {}", msg)
            }
            ConversionErrors::ConflictingCertificates(msg) => {
                write!(f, "Conflicting certificates: {}", msg)
            }
            ConversionErrors::InvalidServerConfig(msg) => {
                write!(
                    f,
                    "Server Config could not have been parsed due to: {}",
                    msg
                )
            }
            ConversionErrors::InvalidCertificate(msg) => {
                write!(f, "Certificate could not have been read due to: {}", msg)
            }
        }
    }
}

pub fn get_default_address() -> SocketAddr {
    DEFAULT_SOCKET_ADDRESS.parse().unwrap_or_unreachable()
}

fn convert_to_socket_address<'de, D>(deserializer: D) -> Result<SocketAddr, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;

    s.parse::<SocketAddr>().map_err(serde::de::Error::custom)
}

// [impl->swdd~server-loads-config-file~1]
#[derive(Debug, Deserialize, PartialEq)]
pub struct ServerConfig {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_manifest: Option<String>,
    #[serde(deserialize_with = "convert_to_socket_address")]
    #[serde(default = "get_default_address")]
    pub address: SocketAddr,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub insecure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ca_pem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    crt_pem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key_pem: Option<String>,
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
            startup_manifest: None,
            address: get_default_address(),
            insecure: Some(false),
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
    pub fn from_file(file_path: PathBuf) -> Result<ServerConfig, ConversionErrors> {
        let server_config_content = read_to_string(file_path.to_str().unwrap_or_unreachable())
            .map_err(|err| ConversionErrors::InvalidServerConfig(err.to_string()))?;
        let mut server_config: ServerConfig = from_str(&server_config_content)
            .map_err(|err| ConversionErrors::InvalidServerConfig(err.to_string()))?;

        if server_config.version != CONFIG_VERSION {
            return Err(ConversionErrors::WrongVersion(server_config.version));
        }

        if (server_config.ca_pem.is_some() && server_config.ca_pem_content.is_some())
            || (server_config.crt_pem.is_some() && server_config.crt_pem_content.is_some())
            || (server_config.key_pem.is_some() && server_config.key_pem_content.is_some())
        {
            return Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string(),
            ));
        }

        if let Some(ca_pem_path) = &server_config.ca_pem {
            let ca_pem_content = read_pem_file(ca_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            server_config.ca_pem_content = Some(ca_pem_content);
        }
        if let Some(crt_pem_path) = &server_config.crt_pem {
            let crt_pem_content = read_pem_file(crt_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            server_config.crt_pem_content = Some(crt_pem_content);
        }
        if let Some(key_pem_path) = &server_config.key_pem {
            let key_pem_content = read_pem_file(key_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            server_config.key_pem_content = Some(key_pem_content);
        }

        Ok(server_config)
    }

    pub fn update_with_args(&mut self, args: &Arguments) {
        if let Some(path) = &args.manifest_path {
            self.startup_manifest = Some(path.to_string());
        }

        if let Some(addr) = &args.addr {
            self.address = *addr;
        }

        self.insecure = Some(args.insecure);

        if let Some(ca_pem_path) = &args.ca_pem {
            self.ca_pem = Some(ca_pem_path.to_owned());
            let ca_pem_content = read_pem_file(ca_pem_path, false).unwrap_or_default();
            self.ca_pem_content = Some(ca_pem_content);
        }
        if let Some(crt_pem_path) = &args.crt_pem {
            self.crt_pem = Some(crt_pem_path.to_owned());
            let crt_pem_content = read_pem_file(crt_pem_path, false).unwrap_or_default();
            self.crt_pem_content = Some(crt_pem_content);
        }
        if let Some(key_pem_path) = &args.key_pem {
            self.key_pem = Some(key_pem_path.to_owned());
            let key_pem_content = read_pem_file(key_pem_path, false).unwrap_or_default();
            self.key_pem_content = Some(key_pem_content);
        }
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
    use std::path::PathBuf;

    use common::DEFAULT_SOCKET_ADDRESS;

    use crate::{cli::Arguments, server_config::ConversionErrors};

    use super::ServerConfig;
    use super::DEFAULT_SERVER_CONFIG_FILE_PATH;

    const STARTUP_MANIFEST_PATH: &str = "some_path_to_config/config.yaml";
    const TEST_SOCKET_ADDRESS: &str = "127.0.0.1:3333";
    const CA_PEM_PATH: &str = "some_path_to_ca_pem/ca.pem";
    const CRT_PEM_PATH: &str = "some_path_to_crt_pem/crt.pem";
    const KEY_PEM_PATH: &str = "some_path_to_key_pem/key.pem";
    const CA_PEM_CONTENT: &str = r"the content of the
        ca.pem file is stored in here";
    const CRT_PEM_CONTENT: &str = r"the content of the
        crt.pem file is stored in here";
    const KEY_PEM_CONTENT: &str = r"the content of the
        key.pem file is stored in here";

    // [utest->swdd~server-loads-config-file~1]
    #[test]
    fn utest_default_server_config() {
        let default_server_config = ServerConfig::default();

        assert_eq!(
            default_server_config.address,
            DEFAULT_SOCKET_ADDRESS.parse::<SocketAddr>().unwrap()
        );
        assert_eq!(default_server_config.insecure, Some(false));
        assert_eq!(default_server_config.version, "v1");
    }

    // [utest->swdd~server-loads-config-file~1]
    #[test]
    fn utest_server_config_wrong_version() {
        let server_config_content: &str = r"#
        version = 'v2'
        #";

        let server_config = ServerConfig::from_file(PathBuf::from(server_config_content));

        assert_eq!(
            server_config,
            Err(ConversionErrors::WrongVersion("v2".to_string()))
        );
    }

    // [utest->swdd~server-loads-config-file~1]
    #[test]
    fn utest_server_config_conflicting_certificates() {
        let server_config_content = format!(
            r"#
        version = 'v1'
        ca_pem = '''{}'''
        ca_pem_content = '''{}'''
        #",
            CA_PEM_PATH, CRT_PEM_CONTENT
        );

        let server_config = ServerConfig::from_file(PathBuf::from(server_config_content.as_str()));

        assert_eq!(
            server_config,
            Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string()
            ))
        );
    }

    // [utest->swdd~server-loads-config-file~1]
    #[test]
    fn utest_server_config_update_with_args() {
        let mut server_config = ServerConfig::default();
        let args = Arguments {
            manifest_path: Some(STARTUP_MANIFEST_PATH.to_string()),
            config_path: Some(DEFAULT_SERVER_CONFIG_FILE_PATH.to_string()),
            addr: TEST_SOCKET_ADDRESS.parse::<SocketAddr>().ok(),
            insecure: false,
            ca_pem: Some(CA_PEM_PATH.to_string()),
            crt_pem: Some(CRT_PEM_PATH.to_string()),
            key_pem: Some(KEY_PEM_PATH.to_string()),
        };

        server_config.update_with_args(&args);

        assert_eq!(
            server_config.startup_manifest,
            Some(STARTUP_MANIFEST_PATH.to_string())
        );
        assert_eq!(
            server_config.address,
            TEST_SOCKET_ADDRESS.parse::<SocketAddr>().unwrap()
        );
        assert_eq!(server_config.insecure, Some(false));
        assert_eq!(server_config.ca_pem, Some(CA_PEM_PATH.to_string()));
        assert_eq!(server_config.crt_pem, Some(CRT_PEM_PATH.to_string()));
        assert_eq!(server_config.key_pem, Some(KEY_PEM_PATH.to_string()));
    }

    // [utest->swdd~server-loads-config-file~1]
    #[test]
    fn utest_server_config_update_with_args_certificates_content() {
        let server_config_content = format!(
            r"#
        version = 'v1'
        ca_pem_content = '''{}'''
        crt_pem_content = '''{}'''
        key_pem_content = '''{}'''
        #",
            CA_PEM_CONTENT, CRT_PEM_CONTENT, KEY_PEM_CONTENT
        );

        let mut server_config =
            ServerConfig::from_file(PathBuf::from(server_config_content.as_str())).unwrap();
        let args = Arguments {
            manifest_path: Some(STARTUP_MANIFEST_PATH.to_string()),
            config_path: Some(DEFAULT_SERVER_CONFIG_FILE_PATH.to_string()),
            addr: TEST_SOCKET_ADDRESS.parse::<SocketAddr>().ok(),
            insecure: false,
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        server_config.update_with_args(&args);

        assert_eq!(
            server_config.ca_pem_content,
            Some(CA_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.crt_pem_content,
            Some(CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.key_pem_content,
            Some(KEY_PEM_CONTENT.to_string())
        );
    }

    // [utest->swdd~server-loads-config-file~1]
    #[test]
    fn utest_server_config_from_file_successful() {
        let server_config_content = format!(
            r"#
        version = 'v1'
        startup_manifest = '/workspaces/ankaios/server/resources/startConfig.yaml'
        address = '127.0.0.1:25551'
        insecure = true
        ca_pem_content = '''{}'''
        crt_pem_content = '''{}'''
        key_pem_content = '''{}'''
        #",
            CA_PEM_CONTENT, CRT_PEM_CONTENT, KEY_PEM_CONTENT
        );

        let server_config_res = ServerConfig::from_file(PathBuf::from(server_config_content));

        assert!(server_config_res.is_ok());

        let server_config = server_config_res.unwrap();

        assert_eq!(
            server_config.address,
            "127.0.0.1:25551".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            server_config.ca_pem_content,
            Some(CA_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.crt_pem_content,
            Some(CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.key_pem_content,
            Some(KEY_PEM_CONTENT.to_string())
        );
        assert_eq!(server_config.insecure, Some(true));
        assert_eq!(
            server_config.startup_manifest,
            Some("/workspaces/ankaios/server/resources/startConfig.yaml".to_string())
        );
    }
}

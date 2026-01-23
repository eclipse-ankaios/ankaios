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
use common::config::{CONFIG_VERSION, ConfigFile, ConversionErrors};
use common::std_extensions::{UnreachableOption, UnreachableResult};
use grpc::security::read_pem_file;

use serde::{Deserialize, Deserializer};
use std::fs::read_to_string;
use std::net::SocketAddr;
use std::path::PathBuf;
use toml::from_str;

pub const DEFAULT_SERVER_CONFIG_FILE_PATH: &str = "/etc/ankaios/ank-server.conf";

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
    pub startup_manifest: Option<String>,
    #[serde(deserialize_with = "convert_to_socket_address")]
    #[serde(default = "get_default_address")]
    pub address: SocketAddr,
    #[serde(default)]
    pub insecure: Option<bool>,
    ca_pem: Option<String>,
    crt_pem: Option<String>,
    key_pem: Option<String>,
    pub ca_pem_content: Option<String>,
    pub crt_pem_content: Option<String>,
    pub key_pem_content: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            version: CONFIG_VERSION.to_string(),
            startup_manifest: None,
            address: get_default_address(),
            insecure: Some(bool::default()),
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
            ca_pem_content: None,
            crt_pem_content: None,
            key_pem_content: None,
        }
    }
}

impl ConfigFile for ServerConfig {
    fn from_file(file_path: PathBuf) -> Result<ServerConfig, ConversionErrors> {
        let server_config_content = read_to_string(file_path.to_str().unwrap_or_unreachable())
            .map_err(|err| ConversionErrors::InvalidConfig(err.to_string()))?;
        let mut server_config: ServerConfig = from_str(&server_config_content)
            .map_err(|err| ConversionErrors::InvalidConfig(err.to_string()))?;

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
}

impl ServerConfig {
    pub fn update_with_args(&mut self, args: &Arguments) {
        if let Some(path) = &args.manifest_path {
            self.startup_manifest = Some(path.to_string());
        }

        if let Some(addr) = &args.addr {
            self.address = *addr;
        }

        if let Some(insecure) = args.insecure {
            self.insecure = Some(insecure);
        }

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
    use super::DEFAULT_SERVER_CONFIG_FILE_PATH;
    use super::ServerConfig;
    use crate::cli::Arguments;

    use ankaios_api::test_utils::fixtures;
    use common::DEFAULT_SOCKET_ADDRESS;
    use common::config::{ConfigFile, ConversionErrors};

    use std::io::Write;
    use std::net::SocketAddr;
    use std::path::PathBuf;

    use tempfile::NamedTempFile;

    const STARTUP_MANIFEST_PATH: &str = "some_path_to_config/config.yaml";
    const TEST_SOCKET_ADDRESS: &str = "127.0.0.1:3333";

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

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{server_config_content}").unwrap();

        let server_config = ServerConfig::from_file(PathBuf::from(tmp_config_file.path()));

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
            fixtures::CA_PEM_PATH,
            fixtures::CRT_PEM_CONTENT,
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{server_config_content}").unwrap();

        let server_config = ServerConfig::from_file(PathBuf::from(tmp_config_file.path()));

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
            insecure: Some(false),
            ca_pem: Some(fixtures::CA_PEM_PATH.to_string()),
            crt_pem: Some(fixtures::CRT_PEM_PATH.to_string()),
            key_pem: Some(fixtures::KEY_PEM_PATH.to_string()),
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
        assert_eq!(
            server_config.ca_pem,
            Some(fixtures::CA_PEM_PATH.to_string())
        );
        assert_eq!(
            server_config.crt_pem,
            Some(fixtures::CRT_PEM_PATH.to_string())
        );
        assert_eq!(
            server_config.key_pem,
            Some(fixtures::KEY_PEM_PATH.to_string())
        );
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
            fixtures::CA_PEM_CONTENT,
            fixtures::CRT_PEM_CONTENT,
            fixtures::KEY_PEM_CONTENT,
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{server_config_content}").unwrap();

        let mut server_config =
            ServerConfig::from_file(PathBuf::from(tmp_config_file.path())).unwrap();
        let args = Arguments {
            manifest_path: Some(STARTUP_MANIFEST_PATH.to_string()),
            config_path: Some(DEFAULT_SERVER_CONFIG_FILE_PATH.to_string()),
            addr: TEST_SOCKET_ADDRESS.parse::<SocketAddr>().ok(),
            insecure: Some(false),
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        server_config.update_with_args(&args);

        assert_eq!(
            server_config.ca_pem_content,
            Some(fixtures::CA_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.crt_pem_content,
            Some(fixtures::CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.key_pem_content,
            Some(fixtures::KEY_PEM_CONTENT.to_string())
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
            fixtures::CA_PEM_CONTENT,
            fixtures::CRT_PEM_CONTENT,
            fixtures::KEY_PEM_CONTENT,
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{server_config_content}").unwrap();

        let server_config_res = ServerConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert!(server_config_res.is_ok());

        let server_config = server_config_res.unwrap();

        assert_eq!(
            server_config.address,
            "127.0.0.1:25551".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            server_config.ca_pem_content,
            Some(fixtures::CA_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.crt_pem_content,
            Some(fixtures::CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            server_config.key_pem_content,
            Some(fixtures::KEY_PEM_CONTENT.to_string())
        );
        assert_eq!(server_config.insecure, Some(true));
        assert_eq!(
            server_config.startup_manifest,
            Some("/workspaces/ankaios/server/resources/startConfig.yaml".to_string())
        );
    }
}

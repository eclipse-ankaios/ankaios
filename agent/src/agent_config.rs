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
use crate::io_utils::DEFAULT_RUN_FOLDER;
use common::std_extensions::UnreachableOption;
use common::DEFAULT_SERVER_ADDRESS;
use grpc::security::read_pem_file;

use serde::Deserialize;
use std::fmt;
use std::fs::read_to_string;
use std::path::PathBuf;
use toml::from_str;

const CONFIG_VERSION: &str = "v1";

#[cfg(not(test))]
pub const DEFAULT_AGENT_CONFIG_FILE_PATH: &str = "/etc/ankaios/ank-agent.conf";

#[cfg(test)]
pub const DEFAULT_AGENT_CONFIG_FILE_PATH: &str = "/tmp/ankaios/ank-agent.conf";

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConversionErrors {
    WrongVersion(String),
    ConflictingCertificates(String),
    InvalidAgentConfig(String),
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
            ConversionErrors::InvalidAgentConfig(msg) => {
                write!(f, "Agent Config could not have been parsed due to: {}", msg)
            }
            ConversionErrors::InvalidCertificate(msg) => {
                write!(f, "Certificate could not have been read due to: {}", msg)
            }
        }
    }
}

pub fn get_default_url() -> String {
    DEFAULT_SERVER_ADDRESS.to_string()
}

fn get_default_run_folder() -> Option<String> {
    Some(DEFAULT_RUN_FOLDER.to_string())
}

// [impl->swdd~agent-loads-config-file~1]
#[derive(Debug, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub version: String,
    pub name: Option<String>,
    #[serde(default = "get_default_url")]
    pub server_url: String,
    #[serde(default = "get_default_run_folder")]
    pub run_folder: Option<String>,
    #[serde(default)]
    pub insecure: Option<bool>,
    ca_pem: Option<String>,
    crt_pem: Option<String>,
    key_pem: Option<String>,
    pub ca_pem_content: Option<String>,
    pub crt_pem_content: Option<String>,
    pub key_pem_content: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            version: CONFIG_VERSION.to_string(),
            name: None,
            server_url: get_default_url(),
            run_folder: get_default_run_folder(),
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

impl AgentConfig {
    pub fn from_file(file_path: PathBuf) -> Result<AgentConfig, ConversionErrors> {
        let agent_config_content = read_to_string(file_path.to_str().unwrap_or_unreachable())
            .map_err(|err| ConversionErrors::InvalidAgentConfig(err.to_string()))?;
        let mut agent_config: AgentConfig = from_str(&agent_config_content)
            .map_err(|err| ConversionErrors::InvalidAgentConfig(err.to_string()))?;

        if agent_config.version != CONFIG_VERSION {
            return Err(ConversionErrors::WrongVersion(agent_config.version));
        }

        if (agent_config.ca_pem.is_some() && agent_config.ca_pem_content.is_some())
            || (agent_config.crt_pem.is_some() && agent_config.crt_pem_content.is_some())
            || (agent_config.key_pem.is_some() && agent_config.key_pem_content.is_some())
        {
            return Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string(),
            ));
        }

        if let Some(ca_pem_path) = &agent_config.ca_pem {
            let ca_pem_content = read_pem_file(ca_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            agent_config.ca_pem_content = Some(ca_pem_content);
        }
        if let Some(crt_pem_path) = &agent_config.crt_pem {
            let crt_pem_content = read_pem_file(crt_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            agent_config.crt_pem_content = Some(crt_pem_content);
        }
        if let Some(key_pem_path) = &agent_config.key_pem {
            let key_pem_content = read_pem_file(key_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            agent_config.key_pem_content = Some(key_pem_content);
        }

        Ok(agent_config)
    }

    pub fn update_with_args(&mut self, args: &Arguments) {
        if let Some(name) = &args.agent_name {
            self.name = Some(name.to_string());
        }

        if let Some(url) = &args.server_url {
            self.server_url = url.to_string();
        }

        if let Some(run_folder) = &args.run_folder {
            self.run_folder = Some(run_folder.to_string());
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
            let key_pem_content = read_pem_file(key_pem_path, true).unwrap_or_default();
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
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    use common::DEFAULT_SERVER_ADDRESS;

    use crate::io_utils::DEFAULT_RUN_FOLDER;
    use crate::{agent_config::ConversionErrors, cli::Arguments};

    use super::{AgentConfig, CONFIG_VERSION};

    const AGENT_NAME: &str = "agent_1";
    const CA_PEM_PATH: &str = "some_path_to_ca_pem/ca.pem";
    const CRT_PEM_PATH: &str = "some_path_to_crt_pem/crt.pem";
    const KEY_PEM_PATH: &str = "some_path_to_key_pem/key.pem";
    const CA_PEM_CONTENT: &str = r"the content of the
        ca.pem file is stored in here";
    const CRT_PEM_CONTENT: &str = r"the content of the
        crt.pem file is stored in here";
    const KEY_PEM_CONTENT: &str = r"the content of the
        key.pem file is stored in here";

    // [utest->swdd~agent-loads-config-file~1]
    #[test]
    fn utest_default_agent_config() {
        let default_agent_config = AgentConfig::default();

        assert_eq!(
            default_agent_config.server_url,
            DEFAULT_SERVER_ADDRESS.to_string()
        );
        assert_eq!(default_agent_config.insecure, Some(false));
        assert_eq!(default_agent_config.version, CONFIG_VERSION);
    }

    // [utest->swdd~agent-loads-config-file~1]
    #[test]
    fn utest_agent_config_wrong_version() {
        let agent_config_content: &str = r"#
        version = 'v2'
        #";

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", agent_config_content).unwrap();

        let agent_config = AgentConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert_eq!(
            agent_config,
            Err(ConversionErrors::WrongVersion("v2".to_string()))
        );
    }

    // [utest->swdd~agent-loads-config-file~1]
    #[test]
    fn utest_agent_config_conflicting_certificates() {
        let agent_config_content = format!(
            r"#
        version = 'v1'
        ca_pem = '''{}'''
        ca_pem_content = '''{}'''
        #",
            CA_PEM_PATH, CRT_PEM_CONTENT
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", agent_config_content).unwrap();

        let agent_config = AgentConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert_eq!(
            agent_config,
            Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string()
            ))
        );
    }

    // [utest->swdd~agent-loads-config-file~1]
    #[test]
    fn utest_agent_config_update_with_args() {
        let mut agent_config = AgentConfig::default();
        let args = Arguments {
            config_path: None,
            agent_name: Some(AGENT_NAME.to_string()),
            server_url: Some(DEFAULT_SERVER_ADDRESS.to_string()),
            run_folder: Some(DEFAULT_RUN_FOLDER.to_string()),
            insecure: false,
            ca_pem: Some(CA_PEM_PATH.to_string()),
            crt_pem: Some(CRT_PEM_PATH.to_string()),
            key_pem: Some(KEY_PEM_PATH.to_string()),
        };

        agent_config.update_with_args(&args);

        assert_eq!(agent_config.name, Some(AGENT_NAME.to_string()));
        assert_eq!(agent_config.server_url, DEFAULT_SERVER_ADDRESS.to_string());
        assert_eq!(
            agent_config.run_folder,
            Some(DEFAULT_RUN_FOLDER.to_string())
        );
        assert_eq!(agent_config.insecure, Some(false));
        assert_eq!(agent_config.ca_pem, Some(CA_PEM_PATH.to_string()));
        assert_eq!(agent_config.crt_pem, Some(CRT_PEM_PATH.to_string()));
        assert_eq!(agent_config.key_pem, Some(KEY_PEM_PATH.to_string()));
    }

    // [utest->swdd~agent-loads-config-file~1]
    #[test]
    fn utest_agent_config_update_with_args_certificates_content() {
        let agent_config_content = format!(
            r"#
        version = 'v1'
        ca_pem_content = '''{}'''
        crt_pem_content = '''{}'''
        key_pem_content = '''{}'''
        #",
            CA_PEM_CONTENT, CRT_PEM_CONTENT, KEY_PEM_CONTENT
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", agent_config_content).unwrap();

        let mut agent_config =
            AgentConfig::from_file(PathBuf::from(tmp_config_file.path())).unwrap();
        let args = Arguments {
            config_path: None,
            agent_name: Some(AGENT_NAME.to_string()),
            server_url: Some(DEFAULT_SERVER_ADDRESS.to_string()),
            run_folder: Some(DEFAULT_RUN_FOLDER.to_string()),
            insecure: false,
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        agent_config.update_with_args(&args);

        assert_eq!(
            agent_config.ca_pem_content,
            Some(CA_PEM_CONTENT.to_string())
        );
        assert_eq!(
            agent_config.crt_pem_content,
            Some(CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            agent_config.key_pem_content,
            Some(KEY_PEM_CONTENT.to_string())
        );
    }

    // [utest->swdd~agent-loads-config-file~1]
    #[test]
    fn utest_agent_config_from_file_successful() {
        let agent_config_content = format!(
            r"#
        version = 'v1'
        name = 'agent_1'
        server_url = 'http[s]://127.0.0.1:25551'
        run_folder = '/tmp/ankaios/'
        insecure = true
        ca_pem_content = '''{}'''
        crt_pem_content = '''{}'''
        key_pem_content = '''{}'''
        #",
            CA_PEM_CONTENT, CRT_PEM_CONTENT, KEY_PEM_CONTENT
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", agent_config_content).unwrap();

        let agent_config_res = AgentConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert!(agent_config_res.is_ok());

        let agent_config = agent_config_res.unwrap();

        assert_eq!(agent_config.name, Some(AGENT_NAME.to_string()));
        assert_eq!(agent_config.server_url, DEFAULT_SERVER_ADDRESS.to_string());
        assert_eq!(
            agent_config.run_folder,
            Some(DEFAULT_RUN_FOLDER.to_string())
        );
        assert_eq!(agent_config.insecure, Some(true));
        assert_eq!(
            agent_config.ca_pem_content,
            Some(CA_PEM_CONTENT.to_string())
        );
        assert_eq!(
            agent_config.crt_pem_content,
            Some(CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            agent_config.key_pem_content,
            Some(KEY_PEM_CONTENT.to_string())
        );
    }
}

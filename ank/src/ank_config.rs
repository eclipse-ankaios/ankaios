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

use crate::cli::AnkCli;
use common::std_extensions::UnreachableOption;
use common::DEFAULT_SERVER_ADDRESS;
use grpc::security::read_pem_file;

use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::fs::read_to_string;
use std::path::PathBuf;
use toml::from_str;

const SUPPORTED_CONFIG_VARIANTS: usize = 1;

pub const CONFIG_VERSION: &str = "v1";
pub const DEFAULT_CONFIG: &str = "default";
pub const DEFAULT_RESPONSE_TIMEOUT: u64 = 3000;

#[cfg(not(test))]
pub const DEFAULT_ANK_CONFIG_FILE_PATH: &str = "$HOME/.config/ankaios/ank-agent.conf";

#[cfg(test)]
pub const DEFAULT_ANK_CONFIG_FILE_PATH: &str = "/tmp/ankaios/ank-agent.conf";

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConversionErrors {
    WrongVersion(String),
    ConflictingCertificates(String),
    InvalidAnkConfig(String),
    InvalidCertificate(String),
    DefaultContextNotFound(),
    UnsupportedContexts(),
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
            ConversionErrors::InvalidAnkConfig(msg) => {
                write!(f, "Ank Config could not have been parsed due to: {}", msg)
            }
            ConversionErrors::InvalidCertificate(msg) => {
                write!(f, "Certificate could not have been read due to: {}", msg)
            }
            ConversionErrors::DefaultContextNotFound() => {
                write!(f, "Default not found inside config file")
            }
            ConversionErrors::UnsupportedContexts() => {
                write!(f, "Contexts are not currently supported")
            }
        }
    }
}

fn get_default_response_timeout() -> u64 {
    DEFAULT_RESPONSE_TIMEOUT
}

fn get_default_url() -> String {
    DEFAULT_SERVER_ADDRESS.to_string()
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct AnkConfig {
    pub version: String,
    #[serde(default = "get_default_response_timeout")]
    pub response_timeout: u64,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub quiet: bool,
    #[serde(default)]
    pub no_wait: bool,
    #[serde(flatten)]
    pub config_variant: HashMap<String, ConfigVariant>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ConfigVariant {
    #[serde(default = "get_default_url")]
    pub server_url: String,
    #[serde(default)]
    pub insecure: bool,
    ca_pem: Option<String>,
    crt_pem: Option<String>,
    key_pem: Option<String>,
    pub ca_pem_content: Option<String>,
    pub crt_pem_content: Option<String>,
    pub key_pem_content: Option<String>,
}

impl Default for AnkConfig {
    fn default() -> AnkConfig {
        AnkConfig {
            version: CONFIG_VERSION.to_string(),
            response_timeout: get_default_response_timeout(),
            verbose: bool::default(),
            quiet: bool::default(),
            no_wait: bool::default(),
            config_variant: HashMap::new(),
        }
    }
}

impl Default for ConfigVariant {
    fn default() -> Self {
        ConfigVariant {
            server_url: get_default_url(),
            insecure: bool::default(),
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
            ca_pem_content: None,
            crt_pem_content: None,
            key_pem_content: None,
        }
    }
}

impl AnkConfig {
    pub fn from_file(file_path: PathBuf) -> Result<AnkConfig, ConversionErrors> {
        let ank_config_content = read_to_string(file_path.to_str().unwrap_or_unreachable())
            .map_err(|err| ConversionErrors::InvalidAnkConfig(err.to_string()))?;
        let mut ank_config: AnkConfig = from_str(&ank_config_content)
            .map_err(|err| ConversionErrors::InvalidAnkConfig(err.to_string()))?;

        if ank_config.version != CONFIG_VERSION {
            return Err(ConversionErrors::WrongVersion(ank_config.version));
        }

        if ank_config.config_variant.len() > SUPPORTED_CONFIG_VARIANTS {
            return Err(ConversionErrors::UnsupportedContexts());
        }

        let default_context = ank_config
            .config_variant
            .get_mut(DEFAULT_CONFIG)
            .ok_or(ConversionErrors::DefaultContextNotFound())?;

        if Self::has_conflicting_certificates(default_context) {
            return Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string(),
            ));
        }

        Self::read_pem_files(default_context)?;

        Ok(ank_config)
    }

    fn has_conflicting_certificates(config: &ConfigVariant) -> bool {
        config.ca_pem.is_some() && config.ca_pem_content.is_some()
            || config.crt_pem.is_some() && config.crt_pem_content.is_some()
            || config.key_pem.is_some() && config.key_pem_content.is_some()
    }

    fn read_pem_files(config: &mut ConfigVariant) -> Result<(), ConversionErrors> {
        if let Some(ca_pem_path) = &config.ca_pem {
            config.ca_pem_content = Some(
                read_pem_file(ca_pem_path, false)
                    .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?,
            );
        }
        if let Some(crt_pem_path) = &config.crt_pem {
            config.crt_pem_content = Some(
                read_pem_file(crt_pem_path, false)
                    .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?,
            );
        }
        if let Some(key_pem_path) = &config.key_pem {
            config.key_pem_content = Some(
                read_pem_file(key_pem_path, false)
                    .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?,
            );
        }
        Ok(())
    }

    pub fn update_with_args(&mut self, args: &AnkCli) {
        if let Some(response_timeout) = args.response_timeout_ms {
            self.response_timeout = response_timeout;
        }

        self.verbose = args.verbose;
        self.quiet = args.quiet;
        self.no_wait = args.no_wait;

        if let Some(default_config) = self.config_variant.get_mut(DEFAULT_CONFIG) {
            default_config.insecure = args.insecure;

            if let Some(ca_pem_path) = &args.ca_pem {
                default_config.ca_pem = Some(ca_pem_path.to_owned());
                let ca_pem_content = read_pem_file(ca_pem_path, false).unwrap_or_default();
                default_config.ca_pem_content = Some(ca_pem_content);
            }
            if let Some(crt_pem_path) = &args.crt_pem {
                default_config.crt_pem = Some(crt_pem_path.to_owned());
                let crt_pem_content = read_pem_file(crt_pem_path, false).unwrap_or_default();
                default_config.crt_pem_content = Some(crt_pem_content);
            }
            if let Some(key_pem_path) = &args.key_pem {
                default_config.key_pem = Some(key_pem_path.to_owned());
                let key_pem_content = read_pem_file(key_pem_path, false).unwrap_or_default();
                default_config.key_pem_content = Some(key_pem_content);
            }
        }
    }
}

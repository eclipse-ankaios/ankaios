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
use serde::de::{Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::read_to_string;
use std::path::PathBuf;
use toml::{from_str, Value};

#[cfg(not(test))]
use common::std_extensions::GracefulExitResult;
#[cfg(not(test))]
use once_cell::sync::Lazy;
#[cfg(not(test))]
use std::env;

pub const CONFIG_VERSION: &str = "v1";
pub const DEFAULT_CONFIG: &str = "default";
pub const DEFAULT_RESPONSE_TIMEOUT: u64 = 3000;

#[cfg(not(test))]
pub static DEFAULT_ANK_CONFIG_FILE_PATH: Lazy<String> = Lazy::new(|| {
    let home_dir = env::var("HOME").unwrap_or_exit("HOME environment variable not set");
    format!("{}/.config/ankaios/ank.conf", home_dir)
});

#[cfg(test)]
pub const DEFAULT_ANK_CONFIG_FILE_PATH: &str = "/tmp/ankaios/ank.conf";

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConversionErrors {
    WrongVersion(String),
    ConflictingCertificates(String),
    InvalidAnkConfig(String),
    InvalidCertificate(String),
}

impl fmt::Display for ConversionErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionErrors::WrongVersion(msg) => write!(f, "Wrong version: {}", msg),
            ConversionErrors::ConflictingCertificates(msg) => {
                write!(f, "Conflicting certificates: {}", msg)
            }
            ConversionErrors::InvalidAnkConfig(msg) => {
                write!(f, "Ank Config could not have been parsed due to: {}", msg)
            }
            ConversionErrors::InvalidCertificate(msg) => {
                write!(f, "Certificate could not have been read due to: {}", msg)
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

// [impl->swdd~cli-loads-config-file~1]
#[derive(Debug, PartialEq)]
pub struct AnkConfig {
    pub version: String,
    pub response_timeout: u64,
    pub verbose: bool,
    pub quiet: bool,
    pub no_wait: bool,
    pub server_url: String,
    pub insecure: bool,
    ca_pem: Option<String>,
    crt_pem: Option<String>,
    key_pem: Option<String>,
    pub ca_pem_content: Option<String>,
    pub crt_pem_content: Option<String>,
    pub key_pem_content: Option<String>,
}

#[derive(Deserialize, Debug)]
struct AnkConfigHelper {
    version: String,
    #[serde(default = "get_default_response_timeout")]
    response_timeout: u64,
    #[serde(default)]
    verbose: bool,
    #[serde(default)]
    quiet: bool,
    #[serde(default)]
    no_wait: bool,
    #[serde(default = "get_default_url")]
    server_url: String,
    #[serde(default)]
    insecure: bool,
    ca_pem: Option<String>,
    crt_pem: Option<String>,
    key_pem: Option<String>,
    ca_pem_content: Option<String>,
    crt_pem_content: Option<String>,
    key_pem_content: Option<String>,
}

impl From<AnkConfigHelper> for AnkConfig {
    fn from(helper: AnkConfigHelper) -> Self {
        AnkConfig {
            version: helper.version,
            response_timeout: helper.response_timeout,
            verbose: helper.verbose,
            quiet: helper.quiet,
            no_wait: helper.no_wait,
            server_url: helper.server_url,
            insecure: helper.insecure,
            ca_pem: helper.ca_pem,
            crt_pem: helper.crt_pem,
            key_pem: helper.key_pem,
            ca_pem_content: helper.ca_pem_content,
            crt_pem_content: helper.crt_pem_content,
            key_pem_content: helper.key_pem_content,
        }
    }
}

struct AnkConfigVisitor {
    table_key: &'static str,
}

impl<'de> Visitor<'de> for AnkConfigVisitor {
    type Value = AnkConfig;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a TOML map for AnkConfig with some table fields")
    }

    fn visit_map<V>(self, mut map: V) -> Result<AnkConfig, V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut merged: BTreeMap<String, Value> = BTreeMap::new();

        while let Some(key) = map.next_key::<String>()? {
            let value: Value = map.next_value()?;
            match key.as_str() {
                k if k == self.table_key => {
                    if let Value::Table(inner) = value {
                        merged.extend(inner.into_iter());
                    } else {
                        return Err(V::Error::custom(format!(
                            "Expected '{}' to be a table",
                            self.table_key
                        )));
                    }
                }
                _ => {
                    merged.insert(key, value);
                }
            }
        }

        let deserializer = serde::de::value::MapDeserializer::new(merged.into_iter());
        let helper = AnkConfigHelper::deserialize(deserializer).map_err(V::Error::custom)?;
        Ok(helper.into())
    }
}

impl<'de> Deserialize<'de> for AnkConfig {
    fn deserialize<D>(deserializer: D) -> Result<AnkConfig, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(AnkConfigVisitor {
            table_key: DEFAULT_CONFIG,
        })
    }
}

impl Default for AnkConfig {
    fn default() -> Self {
        AnkConfig {
            version: CONFIG_VERSION.to_string(),
            response_timeout: get_default_response_timeout(),
            verbose: bool::default(),
            quiet: bool::default(),
            no_wait: bool::default(),
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
    // [impl->swdd~cli-loads-config-file~1]
    pub fn from_file(file_path: PathBuf) -> Result<AnkConfig, ConversionErrors> {
        let ank_config_content = read_to_string(file_path.to_str().unwrap_or_unreachable())
            .map_err(|err| ConversionErrors::InvalidAnkConfig(err.to_string()))?;
        let mut ank_config: AnkConfig = from_str(&ank_config_content)
            .map_err(|err| ConversionErrors::InvalidAnkConfig(err.to_string()))?;

        if ank_config.version != CONFIG_VERSION {
            return Err(ConversionErrors::WrongVersion(ank_config.version));
        }

        if (ank_config.ca_pem.is_some() && ank_config.ca_pem_content.is_some())
            || (ank_config.crt_pem.is_some() && ank_config.crt_pem_content.is_some())
            || (ank_config.key_pem.is_some() && ank_config.key_pem_content.is_some())
        {
            return Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string(),
            ));
        }

        if let Some(ca_pem_path) = &ank_config.ca_pem {
            let ca_pem_content = read_pem_file(ca_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            ank_config.ca_pem_content = Some(ca_pem_content);
        }
        if let Some(crt_pem_path) = &ank_config.crt_pem {
            let crt_pem_content = read_pem_file(crt_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            ank_config.crt_pem_content = Some(crt_pem_content);
        }
        if let Some(key_pem_path) = &ank_config.key_pem {
            let key_pem_content = read_pem_file(key_pem_path, false)
                .map_err(|err| ConversionErrors::InvalidCertificate(err.to_string()))?;
            ank_config.key_pem_content = Some(key_pem_content);
        }

        Ok(ank_config)
    }

    pub fn update_with_args(&mut self, args: &AnkCli) {
        if let Some(response_timeout) = args.response_timeout_ms {
            self.response_timeout = response_timeout;
        }

        if let Some(verbose) = args.verbose {
            self.verbose = verbose;
        }
        if let Some(quiet) = args.quiet {
            self.quiet = quiet;
        }
        if let Some(no_wait) = args.no_wait {
            self.no_wait = no_wait;
        }
        if let Some(insecure) = args.insecure {
            self.insecure = insecure;
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

    use crate::{
        ank_config::{get_default_response_timeout, get_default_url, ConversionErrors},
        cli::{AnkCli, Commands, GetArgs, GetCommands},
    };

    use super::{AnkConfig, DEFAULT_ANK_CONFIG_FILE_PATH};

    const CA_PEM_PATH: &str = "some_path_to_ca_pem/ca.pem";
    const CRT_PEM_PATH: &str = "some_path_to_crt_pem/crt.pem";
    const KEY_PEM_PATH: &str = "some_path_to_key_pem/key.pem";
    const CA_PEM_CONTENT: &str = r"the content of the
        ca.pem file is stored in here";
    const CRT_PEM_CONTENT: &str = r"the content of the
        crt.pem file is stored in here";
    const KEY_PEM_CONTENT: &str = r"the content of the
        key.pem file is stored in here";

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_default_ank_config() {
        let default_ank_config = AnkConfig::default();

        assert_eq!(
            default_ank_config.response_timeout,
            get_default_response_timeout()
        );
        assert!(!default_ank_config.verbose);
        assert!(!default_ank_config.quiet);
        assert!(!default_ank_config.no_wait);
        assert!(!default_ank_config.insecure);
        assert!(default_ank_config.ca_pem.is_none());
        assert!(default_ank_config.crt_pem.is_none());
        assert!(default_ank_config.key_pem.is_none());
        assert!(default_ank_config.ca_pem_content.is_none());
        assert!(default_ank_config.crt_pem_content.is_none());
        assert!(default_ank_config.key_pem_content.is_none());
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_wrong_version() {
        let ank_config_content: &str = r"#
        version = 'v2'
        #";

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", ank_config_content).unwrap();

        let ank_config = AnkConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert_eq!(
            ank_config,
            Err(ConversionErrors::WrongVersion("v2".to_string()))
        );
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_conflicting_certificates() {
        let ank_config_content = format!(
            r"#
        version = 'v1'
        [default]
        ca_pem = '''{}'''
        ca_pem_content = '''{}'''
        #",
            CA_PEM_PATH, CRT_PEM_CONTENT
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", ank_config_content).unwrap();

        let ank_config = AnkConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert_eq!(
            ank_config,
            Err(ConversionErrors::ConflictingCertificates(
                "Certificate paths and certificate content are both set".to_string()
            ))
        );
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_update_with_args() {
        let mut ank_config = AnkConfig::default();
        let args = AnkCli {
            command: Commands::Get(GetArgs {
                command: Some(GetCommands::State {
                    output_format: crate::cli::OutputFormat::Yaml,
                    object_field_mask: Vec::new(),
                }),
            }),
            server_url: Some(DEFAULT_SERVER_ADDRESS.to_string()),
            config_path: Some(DEFAULT_ANK_CONFIG_FILE_PATH.to_string()),
            response_timeout_ms: Some(5000),
            insecure: Some(false),
            verbose: Some(true),
            quiet: Some(true),
            no_wait: Some(true),
            ca_pem: Some(CA_PEM_PATH.to_string()),
            crt_pem: Some(CRT_PEM_PATH.to_string()),
            key_pem: Some(KEY_PEM_PATH.to_string()),
        };

        ank_config.update_with_args(&args);

        assert_eq!(ank_config.response_timeout, 5000);
        assert!(ank_config.verbose);
        assert!(ank_config.quiet);
        assert!(ank_config.no_wait);
        assert!(!ank_config.insecure);
        assert_eq!(ank_config.ca_pem, Some(CA_PEM_PATH.to_string()));
        assert_eq!(ank_config.crt_pem, Some(CRT_PEM_PATH.to_string()));
        assert_eq!(ank_config.key_pem, Some(KEY_PEM_PATH.to_string()));
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_update_with_args_certificates_content() {
        let ank_config_content = format!(
            r"#
        version = 'v1'
        [default]
        ca_pem_content = '''{}'''
        crt_pem_content = '''{}'''
        key_pem_content = '''{}'''
        #",
            CA_PEM_CONTENT, CRT_PEM_CONTENT, KEY_PEM_CONTENT
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", ank_config_content).unwrap();

        let mut ank_config = AnkConfig::from_file(PathBuf::from(tmp_config_file.path())).unwrap();
        let args = AnkCli {
            command: Commands::Get(GetArgs {
                command: Some(GetCommands::State {
                    output_format: crate::cli::OutputFormat::Yaml,
                    object_field_mask: Vec::new(),
                }),
            }),
            server_url: Some(DEFAULT_SERVER_ADDRESS.to_string()),
            config_path: Some(DEFAULT_ANK_CONFIG_FILE_PATH.to_string()),
            response_timeout_ms: Some(5000),
            insecure: Some(false),
            verbose: Some(true),
            quiet: Some(true),
            no_wait: Some(true),
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        ank_config.update_with_args(&args);

        assert_eq!(ank_config.ca_pem_content, Some(CA_PEM_CONTENT.to_string()));
        assert_eq!(
            ank_config.crt_pem_content,
            Some(CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            ank_config.key_pem_content,
            Some(KEY_PEM_CONTENT.to_string())
        );
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_update_with_args_flags_unset() {
        let ank_config_content = format!(
            r"#
        version = 'v1'
        [default]
        ca_pem_content = '''{}'''
        crt_pem_content = '''{}'''
        key_pem_content = '''{}'''
        #",
            CA_PEM_CONTENT, CRT_PEM_CONTENT, KEY_PEM_CONTENT
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", ank_config_content).unwrap();

        let mut ank_config = AnkConfig::from_file(PathBuf::from(tmp_config_file.path())).unwrap();
        let args = AnkCli {
            command: Commands::Get(GetArgs {
                command: Some(GetCommands::State {
                    output_format: crate::cli::OutputFormat::Yaml,
                    object_field_mask: Vec::new(),
                }),
            }),
            server_url: Some(DEFAULT_SERVER_ADDRESS.to_string()),
            config_path: Some(DEFAULT_ANK_CONFIG_FILE_PATH.to_string()),
            response_timeout_ms: Some(5000),
            insecure: None,
            verbose: None,
            quiet: None,
            no_wait: None,
            ca_pem: None,
            crt_pem: None,
            key_pem: None,
        };

        ank_config.update_with_args(&args);

        assert!(!ank_config.verbose);
        assert!(!ank_config.quiet);
        assert!(!ank_config.no_wait);
        assert!(!ank_config.insecure);
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_no_context_returns_default() {
        let ank_config_content = r"#
        version = 'v1'
        #";
        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", ank_config_content).unwrap();

        let ank_config = AnkConfig::from_file(PathBuf::from(tmp_config_file.path())).unwrap();

        assert_eq!(ank_config.server_url, get_default_url());
        assert!(!ank_config.insecure);
        assert!(ank_config.ca_pem.is_none());
        assert!(ank_config.crt_pem.is_none());
        assert!(ank_config.key_pem.is_none());
        assert!(ank_config.ca_pem_content.is_none());
        assert!(ank_config.crt_pem_content.is_none());
        assert!(ank_config.key_pem_content.is_none());
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_multiple_contexts_found() {
        let ank_config_content = r"#
        version = 'v1'
        [default]
        [context]
        #";
        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", ank_config_content).unwrap();

        let ank_config = AnkConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert!(ank_config.is_ok());
    }

    // [utest->swdd~cli-loads-config-file~1]
    #[test]
    fn utest_ank_config_from_file_successful() {
        let ank_config_content = format!(
            r"#
        version = 'v1'
        response_timeout = 3000
        verbose = false
        quiet = false
        no_wait = false
        [default]
        server_url = 'https://127.0.0.1:25551'
        insecure = false
        ca_pem_content = '''{}'''
        crt_pem_content = '''{}'''
        key_pem_content = '''{}'''
        #",
            CA_PEM_CONTENT, CRT_PEM_CONTENT, KEY_PEM_CONTENT
        );

        let mut tmp_config_file = NamedTempFile::new().unwrap();
        write!(tmp_config_file, "{}", ank_config_content).unwrap();

        let ank_config_res = AnkConfig::from_file(PathBuf::from(tmp_config_file.path()));

        assert!(ank_config_res.is_ok());

        let ank_config = ank_config_res.unwrap();

        assert_eq!(ank_config.response_timeout, 3000);
        assert!(!ank_config.verbose);
        assert!(!ank_config.quiet);
        assert!(!ank_config.no_wait);
        assert_eq!(ank_config.server_url, DEFAULT_SERVER_ADDRESS.to_string());
        assert!(!ank_config.insecure);
        assert_eq!(ank_config.ca_pem_content, Some(CA_PEM_CONTENT.to_string()));
        assert_eq!(
            ank_config.crt_pem_content,
            Some(CRT_PEM_CONTENT.to_string())
        );
        assert_eq!(
            ank_config.key_pem_content,
            Some(KEY_PEM_CONTENT.to_string())
        );
    }
}

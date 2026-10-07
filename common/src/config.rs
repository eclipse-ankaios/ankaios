// Copyright (c) 2026 Elektrobit Automotive GmbH
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

use serde::Deserialize;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use crate::DEFAULT_SERVER_ADDRESS;

// [impl->swdd~common-config-handling~1]

pub const CONFIG_VERSION: &str = "v1";

pub const UNIX_SOCKET_SCHEME: &str = "unix://";
pub const HTTP_SCHEME: &str = "http://";
pub const HTTPS_SCHEME: &str = "https://";

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConversionErrors {
    WrongVersion(String),
    ConflictingCertificates(String),
    InvalidConfig(String),
    InvalidCertificate(String),
}

pub trait ConfigFile: Default + Sized {
    fn from_file(file_path: PathBuf) -> Result<Self, ConversionErrors>;
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub enum ServerUrl {
    Tcp(String),
    Unix(PathBuf),
}

impl ServerUrl {
    pub fn is_unix(&self) -> bool {
        matches!(self, ServerUrl::Unix(_))
    }

    pub fn validate_tls_settings(
        &self,
        config_name: &str,
        insecure: bool,
        has_tls_certificate_settings: bool,
    ) -> Result<(), String> {
        if !self.is_unix() {
            return Ok(());
        }

        if insecure {
            return Err(format!(
                "Invalid {config_name} config: 'insecure' must not be enabled for unix:// endpoints"
            ));
        }

        if has_tls_certificate_settings {
            return Err(format!(
                "Invalid {config_name} config: TLS certificate settings are not allowed for unix:// endpoints"
            ));
        }

        Ok(())
    }
}

pub fn parse_unix_socket_path(raw_path: &str) -> Result<PathBuf, String> {
    if raw_path.is_empty() {
        return Err("Wrong server url format: empty unix socket path.".to_string());
    }
    if !Path::new(raw_path).is_absolute() {
        return Err(format!(
            "Wrong server url format: unix socket path must be absolute: '{raw_path}'."
        ));
    }
    Ok(PathBuf::from(raw_path))
}

impl FromStr for ServerUrl {
    type Err = String;

    fn from_str(url: &str) -> Result<Self, Self::Err> {
        if let Some(path) = url.strip_prefix(UNIX_SOCKET_SCHEME) {
            return parse_unix_socket_path(path).map(ServerUrl::Unix);
        }

        if url.starts_with(HTTPS_SCHEME) || url.starts_with(HTTP_SCHEME) {
            Ok(ServerUrl::Tcp(url.to_string()))
        } else {
            Err(format!("Wrong server url format: '{url}'."))
        }
    }
}

impl TryFrom<String> for ServerUrl {
    type Error = String;

    fn try_from(url: String) -> Result<Self, Self::Error> {
        url.parse()
    }
}

impl fmt::Display for ServerUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerUrl::Tcp(url) => write!(f, "{url}"),
            ServerUrl::Unix(path) => write!(f, "{UNIX_SOCKET_SCHEME}{}", path.display()),
        }
    }
}

pub fn get_default_url() -> ServerUrl {
    ServerUrl::Tcp(DEFAULT_SERVER_ADDRESS.to_string())
}

impl fmt::Display for ConversionErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversionErrors::WrongVersion(msg) => write!(f, "Wrong version: {msg}"),
            ConversionErrors::ConflictingCertificates(msg) => {
                write!(f, "Conflicting certificates: {msg}")
            }
            ConversionErrors::InvalidConfig(msg) => {
                write!(f, "Config file could not have been parsed due to: {msg}")
            }
            ConversionErrors::InvalidCertificate(msg) => {
                write!(f, "Certificate could not have been read due to: {msg}")
            }
        }
    }
}

/// Handles the configuration file loading.
///
/// This function attempts to load the configuration from the provided `config_path`.
/// If no path is provided, it will iterate over the `default_paths` and load the first
/// existing configuration file. If no configuration file is found, it will return the
/// default configuration.
///
/// Loading errors are returned to the caller instead of being handled here, so that each
/// component can report them through its own tracing/logging mechanism.
///
/// ## Returns
///
/// The loaded configuration or the error that occurred while loading it.
pub fn handle_config<T: ConfigFile>(
    config_path: &Option<String>,
    default_paths: &[&str],
) -> Result<T, ConversionErrors> {
    match config_path {
        Some(config_path) => {
            let config_path = PathBuf::from(config_path);
            log::info!(
                "Loading config from user provided path '{}'",
                config_path.display()
            );
            T::from_file(config_path)
        }
        None => {
            for path in default_paths {
                let default_path = PathBuf::from(path);
                if default_path.try_exists().unwrap_or(false) {
                    log::info!(
                        "Loading config from default path '{}'",
                        default_path.display()
                    );
                    return T::from_file(default_path);
                }
            }

            log::debug!(
                "No config file found at default paths '{:?}'. Continue with default config.",
                default_paths,
            );
            Ok(T::default())
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

// [utest->swdd~common-config-handling~1]
#[cfg(test)]
mod tests {
    use super::{
        ConfigFile, ConversionErrors, ServerUrl, UNIX_SOCKET_SCHEME,
        get_default_url, handle_config,
    };
    use crate::DEFAULT_SERVER_ADDRESS;
    use crate::std_extensions::UnreachableOption;
    use serde::Deserialize;
    use std::fs::read_to_string;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;
    use toml::from_str;

    #[test]
    fn utest_conversion_errors_display() {
        let test_cases = vec![
            (
                ConversionErrors::WrongVersion("v0.0".to_string()),
                "Wrong version: v0.0",
            ),
            (
                ConversionErrors::ConflictingCertificates("both set".to_string()),
                "Conflicting certificates: both set",
            ),
            (
                ConversionErrors::InvalidConfig("parse error".to_string()),
                "Config file could not have been parsed due to: parse error",
            ),
            (
                ConversionErrors::InvalidCertificate("reason".to_string()),
                "Certificate could not have been read due to: reason",
            ),
        ];

        for (error, expected) in test_cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn utest_get_default_url() {
        assert_eq!(
            get_default_url(),
            ServerUrl::Tcp(DEFAULT_SERVER_ADDRESS.to_string())
        );
    }

    #[test]
    fn utest_server_url_parses_http_https_and_unix_absolute() {
        assert_eq!(
            "http://127.0.0.1:25551".parse::<ServerUrl>(),
            Ok(ServerUrl::Tcp("http://127.0.0.1:25551".to_string()))
        );
        assert_eq!(
            "https://127.0.0.1:25551".parse::<ServerUrl>(),
            Ok(ServerUrl::Tcp("https://127.0.0.1:25551".to_string()))
        );
        assert_eq!(
            "unix:///tmp/ank.sock".parse::<ServerUrl>(),
            Ok(ServerUrl::Unix(PathBuf::from("/tmp/ank.sock")))
        );
    }

    #[test]
    fn utest_server_url_rejects_invalid_url() {
        assert!("127.0.0.1:25551".parse::<ServerUrl>().is_err());
    }

    #[test]
    fn utest_server_url_rejects_empty_unix_path() {
        assert_eq!(
            UNIX_SOCKET_SCHEME.parse::<ServerUrl>(),
            Err("Wrong server url format: empty unix socket path.".to_string())
        );
    }

    #[test]
    fn utest_server_url_rejects_relative_unix_path() {
        assert!("unix://tmp/ank.sock".parse::<ServerUrl>().is_err());
    }

    #[test]
    fn utest_server_url_validate_tls_settings_allows_tcp_with_tls_or_insecure() {
        let url = ServerUrl::Tcp("https://127.0.0.1:25551".to_string());
        assert!(url.validate_tls_settings("ank", true, true).is_ok());
    }

    #[test]
    fn utest_server_url_validate_tls_settings_rejects_unix_with_insecure() {
        let url = ServerUrl::Unix(PathBuf::from("/tmp/ank.sock"));
        assert!(url.validate_tls_settings("ank", true, false).is_err());
    }

    #[test]
    fn utest_server_url_validate_tls_settings_rejects_unix_with_tls() {
        let url = ServerUrl::Unix(PathBuf::from("/tmp/ank.sock"));
        assert!(url.validate_tls_settings("ank", false, true).is_err());
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestConfig {
        test_string: String,
        test_bool: bool,
    }

    impl Default for TestConfig {
        fn default() -> Self {
            TestConfig {
                test_string: "default_value".to_string(),
                test_bool: false,
            }
        }
    }

    impl ConfigFile for TestConfig {
        fn from_file(file_path: PathBuf) -> Result<TestConfig, ConversionErrors> {
            let config_content = read_to_string(file_path.to_str().unwrap_or_unreachable())
                .map_err(|err| ConversionErrors::InvalidConfig(err.to_string()))?;
            let config: TestConfig = from_str(&config_content)
                .map_err(|err| ConversionErrors::InvalidConfig(err.to_string()))?;
            Ok(config)
        }
    }

    const VALID_TEST_CONFIG_CONTENT: &str = r#"
    test_string = 'test_value'
    test_bool = true
    "#;

    #[test]
    fn utest_handle_config_valid_config() {
        let mut tmp_config = NamedTempFile::new().expect("Failed to create the temp file");
        write!(tmp_config, "{VALID_TEST_CONFIG_CONTENT}")
            .expect("Failed to write to the temp file");

        let test_config: TestConfig = handle_config(
            &Some(tmp_config.into_temp_path().to_str().unwrap().to_string()),
            &["/a/very/invalid/path/to/config/file"],
        )
        .unwrap();

        assert_eq!(test_config.test_string, "test_value");
        assert!(test_config.test_bool);
    }

    #[test]
    fn utest_handle_config_default_paths() {
        let mut file = NamedTempFile::new().expect("Failed to create file");
        writeln!(file, "{VALID_TEST_CONFIG_CONTENT}").expect("Failed to write to file");

        let test_config: TestConfig =
            handle_config(&None, &[file.path().to_str().unwrap()]).unwrap();

        assert_eq!(test_config.test_string, "test_value");
        assert!(test_config.test_bool);
    }

    #[test]
    fn utest_handle_config_default_paths_first_path_taking_precedence_over_the_other() {
        // Config file 1
        let mut default_file_1 = NamedTempFile::new().expect("Failed to create file");
        writeln!(default_file_1, "{VALID_TEST_CONFIG_CONTENT}").expect("Failed to write to file");

        // Config file 2 with different content
        const CHANGED_TEST_VALUE: &str = "different_test_value";
        let mut default_file_2 = NamedTempFile::new().expect("Failed to create file");
        let other_config_content =
            VALID_TEST_CONFIG_CONTENT.replace("test_value", CHANGED_TEST_VALUE);
        writeln!(default_file_2, "{other_config_content}").expect("Failed to write to file");

        let file_path_1 = default_file_1.path().to_str().unwrap().to_owned();
        let file_path_2 = default_file_2.path().to_str().unwrap().to_owned();

        let test_config: TestConfig = handle_config(&None, &[&file_path_1, &file_path_2]).unwrap();

        assert_eq!(test_config.test_string, "test_value");
        assert!(test_config.test_bool);

        // config file 1 is deleted, so config file 2 should be loaded
        drop(default_file_1);

        let test_config: TestConfig = handle_config(&None, &[&file_path_1, &file_path_2]).unwrap();

        assert_eq!(test_config.test_string, CHANGED_TEST_VALUE);
        assert!(test_config.test_bool);
    }

    #[test]
    fn utest_handle_config_default() {
        let test_config: TestConfig =
            handle_config(&None, &["/a/very/invalid/path/to/config/file"]).unwrap();

        assert_eq!(test_config, TestConfig::default());
    }
}

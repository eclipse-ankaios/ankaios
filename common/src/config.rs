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

use crate::std_extensions::GracefulExitResult;
use std::fmt;
use std::path::PathBuf;

// [impl->swdd~common-config-handling~1]

pub const CONFIG_VERSION: &str = "v1";

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

pub fn handle_config<T: ConfigFile>(config_path: &Option<String>, default_path: &str) -> T {
    match config_path {
        Some(config_path) => {
            let config_path = PathBuf::from(config_path);
            log::info!(
                "Loading config from user provided path '{}'",
                config_path.display()
            );
            T::from_file(config_path).unwrap_or_exit("Config file could not be parsed")
        }
        None => {
            let default_path = PathBuf::from(default_path);
            if !default_path.try_exists().unwrap_or(false) {
                log::debug!(
                    "No config file found at default path '{}'. Using cli arguments and environment variables only.",
                    default_path.display()
                );
                T::default()
            } else {
                log::info!(
                    "Loading config from default path '{}'",
                    default_path.display()
                );
                T::from_file(default_path).unwrap_or_exit("Config file could not be parsed")
            }
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
    use super::{ConfigFile, ConversionErrors, handle_config};
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
            "/a/very/invalid/path/to/config/file",
        );

        assert_eq!(test_config.test_string, "test_value");
        assert!(test_config.test_bool);
    }

    #[test]
    fn utest_handle_config_default_path() {
        let mut file = NamedTempFile::new().expect("Failed to create file");
        writeln!(file, "{VALID_TEST_CONFIG_CONTENT}").expect("Failed to write to file");

        let test_config: TestConfig = handle_config(&None, file.path().to_str().unwrap());

        assert_eq!(test_config.test_string, "test_value");
        assert!(test_config.test_bool);
    }

    #[test]
    fn utest_handle_config_default() {
        let test_config: TestConfig = handle_config(&None, "/a/very/invalid/path/to/config/file");

        assert_eq!(test_config, TestConfig::default());
    }
}

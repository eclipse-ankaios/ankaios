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

use base64::{engine::general_purpose, Engine};
use common::objects::{Base64Data, Data, File, FileContent};
use std::{fmt, fs::create_dir_all, path::Path, path::MAIN_SEPARATOR_STR};

use super::WorkloadConfigFilesPath;

#[derive(Debug, PartialEq)]
pub struct ConfigFileCreatorError {
    message: String,
}

impl ConfigFileCreatorError {
    pub fn new(message: String) -> Self {
        ConfigFileCreatorError { message }
    }
}

impl fmt::Display for ConfigFileCreatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to create config file: '{}'", self.message)
    }
}

pub struct ConfigFilesCreator;

impl ConfigFilesCreator {
    pub fn create_files(
        config_files_base_path: WorkloadConfigFilesPath,
        config_files: Vec<File>,
    ) -> Result<(), ConfigFileCreatorError> {
        for file in config_files {
            let mount_point = Path::new(&file.mount_point);

            Self::validate_mount_point(mount_point)?;

            let relative_mount_point = Self::remove_root_dir(mount_point);

            let mount_directory = relative_mount_point.parent().ok_or_else(|| {
                ConfigFileCreatorError::new(format!(
                    "invalid mount point: '{}': unable to get parent directory",
                    mount_point.display()
                ))
            })?;

            let host_config_file_dir = config_files_base_path.as_path_buf().join(mount_directory);

            create_dir_all(host_config_file_dir)
                .map_err(|err| ConfigFileCreatorError::new(err.to_string()))?;

            let host_config_file_path = config_files_base_path
                .as_path_buf()
                .join(relative_mount_point);

            let file_io_result = match &file.file_content {
                FileContent::Data(Data { data }) => std::fs::write(host_config_file_path, data),
                FileContent::BinaryData(Base64Data {
                    base64_data: binary_data,
                }) => {
                    let binary = general_purpose::STANDARD
                        .decode(binary_data)
                        .map_err(|err| ConfigFileCreatorError::new(err.to_string()))?;
                    std::fs::write(host_config_file_path, binary)
                }
            };

            file_io_result.map_err(|err| {
                ConfigFileCreatorError::new(format!(
                    "write failed for '{}': '{}'",
                    mount_point.display(),
                    err
                ))
            })?;
        }

        Ok(())
    }

    fn validate_mount_point(mount_point: &Path) -> Result<(), ConfigFileCreatorError> {
        let mut mount_point_components = mount_point.components();
        let first_component = mount_point_components.next();

        if first_component != Some(std::path::Component::RootDir) {
            return Err(ConfigFileCreatorError::new(format!(
                "invalid mount point: path '{}' is relative, expected absolute path",
                mount_point.display()
            )));
        }

        if mount_point.to_string_lossy().ends_with(MAIN_SEPARATOR_STR) {
            return Err(ConfigFileCreatorError::new(format!(
                "invalid mount point: '{}' is a directory, expected a file",
                mount_point.display()
            )));
        }

        if mount_point_components
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        {
            Ok(())
        } else {
            Err(ConfigFileCreatorError::new(format!(
                "invalid mount point: path '{}' contains invalid path components",
                mount_point.display()
            )))
        }
    }

    fn remove_root_dir(path: &Path) -> &Path {
        path.components()
            .next()
            .and_then(|first_path_part| path.strip_prefix(first_path_part).ok())
            .unwrap() // an absolute path has always at least one component that is a direct sub path
    }
}

#[cfg(test)]
mod tests {
    use super::{Base64Data, ConfigFilesCreator, Data, File, FileContent, WorkloadConfigFilesPath};
    use std::fs::read_to_string;
    const WORKLOAD_CONFIG_FILES_PATH: &str =
        "tmp/ankaios/agent_A_io/workload_A.12xyz3/config_files";
    const TEST_BASE64_DATA: &str = "ZGF0YQ=="; // "data" as base64
    const DECODED_TEST_BASE64_DATA: &str = "data";
    const TEST_CONFIG_FILE_DATA: &str = "some config";

    #[test]
    fn utest_config_files_creator_create_files() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_files_dir = tempdir.path().join(WORKLOAD_CONFIG_FILES_PATH);
        let config_files = vec![
            // Text based file
            File {
                mount_point: "/some/path/test.conf".to_string(),
                file_content: FileContent::Data(Data {
                    data: TEST_CONFIG_FILE_DATA.to_owned(),
                }),
            },
            // Binary file
            File {
                mount_point: "/hello".to_string(),
                file_content: FileContent::BinaryData(Base64Data {
                    base64_data: TEST_BASE64_DATA.to_owned(), // "data" as base64
                }),
            },
        ];

        assert_eq!(
            Ok(()),
            ConfigFilesCreator::create_files(
                WorkloadConfigFilesPath::new(config_files_dir.clone()),
                config_files
            )
        );

        let test1_content = read_to_string(config_files_dir.join("some/path/test.conf")).unwrap();
        assert_eq!(test1_content, TEST_CONFIG_FILE_DATA);

        let test2_content = std::fs::read(config_files_dir.join("hello")).unwrap();
        assert_eq!(test2_content, DECODED_TEST_BASE64_DATA.as_bytes());
    }

    #[test]
    fn utest_config_files_creator_create_config_files_fails_with_directory_instead_of_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_files_dir = tempdir.path().join(WORKLOAD_CONFIG_FILES_PATH);
        let config_files = vec![
            File {
                mount_point: "/".to_string(),
                file_content: FileContent::Data(Data {
                    data: TEST_CONFIG_FILE_DATA.to_owned(),
                }),
            },
            File {
                mount_point: "/invalid/".to_string(),
                file_content: FileContent::Data(Data {
                    data: TEST_CONFIG_FILE_DATA.to_owned(),
                }),
            },
        ];

        for file in config_files {
            let result = ConfigFilesCreator::create_files(
                WorkloadConfigFilesPath::new(config_files_dir.clone()),
                vec![file],
            );
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("is a directory, expected a file"));
        }
    }

    #[test]
    fn utest_config_files_creator_create_config_files_fails_with_invalid_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_files_dir = tempdir.path().join(WORKLOAD_CONFIG_FILES_PATH);
        let config_files = vec![File {
            mount_point: "/..".to_string(),
            file_content: FileContent::Data(Data {
                data: TEST_CONFIG_FILE_DATA.to_owned(),
            }),
        }];

        for file in config_files {
            let result = ConfigFilesCreator::create_files(
                WorkloadConfigFilesPath::new(config_files_dir.clone()),
                vec![file],
            );
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("contains invalid path components"));
        }
    }

    #[test]
    fn utest_config_files_creator_create_config_files_fails_with_relative_path() {
        let tempdir = tempfile::tempdir().unwrap();
        let config_files_dir = tempdir.path().join(WORKLOAD_CONFIG_FILES_PATH);
        let config_files = vec![
            File {
                mount_point: "".to_string(),
                file_content: FileContent::Data(Data {
                    data: TEST_CONFIG_FILE_DATA.to_owned(),
                }),
            },
            File {
                mount_point: "invalid/relative/mount/point/file.conf".to_string(),
                file_content: FileContent::Data(Data {
                    data: TEST_CONFIG_FILE_DATA.to_owned(),
                }),
            },
            File {
                mount_point: "relative".to_string(),
                file_content: FileContent::Data(Data {
                    data: TEST_CONFIG_FILE_DATA.to_owned(),
                }),
            },
        ];

        for file in config_files {
            let result = ConfigFilesCreator::create_files(
                WorkloadConfigFilesPath::new(config_files_dir.clone()),
                vec![file],
            );
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("is relative, expected absolute path"));
        }
    }
}

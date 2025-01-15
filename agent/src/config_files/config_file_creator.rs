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
use std::{
    fmt,
    path::{Path, PathBuf, MAIN_SEPARATOR_STR},
};

use super::WorkloadConfigFilesPath;

// module can be removed when existing filesystem io is extracted to common library within issue #431
#[allow(dead_code)]
mod config_file_io {
    use super::Path;

    #[cfg(test)]
    use mockall::automock;

    pub struct ConfigFileIo;

    #[cfg_attr(test, automock)]
    impl ConfigFileIo {
        pub fn write_file<C>(file_path: &Path, file_content: C) -> Result<(), std::io::Error>
        where
            C: AsRef<[u8]> + 'static,
        {
            std::fs::write(file_path, file_content)
        }

        pub fn create_dir_all(dir_path: &Path) -> Result<(), std::io::Error> {
            std::fs::create_dir_all(dir_path)
        }
    }
}

#[derive(Debug, Default)]
pub struct HostConfigFileLocation {
    pub directory: PathBuf,
    pub file_name: String,
}

impl HostConfigFileLocation {
    pub fn get_absolute_file_path(mut self) -> PathBuf {
        self.directory.push(self.file_name);
        self.directory
    }
}

impl TryFrom<(&WorkloadConfigFilesPath, &Path)> for HostConfigFileLocation {
    type Error = String;

    fn try_from(
        (config_files_base_path, mount_point): (&WorkloadConfigFilesPath, &Path),
    ) -> Result<Self, String> {
        let mount_point_as_string = mount_point.to_str().ok_or_else(|| {
            format!(
                "path '{}' is not a valid UTF-8 sequence",
                mount_point.display()
            )
        })?;

        if mount_point_as_string.ends_with(MAIN_SEPARATOR_STR) {
            return Err(format!(
                "'{}' is a directory, expected a file",
                mount_point.display()
            ));
        }

        let mut mount_point_components = mount_point.components().peekable();
        let first_component = mount_point_components.next();

        if first_component != Some(std::path::Component::RootDir) {
            return Err(format!(
                "path '{}' is relative, expected absolute path",
                mount_point.display()
            ));
        }

        let mut host_config_file_location = HostConfigFileLocation {
            directory: config_files_base_path.as_path_buf().clone(),
            ..Default::default()
        };

        while let Some(component) = mount_point_components.next() {
            match component {
                std::path::Component::Normal(_) => {}
                _ => {
                    return Err(format!(
                        "path '{}' contains invalid path components",
                        mount_point.display()
                    ));
                }
            }

            if mount_point_components.peek().is_some() {
                // component is not the last one
                host_config_file_location.directory.push(component);
            } else {
                // component is the last one and considered as the config file name
                host_config_file_location.file_name =
                    component.as_os_str().to_str().unwrap().to_owned(); // utf-8 compatibility is checked above
            }
        }

        Ok(host_config_file_location)
    }
}

#[cfg_attr(test, mockall_double::double)]
use config_file_io::ConfigFileIo;

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

            let host_config_file_location =
                HostConfigFileLocation::try_from((&config_files_base_path, mount_point)).map_err(
                    |err| {
                        ConfigFileCreatorError::new(format!(
                            "Invalid mount point '{}': '{}'",
                            mount_point.display(),
                            err
                        ))
                    },
                )?;

            ConfigFileIo::create_dir_all(&host_config_file_location.directory).map_err(|err| {
                ConfigFileCreatorError::new(format!(
                    "failed to create config file directory structure for '{}': '{}'",
                    mount_point.display(),
                    err
                ))
            })?;

            let host_config_file_path = host_config_file_location.get_absolute_file_path();
            Self::write_config_file(host_config_file_path.as_path(), file)?;
        }

        Ok(())
    }

    fn write_config_file(
        config_file_path: &Path,
        file: File,
    ) -> Result<(), ConfigFileCreatorError> {
        let file_io_result = match file.file_content {
            FileContent::Data(Data { data }) => ConfigFileIo::write_file(config_file_path, data),
            FileContent::BinaryData(Base64Data {
                base64_data: binary_data,
            }) => {
                let binary = general_purpose::STANDARD
                    .decode(binary_data)
                    .map_err(|err| {
                        ConfigFileCreatorError::new(format!("invalid base64 data: '{}'", err))
                    })?;
                ConfigFileIo::write_file(config_file_path, binary)
            }
        };

        file_io_result.map_err(|err| {
            ConfigFileCreatorError::new(format!(
                "write failed for '{}': '{}'",
                file.mount_point, err
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate;

    use super::{
        config_file_io::MockConfigFileIo, Base64Data, ConfigFileCreatorError, ConfigFilesCreator,
        Data, File, FileContent, HostConfigFileLocation, WorkloadConfigFilesPath,
    };
    use std::path::{Path, PathBuf};

    const WORKLOAD_CONFIG_FILES_PATH: &str =
        "/tmp/ankaios/agent_A_io/workload_A.12xyz3/config_files";
    const TEST_BASE64_DATA: &str = "ZGF0YQ=="; // "data" as base64
    const DECODED_TEST_BASE64_DATA: &str = "data";
    const TEST_CONFIG_FILE_DATA: &str = "some config";

    #[test]
    fn utest_config_files_creator_create_files() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let config_files_dir_base = PathBuf::from(WORKLOAD_CONFIG_FILES_PATH);
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

        let mock_create_dir_context = MockConfigFileIo::create_dir_all_context();
        mock_create_dir_context
            .expect()
            .once()
            .with(predicate::eq(config_files_dir_base.join("some/path")))
            .returning(|_| Ok(()));

        mock_create_dir_context
            .expect()
            .once()
            .with(predicate::eq(config_files_dir_base.clone()))
            .returning(|_| Ok(()));

        let mock_write_file_context = MockConfigFileIo::write_file_context();
        mock_write_file_context
            .expect()
            .once()
            .with(
                predicate::eq(config_files_dir_base.join("some/path/test.conf")),
                predicate::eq(TEST_CONFIG_FILE_DATA.to_owned()),
            )
            .returning(|_, _: String| Ok(()));

        mock_write_file_context
            .expect()
            .once()
            .with(
                predicate::eq(config_files_dir_base.join("hello")),
                predicate::eq(DECODED_TEST_BASE64_DATA.to_owned().as_bytes().to_vec()),
            )
            .returning(|_, _: Vec<u8>| Ok(()));

        assert_eq!(
            Ok(()),
            ConfigFilesCreator::create_files(
                WorkloadConfigFilesPath::new(config_files_dir_base),
                config_files
            )
        );
    }

    #[test]
    fn utest_config_files_creator_create_files_create_dir_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let config_files_dir_base = PathBuf::from(WORKLOAD_CONFIG_FILES_PATH);
        let config_files = vec![File {
            mount_point: "/some/path/test.conf".to_string(),
            file_content: FileContent::Data(Data {
                data: TEST_CONFIG_FILE_DATA.to_owned(),
            }),
        }];

        let mock_create_dir_context = MockConfigFileIo::create_dir_all_context();
        mock_create_dir_context
            .expect()
            .once()
            .returning(|_| Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)));

        let mock_write_file_context = MockConfigFileIo::write_file_context();
        mock_write_file_context.expect::<String>().never();

        assert_eq!(
            Err(ConfigFileCreatorError::new(
                "failed to create config file directory structure for '/some/path/test.conf': 'permission denied'".to_string()
            )),
            ConfigFilesCreator::create_files(
                WorkloadConfigFilesPath::new(config_files_dir_base),
                config_files
            )
        );
    }

    #[test]
    fn utest_config_files_creator_create_files_write_file_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let config_files_dir_base = PathBuf::from(WORKLOAD_CONFIG_FILES_PATH);
        let config_files = vec![File {
            mount_point: "/some/path/test.conf".to_string(),
            file_content: FileContent::Data(Data {
                data: TEST_CONFIG_FILE_DATA.to_owned(),
            }),
        }];

        let mock_create_dir_context = MockConfigFileIo::create_dir_all_context();
        mock_create_dir_context
            .expect()
            .once()
            .with(predicate::eq(config_files_dir_base.join("some/path")))
            .returning(|_| Ok(()));

        let mock_write_file_context = MockConfigFileIo::write_file_context();
        mock_write_file_context
            .expect()
            .once()
            .returning(|_, _: String| {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            });

        assert_eq!(
            Err(ConfigFileCreatorError::new(
                "write failed for '/some/path/test.conf': 'permission denied'".to_string()
            )),
            ConfigFilesCreator::create_files(
                WorkloadConfigFilesPath::new(config_files_dir_base),
                config_files
            )
        );
    }

    #[test]
    fn utest_config_files_creator_create_config_files_fails_with_invalid_path_components() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let config_files_dir = PathBuf::from(WORKLOAD_CONFIG_FILES_PATH);
        let config_files = vec![File {
            mount_point: "/..".to_string(),
            file_content: FileContent::Data(Data {
                data: TEST_CONFIG_FILE_DATA.to_owned(),
            }),
        }];

        let mock_create_dir_context = MockConfigFileIo::create_dir_all_context();
        mock_create_dir_context.expect().never();

        let mock_write_file_context = MockConfigFileIo::write_file_context();
        mock_write_file_context.expect::<String>().never();

        let result = ConfigFilesCreator::create_files(
            WorkloadConfigFilesPath::new(config_files_dir.clone()),
            config_files,
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("contains invalid path components"));
    }

    #[test]
    fn utest_host_config_file_location_try_from_fails_with_directory_instead_of_file() {
        let config_files_dir = PathBuf::from(WORKLOAD_CONFIG_FILES_PATH);
        let invalid_paths = vec![Path::new("/"), Path::new("/invalid/")];

        for path in invalid_paths {
            let result = HostConfigFileLocation::try_from((
                &WorkloadConfigFilesPath::new(config_files_dir.clone()),
                path,
            ));

            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("is a directory, expected a file"));
        }
    }

    #[test]
    fn utest_host_config_file_location_try_from_fails_with_relative_path() {
        let config_files_dir = PathBuf::from(WORKLOAD_CONFIG_FILES_PATH);
        let invalid_paths = vec![
            Path::new(""),
            Path::new("invalid/relative/mount/point/file.conf"),
            Path::new("relative"),
        ];

        for path in invalid_paths {
            let result = HostConfigFileLocation::try_from((
                &WorkloadConfigFilesPath::new(config_files_dir.clone()),
                path,
            ));
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("is relative, expected absolute path"));
        }
    }

    #[test]
    fn utest_config_files_creator_write_config_file_base64_decode_error() {
        let result = ConfigFilesCreator::write_config_file(
            Path::new("/some/host/file/path/to/binary"),
            File {
                mount_point: "/binary".to_string(),
                file_content: FileContent::BinaryData(Base64Data {
                    base64_data: "/invalid/base64".to_string(),
                }),
            },
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid base64 data"));
    }
}

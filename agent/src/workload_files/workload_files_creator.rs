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

use ankaios_api::ank_base::{FileContentSpec, FileSpec};
use base64::{Engine, engine::general_purpose};
use std::{
    collections::HashMap,
    fmt,
    path::{MAIN_SEPARATOR_STR, Path, PathBuf},
};

use super::WorkloadFilesBasePath;
#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::filesystem;
#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::filesystem_async;

#[cfg(test)]
use mockall::automock;

#[derive(Debug, Default)]
pub struct WorkloadFileHostPath {
    pub directory: PathBuf,
    pub file_name: String,
}

impl WorkloadFileHostPath {
    pub fn get_absolute_file_path(mut self) -> PathBuf {
        self.directory.push(self.file_name);
        self.directory
    }
}

// [impl->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
impl TryFrom<(&WorkloadFilesBasePath, &Path)> for WorkloadFileHostPath {
    type Error = String;

    fn try_from(
        (files_base_path, mount_point): (&WorkloadFilesBasePath, &Path),
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

        let mut host_workload_file_location = WorkloadFileHostPath {
            directory: files_base_path.to_path_buf().clone(),
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
                host_workload_file_location.directory.push(component);
            } else {
                // component is the last one and considered as the workload file name
                host_workload_file_location.file_name =
                    component.as_os_str().to_str().unwrap().to_owned(); // utf-8 compatibility is checked above
            }
        }

        Ok(host_workload_file_location)
    }
}

#[derive(Debug, PartialEq)]
pub struct WorkloadFileCreationError {
    message: String,
}

impl WorkloadFileCreationError {
    pub fn new(message: String) -> Self {
        WorkloadFileCreationError { message }
    }
}

impl fmt::Display for WorkloadFileCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to create workload file: '{}'", self.message)
    }
}

pub struct WorkloadFilesCreator;
pub type HostFilePath = PathBuf;
pub type MountPointPath = PathBuf;

#[cfg_attr(test, automock)]
impl WorkloadFilesCreator {
    // [impl->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    pub async fn create_files(
        workload_files_base_path: &WorkloadFilesBasePath,
        workload_files: &[FileSpec],
    ) -> Result<HashMap<HostFilePath, MountPointPath>, WorkloadFileCreationError> {
        let mut host_file_paths = HashMap::new();
        for file in workload_files {
            let mount_point = Path::new(&file.mount_point);

            let host_workload_file_location =
                WorkloadFileHostPath::try_from((workload_files_base_path, mount_point)).map_err(
                    |err| {
                        WorkloadFileCreationError::new(format!(
                            "invalid mount point '{}': '{}'",
                            mount_point.display(),
                            err
                        ))
                    },
                )?;

            filesystem::make_dir(&host_workload_file_location.directory).map_err(|err| {
                WorkloadFileCreationError::new(format!(
                    "failed to create workload file directory structure for '{}': '{}'",
                    mount_point.display(),
                    err
                ))
            })?;

            let workload_file_host_path = host_workload_file_location.get_absolute_file_path();
            Self::write_file(workload_file_host_path.as_path(), file).await?;
            host_file_paths.insert(workload_file_host_path, mount_point.to_path_buf());
        }

        Ok(host_file_paths)
    }

    async fn write_file(
        file_path: &Path,
        file: &FileSpec,
    ) -> Result<(), WorkloadFileCreationError> {
        let file_io_result = match &file.file_content {
            FileContentSpec::Data { data } => {
                filesystem_async::write_file(file_path, data.clone()).await
            }
            FileContentSpec::BinaryData { binary_data } => {
                // [impl->swdd~workload-files-creator-decodes-base64-to-binary~2]
                let binary_data = binary_data
                    .lines()
                    .map(|line| line.trim())
                    .collect::<String>();

                let binary = general_purpose::STANDARD
                    .decode(binary_data)
                    .map_err(|err| {
                        WorkloadFileCreationError::new(format!(
                            "invalid base64 data in '{}': '{}'",
                            file.mount_point, err
                        ))
                    })?;

                filesystem_async::write_file(file_path, binary).await
            }
        };

        file_io_result.map_err(|err| {
            WorkloadFileCreationError::new(format!(
                "write failed for '{}': '{}'",
                file.mount_point, err
            ))
        })
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
    use mockall::predicate;

    use crate::workload_files::generate_test_workload_files_path;

    use super::{FileContentSpec, FileSpec, WorkloadFileHostPath, WorkloadFilesCreator};

    use crate::io_utils::{FileSystemError, mock_filesystem, mock_filesystem_async};

    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
    };

    const TEST_BASE64_DATA: &str = "ZGF0YQ=="; // "data" as base64
    const DECODED_TEST_BASE64_DATA: &str = "data";
    const TEST_WORKLOAD_FILE_DATA: &str = "some config";

    // [utest->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    // [utest->swdd~workload-files-creator-decodes-base64-to-binary~2]
    #[tokio::test]
    async fn utest_workload_files_creator_create_files() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        let workload_files_path = generate_test_workload_files_path();

        let workload_files = vec![
            // Text based file
            FileSpec {
                mount_point: "/some/path/test.conf".to_string(),
                file_content: FileContentSpec::Data {
                    data: TEST_WORKLOAD_FILE_DATA.to_owned(),
                },
            },
            // Binary file
            FileSpec {
                mount_point: "/hello".to_string(),
                file_content: FileContentSpec::BinaryData {
                    binary_data: TEST_BASE64_DATA.to_owned(), // "data" as base64
                },
            },
        ];

        let mock_make_dir_context = mock_filesystem::make_dir_context();
        mock_make_dir_context
            .expect()
            .once()
            .with(predicate::eq(workload_files_path.join("some/path")))
            .returning(|_| Ok(()));

        mock_make_dir_context
            .expect()
            .once()
            .with(predicate::eq(workload_files_path.clone()))
            .returning(|_| Ok(()));

        let text_host_file_path = workload_files_path.join("some/path/test.conf");
        let mock_write_file_context = mock_filesystem_async::write_file_context();
        mock_write_file_context
            .expect()
            .once()
            .with(
                predicate::eq(text_host_file_path.clone()),
                predicate::eq(TEST_WORKLOAD_FILE_DATA.to_owned()),
            )
            .returning(|_, _: String| Ok(()));

        let binary_file_path = workload_files_path.join("hello");
        mock_write_file_context
            .expect()
            .once()
            .with(
                predicate::eq(binary_file_path.clone()),
                predicate::eq(DECODED_TEST_BASE64_DATA.to_owned().as_bytes().to_vec()),
            )
            .returning(|_, _: Vec<u8>| Ok(()));

        let expected_host_file_paths = HashMap::from([
            (text_host_file_path, PathBuf::from("/some/path/test.conf")),
            (binary_file_path, PathBuf::from("/hello")),
        ]);
        assert_eq!(
            Ok(expected_host_file_paths),
            WorkloadFilesCreator::create_files(&workload_files_path, &workload_files).await
        );
    }

    // [utest->swdd~workload-files-creator-decodes-base64-to-binary~2]
    #[tokio::test]
    async fn utest_workload_files_creator_create_files_decode_wrapped_base64_input() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let wrapped_base64_input = r#"
            VGhpcyBwaWVjZSBvZiB0ZXh0IGlzIHdyYXBwZWQgaW50byBtdWx0aXBsZSBsaW5lcyBhZnRlciBk
            ZWNvZGluZyB0byBiYXNlNjQu
        "#;

        let workload_files_path = generate_test_workload_files_path();

        let mock_make_dir_context = mock_filesystem::make_dir_context();

        mock_make_dir_context.expect().once().returning(|_| Ok(()));

        let binary_file_path = workload_files_path.join("binary");
        let mock_write_file_context = mock_filesystem_async::write_file_context();

        let expected_decoded_base64 =
            "This piece of text is wrapped into multiple lines after decoding to base64.";

        mock_write_file_context
            .expect()
            .once()
            .with(
                predicate::eq(binary_file_path.clone()),
                predicate::eq(expected_decoded_base64.to_owned().as_bytes().to_vec()),
            )
            .returning(|_, _: Vec<u8>| Ok(()));

        let expected_host_file_paths =
            HashMap::from([(binary_file_path, PathBuf::from("/binary"))]);

        let workload_files = [FileSpec {
            mount_point: "/binary".to_string(),
            file_content: FileContentSpec::BinaryData {
                binary_data: wrapped_base64_input.to_string(),
            },
        }];

        assert_eq!(
            Ok(expected_host_file_paths),
            WorkloadFilesCreator::create_files(&workload_files_path, &workload_files).await
        );
    }

    // [utest->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    #[tokio::test]
    async fn utest_workload_files_creator_create_files_create_dir_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let workload_files_path = generate_test_workload_files_path();
        let workload_files = vec![FileSpec {
            mount_point: "/some/path/test.conf".to_string(),
            file_content: FileContentSpec::Data {
                data: TEST_WORKLOAD_FILE_DATA.to_owned(),
            },
        }];

        let mock_make_dir_context = mock_filesystem::make_dir_context();
        mock_make_dir_context.expect().once().returning(|_| {
            Err(FileSystemError::Permissions(
                "/some/path/test.conf".into(),
                std::io::ErrorKind::Other,
            ))
        });

        let mock_write_file_context = mock_filesystem_async::write_file_context();
        mock_write_file_context.expect::<String>().never();

        let result =
            WorkloadFilesCreator::create_files(&workload_files_path, &workload_files).await;

        assert!(result.is_err());
        let expected_error_substring = "failed to create workload file directory structure";
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains(expected_error_substring),
            "Expected substring '{expected_error_substring}' in error, got '{error}'"
        );
    }

    // [utest->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    #[tokio::test]
    async fn utest_workload_files_creator_create_files_write_file_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let workload_files_path = generate_test_workload_files_path();
        let workload_files = vec![FileSpec {
            mount_point: "/some/path/test.conf".to_string(),
            file_content: FileContentSpec::Data {
                data: TEST_WORKLOAD_FILE_DATA.to_owned(),
            },
        }];

        let mock_make_dir_context = mock_filesystem::make_dir_context();
        mock_make_dir_context
            .expect()
            .once()
            .with(predicate::eq(workload_files_path.join("some/path")))
            .returning(|_| Ok(()));

        let mock_write_file_context = mock_filesystem_async::write_file_context();
        mock_write_file_context
            .expect()
            .once()
            .returning(|_, _: String| {
                Err(FileSystemError::Write(
                    "/some/path/test.conf".into(),
                    std::io::ErrorKind::Other,
                ))
            });

        let result =
            WorkloadFilesCreator::create_files(&workload_files_path, &workload_files).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        let expected_error_substring = "write failed for '/some/path/test.conf'";
        assert!(
            error.to_string().contains(expected_error_substring),
            "Expected substring '{expected_error_substring}' in error, got '{error}'"
        );
    }

    // [utest->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    #[tokio::test]
    async fn utest_workload_files_creator_create_files_fails_with_invalid_path_components() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let workload_files_path = generate_test_workload_files_path();
        let workload_files = vec![FileSpec {
            mount_point: "/..".to_string(),
            file_content: FileContentSpec::Data {
                data: TEST_WORKLOAD_FILE_DATA.to_owned(),
            },
        }];

        let mock_make_dir_context = mock_filesystem::make_dir_context();
        mock_make_dir_context.expect().never();

        let mock_write_file_context = mock_filesystem_async::write_file_context();
        mock_write_file_context.expect::<String>().never();

        let result =
            WorkloadFilesCreator::create_files(&workload_files_path, &workload_files).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        let expected_error_substring = "contains invalid path components";
        assert!(
            error.to_string().contains(expected_error_substring),
            "Expected substring '{expected_error_substring}' in error, got '{error}'"
        );
    }

    // [utest->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    #[test]
    fn utest_host_workload_file_location_try_from_fails_with_directory_instead_of_file() {
        let workload_files_path = generate_test_workload_files_path();
        let invalid_paths = vec![Path::new("/"), Path::new("/invalid/")];

        for path in invalid_paths {
            let result = WorkloadFileHostPath::try_from((&workload_files_path, path));

            assert!(result.is_err());
            let error = result.unwrap_err();
            let expected_error_substring = "is a directory, expected a file";
            assert!(
                error.to_string().contains(expected_error_substring),
                "Expected substring '{expected_error_substring}' in error, got '{error}'"
            );
        }
    }

    // [utest->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    #[test]
    fn utest_host_workload_file_location_try_from_fails_with_relative_path() {
        let workload_files_path = generate_test_workload_files_path();
        let invalid_paths = vec![
            Path::new(""),
            Path::new("invalid/relative/mount/point/file.conf"),
            Path::new("relative"),
        ];

        for path in invalid_paths {
            let result = WorkloadFileHostPath::try_from((&workload_files_path, path));
            assert!(result.is_err());
            let error = result.unwrap_err();
            let expected_error_substring = "is relative, expected absolute path";
            assert!(
                error.to_string().contains(expected_error_substring),
                "Expected substring '{expected_error_substring}' in error, got '{error}'"
            );
        }
    }

    // [utest->swdd~workload-files-creator-writes-files-at-mount-point-dependent-path~1]
    #[test]
    fn utest_host_workload_file_location_try_from_fails_invalid_path_components() {
        let workload_files_path = generate_test_workload_files_path();
        let invalid_path = Path::new("/some/bla/../more/file.txt");
        let result = WorkloadFileHostPath::try_from((&workload_files_path, invalid_path));
        assert!(result.is_err());
        let error = result.unwrap_err();
        let expected_error_substring = "contains invalid path components";
        assert!(
            error.to_string().contains(expected_error_substring),
            "Expected substring '{expected_error_substring}' in error, got '{error}'"
        );
    }

    // [utest->swdd~workload-files-creator-decodes-base64-to-binary~2]
    #[tokio::test]
    async fn utest_workload_files_creator_write_file_base64_decode_error() {
        let result = WorkloadFilesCreator::write_file(
            &PathBuf::from("/some/host/file/path/to/binary"),
            &FileSpec {
                mount_point: "/binary".to_string(),
                file_content: FileContentSpec::BinaryData {
                    binary_data: "/invalid/base64".to_string(),
                },
            },
        )
        .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        let expected_error_substring = "invalid base64 data";
        assert!(
            error.to_string().contains(expected_error_substring),
            "Expected substring '{expected_error_substring}' in error, got '{error}'"
        );
    }
}

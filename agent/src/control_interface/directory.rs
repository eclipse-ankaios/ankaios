// Copyright (c) 2023 Elektrobit Automotive GmbH
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

use std::path::PathBuf;

#[cfg_attr(test, mockall_double::double)]
use super::filesystem::FileSystem;
use super::filesystem::FileSystemError;

#[derive(Debug)]
pub struct Directory {
    path: PathBuf,
    filesystem: FileSystem,
}

impl Directory {
    pub fn new(path: PathBuf) -> Result<Self, FileSystemError> {
        let filesystem = FileSystem::new();
        if path.exists() {
            log::trace!("Reusing existing directory '{:?}'", path);
            return Ok(Self { path, filesystem });
        }
        match filesystem.make_dir(&path) {
            Ok(_) => Ok(Self { path, filesystem }),
            Err(err) => Err(err),
        }
    }
    pub fn get_path(&self) -> PathBuf {
        self.path.to_path_buf()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        log::debug!("Deleting directory '{:?}'", self.path);
        if let Err(err) = self.filesystem.remove_dir(&self.path) {
            log::warn!("Could not delete {:?}: {err}", self.path);
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
mockall::mock! {
    pub Directory {
        pub fn new(path: PathBuf) -> Result<Self, FileSystemError>;
        pub fn get_path(&self) -> PathBuf;
    }
    impl Drop for Directory {
        fn drop(&mut self);
    }
    impl PartialEq for Directory {
        fn eq(&self, other: &Self) -> bool;
    }
    impl std::fmt::Debug for Directory {
        fn fmt<'a>(&self, f: &mut std::fmt::Formatter<'a>) -> std::result::Result<(), std::fmt::Error>;
    }
}

#[cfg(test)]
pub fn generate_test_directory_mock(
    base_path: &str,
    sub_path: &str,
) -> __mock_MockDirectory::__new::Context {
    let directory_mock_context = MockDirectory::new_context();
    let expected_path = std::path::Path::new(&base_path.to_owned()).join(sub_path);
    directory_mock_context
        .expect()
        .with(mockall::predicate::eq(expected_path.to_path_buf()))
        .return_once(move |_| {
            let mut mock = MockDirectory::default();
            mock.expect_get_path().return_const(expected_path);
            mock.expect_drop().return_const(());
            Ok(mock)
        });
    directory_mock_context
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        path::Path,
        str::FromStr,
        sync::{Arc, Mutex},
    };

    use crate::control_interface::{filesystem::FileSystemError, MockFileSystem};
    use mockall::predicate;

    use super::Directory;

    #[test]
    fn utest_directory_new_ok_and_get_path_valid() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let directory_mock_context = MockFileSystem::new_context();
        directory_mock_context.expect().return_once(|| {
            let mut mock = MockFileSystem::default();
            mock.expect_make_dir()
                .with(predicate::eq(Path::new("test_path").to_path_buf()))
                .return_once(|_| Ok(()));
            mock.expect_remove_dir()
                .with(predicate::eq(Path::new("test_path").to_path_buf()))
                .return_once(|_| Ok(()));
            mock
        });

        let directory = Directory::new(Path::new("test_path").to_path_buf());
        assert!(directory.is_ok());
        assert_eq!(
            Path::new("test_path").to_path_buf(),
            directory.as_ref().unwrap().path
        );
        assert_eq!(
            Path::new("test_path").to_path_buf(),
            directory.unwrap().get_path()
        );
    }
    #[test]
    fn utest_directory_new_failed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let directory_mock_context = MockFileSystem::new_context();
        directory_mock_context.expect().return_once(|| {
            let mut mock = MockFileSystem::default();
            mock.expect_make_dir()
                .with(predicate::eq(Path::new("test_path").to_path_buf()))
                .return_once(|_| {
                    Err(FileSystemError::CreateDirectory(
                        OsString::from_str("Could not create directory").unwrap(),
                        std::io::ErrorKind::Other,
                    ))
                });
            mock.expect_remove_dir().never();
            mock
        });

        let directory = Directory::new(Path::new("test_path").to_path_buf());

        assert_eq!(
            directory.unwrap_err(),
            FileSystemError::CreateDirectory(
                OsString::from_str("Could not create directory").unwrap(),
                std::io::ErrorKind::Other,
            )
        );
    }
    #[test]
    fn utest_directory_new_remove_failed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let actual_error_list: Arc<Mutex<Vec<Result<(), FileSystemError>>>> =
            Arc::new(Mutex::from(vec![]));
        let actual_error_list_clone = actual_error_list.clone();
        let directory_mock_context = MockFileSystem::new_context();
        directory_mock_context.expect().return_once(move || {
            let mut mock = MockFileSystem::default();
            mock.expect_make_dir()
                .with(predicate::eq(Path::new("test_path").to_path_buf()))
                .return_once(|_| Ok(()));
            mock.expect_remove_dir()
                .with(predicate::eq(Path::new("test_path").to_path_buf()))
                .return_once(move |_| {
                    actual_error_list_clone.lock().unwrap().push(Err(
                        FileSystemError::RemoveDirectory(
                            OsString::from_str("Could not remove directory").unwrap(),
                            std::io::ErrorKind::Other,
                        ),
                    ));
                    Err(FileSystemError::RemoveDirectory(
                        OsString::from_str("Could not remove directory").unwrap(),
                        std::io::ErrorKind::Other,
                    ))
                });
            mock
        });

        let directory = Directory::new(Path::new("test_path").to_path_buf());
        assert!(directory.is_ok());
        drop(directory);

        let result = actual_error_list.lock().unwrap();
        assert!(matches!(
            result.first().unwrap(),
            Err(FileSystemError::RemoveDirectory(msg,_)) if msg == &OsString::from_str("Could not remove directory").unwrap()));
    }
}

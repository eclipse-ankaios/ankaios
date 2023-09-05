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
use super::FileSystem;
use super::FileSystemError;

#[derive(Debug)]
pub struct Fifo {
    path: PathBuf,
    filesystem: FileSystem,
}

impl Fifo {
    pub fn new(path: PathBuf) -> Result<Self, FileSystemError> {
        let filesystem = FileSystem::new();
        if filesystem.is_fifo(&path) {
            log::trace!("Reusing existing fifo file '{:?}'", path);
            Ok(Fifo { path, filesystem })
        } else {
            match filesystem.make_fifo(&path) {
                Ok(_) => Ok(Fifo { path, filesystem }),
                Err(err) => Err(err),
            }
        }
    }
    pub fn get_path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for Fifo {
    fn drop(&mut self) {
        if let Err(err) = self.filesystem.remove_fifo(&self.path) {
            log::error!("{}", err);
        }
    }
}

#[cfg(test)]
mockall::mock! {
    pub Fifo {
        pub fn new(path: PathBuf) -> Result<Self, FileSystemError>;
        pub fn get_path(&self) -> &PathBuf;
    }
    impl Drop for Fifo {
        fn drop(&mut self);
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

    use super::*;
    use crate::control_interface::MockFileSystem;
    use std::{io::ErrorKind, path::Path};

    #[test]
    fn utest_fifo_reuse_existing_and_remove_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();
        let mock = MockFileSystem::new_context();
        let mut file_system_mock = MockFileSystem::default();
        file_system_mock
            .expect_is_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(true);
        file_system_mock.expect_make_fifo().never();
        file_system_mock
            .expect_remove_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));
        mock.expect().return_once(|| file_system_mock);

        assert!(matches!(Fifo::new(test_path_buffer), Ok(_)));
    }

    #[test]
    fn utest_fifo_new_create_and_remove_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();
        let mock = MockFileSystem::new_context();
        let mut file_system_mock = MockFileSystem::default();
        file_system_mock
            .expect_is_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);
        file_system_mock
            .expect_make_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));
        file_system_mock
            .expect_remove_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));
        mock.expect().return_once(|| file_system_mock);

        assert!(matches!(Fifo::new(test_path_buffer), Ok(_)));
    }

    #[test]
    fn utest_fifo_new_create_failed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();
        let mock = MockFileSystem::new_context();
        let mut file_system_mock = MockFileSystem::default();
        file_system_mock
            .expect_is_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);
        file_system_mock
            .expect_make_fifo()
            .with(predicate::eq(Path::new("test_fifo")))
            .times(1)
            .return_once(|path| {
                Err(FileSystemError::CreateFifo(
                    path.to_path_buf().into_os_string(),
                    nix::errno::Errno::EACCES,
                ))
            });
        file_system_mock.expect_remove_fifo().never();
        mock.expect().return_once(|| file_system_mock);

        assert!(matches!(
            Fifo::new(test_path_buffer),
            Err(FileSystemError::CreateFifo(_, nix::errno::Errno::EACCES,))
        ));
    }
    #[test]
    fn utest_fifo_drop_remove_failed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();
        let mock = MockFileSystem::new_context();
        let mut file_system_mock = MockFileSystem::default();
        file_system_mock
            .expect_is_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);
        file_system_mock
            .expect_make_fifo()
            .with(predicate::eq(Path::new("test_fifo")))
            .times(1)
            .return_once(|_| Ok(()));
        file_system_mock
            .expect_remove_fifo()
            .with(predicate::eq(Path::new("test_fifo")))
            .times(1)
            .return_once(|path| {
                Err(FileSystemError::RemoveFifo(
                    path.to_path_buf().into_os_string(),
                    ErrorKind::Other,
                ))
            });
        mock.expect().return_once(|| file_system_mock);

        let fifo = Fifo::new(test_path_buffer);
        assert!(matches!(fifo, Ok(_)));
    }

    #[test]
    fn utest_fifo_get_path() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();
        let mock = MockFileSystem::new_context();
        let mut file_system_mock = MockFileSystem::default();
        file_system_mock
            .expect_is_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);
        file_system_mock
            .expect_make_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));
        file_system_mock
            .expect_remove_fifo()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));
        mock.expect().return_once(|| file_system_mock);

        let fifo = Fifo::new(test_path_buffer);
        assert!(matches!(fifo, Ok(_)));
        assert_eq!(
            &Path::new("test_fifo").to_path_buf(),
            fifo.unwrap().get_path()
        );
    }
}

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

use nix::errno::Errno;
#[cfg(not(test))]
use nix::unistd::mkfifo;
use std::ffi::OsString;
use std::fmt::{self, Display};
#[cfg(not(test))]
use std::fs::{create_dir_all, metadata, remove_dir, remove_file, set_permissions};
#[cfg(not(test))]
use std::os::unix::fs::FileTypeExt;
#[cfg(test)]
use tests::{create_dir_all, metadata, mkfifo, remove_dir, remove_file, set_permissions};

use nix::sys::stat::Mode;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[cfg(test)]
use mockall::automock;

#[derive(Debug, PartialEq)]
pub enum FileSystemError {
    CreateDirectory(OsString, std::io::ErrorKind),
    NotFoundDirectory(OsString),
    CreateFifo(OsString, Errno),
    RemoveFifo(OsString, std::io::ErrorKind),
    RemoveDirectory(OsString, std::io::ErrorKind),
    Permissions(OsString, std::io::ErrorKind),
}

impl Display for FileSystemError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileSystemError::CreateDirectory(path, err) => {
                write!(f, "Could not create directory {path:?}: {err}")
            }
            FileSystemError::CreateFifo(path, err) => {
                write!(f, "Could not create fifo {path:?}: {err}")
            }
            FileSystemError::RemoveFifo(path, err) => {
                write!(f, "Could not remove fifo {path:?} {err:?}")
            }
            FileSystemError::RemoveDirectory(path, err) => {
                write!(f, "Could not remove directory {path:?} {err:?}")
            }
            FileSystemError::NotFoundDirectory(path) => {
                write!(f, "Could not find directory {path:?}")
            }
            FileSystemError::Permissions(path, err) => {
                write!(f, "Could not set permissions to {path:?}  {err:?}")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileSystem {}

#[cfg_attr(test, automock)]
impl FileSystem {
    pub fn new() -> Self {
        Self {}
    }

    // Unit testing this function is too hard and not worth taking into account that it is just calling one line of code
    #[cfg_attr(test, allow(dead_code))]
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    pub fn set_permissions(path: &Path, mode: u32) -> Result<(), FileSystemError> {
        set_permissions(path, Permissions::from_mode(mode))
            .map_err(|err| FileSystemError::Permissions(path.as_os_str().to_owned(), err.kind()))
    }

    pub fn is_fifo(&self, path: &Path) -> bool {
        if let Ok(meta) = metadata(path) {
            return meta.file_type().is_fifo();
        }
        false
    }
    pub fn make_fifo(&self, path: &Path) -> Result<(), FileSystemError> {
        match mkfifo(path, Mode::S_IRWXU) {
            Ok(_) => Ok(()),
            Err(err) => Err(FileSystemError::CreateFifo(
                path.as_os_str().to_owned(),
                err,
            )),
        }
    }
    pub fn remove_fifo(&self, path: &Path) -> Result<(), FileSystemError> {
        if let Err(err) = remove_file(path) {
            return Err(FileSystemError::RemoveFifo(
                path.to_path_buf().into_os_string(),
                err.kind(),
            ));
        }

        Ok(())
    }
    pub fn make_dir(&self, path: &Path) -> Result<(), FileSystemError> {
        match create_dir_all(path) {
            Ok(_) => Ok(()),
            Err(err) => Err(FileSystemError::CreateDirectory(
                path.as_os_str().to_owned(),
                err.kind(),
            )),
        }
    }
    pub fn remove_dir(&self, path: &Path) -> Result<(), FileSystemError> {
        if let Err(err) = remove_dir(path) {
            return Err(FileSystemError::RemoveDirectory(
                path.to_path_buf().into_os_string(),
                err.kind(),
            ));
        }

        Ok(())
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
    use std::{
        collections::VecDeque,
        fs::Permissions,
        io::{self, Error, ErrorKind},
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use mockall::lazy_static;

    use super::{FileSystem, FileSystemError, Mode};

    #[allow(non_camel_case_types)]
    pub enum FakeCall {
        create_dir_all(PathBuf, io::Result<()>), // create_dir_all(path, fake_result)
        mkfifo(PathBuf, Mode, nix::Result<()>),  // mkfifo(path, mode, fake_result)
        remove_dir(PathBuf, io::Result<()>),     // remove_dir(path, fake_result)
        remove_file(PathBuf, io::Result<()>),    // remove_file(path, fake_result)
        metadata(PathBuf, io::Result<Metadata>), // metadata(path, fake_result)
        set_permissions(PathBuf, u32, io::Result<()>), // set_permissions(path, mode, fake_result)
    }

    lazy_static! {
        pub static ref FAKE_CALL_LIST: Mutex<VecDeque<FakeCall>> = Mutex::new(VecDeque::new());
        pub static ref TEST_LOCK: Mutex<()> = Mutex::new(());
    }

    #[derive(Debug, PartialEq, Eq, Copy, Clone)]
    pub struct FileType {
        is_fifo: bool,
    }
    impl FileType {
        pub fn new(is_fifo: bool) -> Self {
            FileType { is_fifo }
        }
        pub fn is_fifo(&self) -> bool {
            self.is_fifo
        }
    }
    pub struct Metadata {
        file_type: FileType,
    }

    impl Metadata {
        pub fn new(file_type: FileType) -> Self {
            Metadata { file_type }
        }
        pub fn file_type(&self) -> FileType {
            self.file_type
        }
    }

    pub fn create_dir_all(path: &Path) -> io::Result<()> {
        if let Some(FakeCall::create_dir_all(fake_path, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == *path {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call create_dir_all({})",
            path.to_string_lossy()
        );
    }
    pub fn metadata(path: &Path) -> io::Result<Metadata> {
        if let Some(FakeCall::metadata(fake_path, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == *path {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call metadata({})",
            path.to_string_lossy()
        );
    }
    pub fn mkfifo(path: &Path, mode: Mode) -> nix::Result<()> {
        if let Some(FakeCall::mkfifo(fake_path, fake_mode, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == *path && fake_mode == mode {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call mkfifo({}, {:?})",
            path.to_string_lossy(),
            mode
        );
    }
    pub fn remove_dir(path: &Path) -> io::Result<()> {
        if let Some(FakeCall::remove_dir(fake_path, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == *path {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call remove_dir({})",
            path.to_string_lossy()
        );
    }

    pub fn remove_file(path: &Path) -> io::Result<()> {
        if let Some(FakeCall::remove_file(fake_path, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == path.to_path_buf() {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call remove_file({})",
            path.to_string_lossy()
        );
    }

    pub fn set_permissions(path: &Path, perm: Permissions) -> io::Result<()> {
        if let Some(FakeCall::set_permissions(fake_path, fake_mode, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == path && fake_mode == perm.mode() {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call set_permissions({:?}, {:?})",
            path, perm
        );
    }

    #[test]
    fn utest_set_permissions_ok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::set_permissions(
                Path::new("test_dir").to_path_buf(),
                0o777,
                Ok(()),
            ));

        assert!(FileSystem::set_permissions(Path::new("test_dir"), 0o777).is_ok());
    }
    #[test]
    fn utest_set_permissions_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::set_permissions(
                Path::new("test_dir").to_path_buf(),
                0o777,
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "some error")),
            ));

        assert_eq!(FileSystem::set_permissions(
            Path::new("test_dir"),
            0o777),
            Err(FileSystemError::Permissions(
                Path::new("test_dir").as_os_str().to_owned(),
                std::io::ErrorKind::PermissionDenied
            ))
        );
    }

    #[test]
    fn utest_filesystem_make_dir_ok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::create_dir_all(
                Path::new("test_dir").to_path_buf(),
                Ok(()),
            ));
        let fs: FileSystem = FileSystem::new();
        assert!(fs.make_dir(Path::new("test_dir")).is_ok());
    }
    #[test]
    fn utest_filesystem_make_dir_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::create_dir_all(
                Path::new("test_dir").to_path_buf(),
                Err(std::io::Error::new(std::io::ErrorKind::Other, "some error")),
            ));
        let fs = FileSystem::new();
        assert_eq!(
            fs.make_dir(Path::new("test_dir")),
            Err(FileSystemError::CreateDirectory(
                Path::new("test_dir").as_os_str().to_owned(),
                std::io::ErrorKind::Other
            ))
        );
    }
    #[test]
    fn utest_filesystem_make_fifo_ok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::mkfifo(
            Path::new("test_fifo").to_path_buf(),
            Mode::S_IRWXU,
            Ok(()),
        ));
        let fs = FileSystem::new();
        assert!(fs.make_fifo(Path::new("test_fifo")).is_ok());
    }
    #[test]
    fn utest_filesystem_make_fifo_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::mkfifo(
            Path::new("test_fifo").to_path_buf(),
            Mode::S_IRWXU,
            Err(nix::Error::EACCES),
        ));
        let fs = FileSystem::new();
        assert!(matches!(
            fs.make_fifo(Path::new("test_fifo")),
            Err(FileSystemError::CreateFifo(_, nix::Error::EACCES))
        ));
    }
    #[test]
    fn utest_filesystem_is_fifo_ok_true() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::metadata(
            Path::new("test_fifo").to_path_buf(),
            Ok(Metadata::new(FileType::new(true))),
        ));
        let fs = FileSystem::new();
        assert!(fs.is_fifo(Path::new("test_fifo")));
    }
    #[test]
    fn utest_filesystem_is_fifo_ok_false() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::metadata(
            Path::new("test_fifo").to_path_buf(),
            Ok(Metadata::new(FileType::new(false))),
        ));
        let fs = FileSystem::new();
        assert!(!fs.is_fifo(Path::new("test_fifo")));
    }
    #[test]
    fn utest_filesystem_is_fifo_nok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::metadata(
            Path::new("test_fifo").to_path_buf(),
            Err(std::io::Error::new(ErrorKind::Other, "oh no!")),
        ));
        let fs = FileSystem::new();
        assert!(!fs.is_fifo(Path::new("test_fifo")));
    }
    #[test]
    fn utest_filesystem_remove_dir_ok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_dir(
                Path::new("test_dir").to_path_buf(),
                Ok(()),
            ));
        let fs = FileSystem::new();
        assert!(fs.remove_dir(Path::new("test_dir")).is_ok());
    }
    #[test]
    fn utest_filesystem_remove_dir_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_dir(
                Path::new("test_dir").to_path_buf(),
                Err(Error::new(ErrorKind::Other, "Some Error!")),
            ));
        let fs = FileSystem::new();
        assert!(matches!(
            fs.remove_dir(Path::new("test_dir")),
            Err(FileSystemError::RemoveDirectory(_, _))
        ));
    }
    #[test]
    fn utest_filesystem_remove_fifo_ok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_file(
                Path::new("test_file").to_path_buf(),
                Ok(()),
            ));
        let fs = FileSystem::new();
        assert!(fs.remove_fifo(Path::new("test_file")).is_ok());
    }
    #[test]
    fn utest_filesystem_remove_fifo_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_file(
                Path::new("test_file").to_path_buf(),
                Err(Error::new(ErrorKind::Other, "Some Error!")),
            ));
        let fs = FileSystem::new();
        assert!(matches!(
            fs.remove_fifo(Path::new("test_file")),
            Err(FileSystemError::RemoveFifo(_, _))
        ));
    }
}

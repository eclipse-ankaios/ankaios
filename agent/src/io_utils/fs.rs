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
use std::{
    ffi::OsString,
    fmt::{self, Display},
    io::ErrorKind,
};

#[cfg(test)]
use mockall::automock;

#[derive(Debug, PartialEq)]
pub enum FileSystemError {
    CreateDirectory(OsString, ErrorKind),
    NotFoundDirectory(OsString),
    CreateFifo(OsString, Errno),
    RemoveFifo(OsString, ErrorKind),
    RemoveDirectory(OsString, ErrorKind),
    Permissions(OsString, ErrorKind),
    Write(OsString, ErrorKind),
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
            FileSystemError::Write(path, err) => {
                write!(f, "Could not write to {path:?}  {err:?}")
            }
        }
    }
}

#[cfg_attr(test, automock)]
pub mod filesystem {

    #[cfg(not(test))]
    use nix::unistd::mkfifo;

    use super::FileSystemError;
    #[cfg(test)]
    use super::tests::{
        create_dir_all, metadata, mkfifo, remove_dir_all as fs_remove_dir_all, remove_file,
        set_permissions as fs_set_permissions,
    };
    #[cfg(not(test))]
    use std::fs::{
        create_dir_all, metadata, remove_dir_all as fs_remove_dir_all, remove_file,
        set_permissions as fs_set_permissions,
    };
    #[cfg(not(test))]
    use std::os::unix::fs::FileTypeExt;

    use nix::sys::stat::Mode;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    // Unit testing this function is too hard and not worth taking into account that it is just calling one line of code
    #[cfg_attr(test, allow(dead_code))]
    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    pub fn set_permissions(path: &Path, mode: u32) -> Result<(), FileSystemError> {
        fs_set_permissions(path, Permissions::from_mode(mode))
            .map_err(|err| FileSystemError::Permissions(path.as_os_str().to_owned(), err.kind()))
    }

    pub fn is_fifo(path: &Path) -> bool {
        if let Ok(meta) = metadata(path) {
            return meta.file_type().is_fifo();
        }
        false
    }

    pub fn make_fifo(path: &Path) -> Result<(), FileSystemError> {
        mkfifo(path, Mode::S_IRWXU)
            .map_err(|err| FileSystemError::CreateFifo(path.as_os_str().to_owned(), err))
    }

    pub fn remove_fifo(path: &Path) -> Result<(), FileSystemError> {
        remove_file(path).map_err(|err| {
            FileSystemError::RemoveFifo(path.to_path_buf().into_os_string(), err.kind())
        })
    }

    pub fn make_dir(path: &Path) -> Result<(), FileSystemError> {
        create_dir_all(path).map_err(|err| {
            FileSystemError::CreateDirectory(path.as_os_str().to_owned(), err.kind())
        })
    }

    pub fn remove_dir_all(path: &Path) -> Result<(), FileSystemError> {
        fs_remove_dir_all(path).map_err(|err| {
            FileSystemError::RemoveDirectory(path.to_path_buf().into_os_string(), err.kind())
        })
    }
}

#[cfg_attr(test, automock)]
pub mod filesystem_async {
    use super::FileSystemError;
    #[cfg(test)]
    use super::tests::{remove_dir_all_async as fs_async_remove_dir_all, write as fs_async_write};

    use std::path::Path;
    #[cfg(not(test))]
    use tokio::fs::{remove_dir_all as fs_async_remove_dir_all, write as fs_async_write};

    pub async fn write_file<C>(file_path: &Path, file_content: C) -> Result<(), FileSystemError>
    where
        C: AsRef<[u8]> + 'static,
    {
        fs_async_write(file_path, file_content)
            .await
            .map_err(|err| FileSystemError::Write(file_path.into(), err.kind()))
    }

    pub async fn remove_dir_all(path: &Path) -> Result<(), FileSystemError> {
        fs_async_remove_dir_all(path)
            .await
            .map_err(|err| match err.kind() {
                tokio::io::ErrorKind::NotFound => FileSystemError::NotFoundDirectory(path.into()),
                _ => FileSystemError::RemoveDirectory(path.into(), err.kind()),
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
    use std::{
        collections::VecDeque,
        fs::Permissions,
        io::{self, Error, ErrorKind},
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use mockall::lazy_static;
    use nix::sys::stat::Mode;

    use super::{FileSystemError, filesystem, filesystem_async};

    #[allow(non_camel_case_types)]
    pub enum FakeCall {
        create_dir_all(PathBuf, io::Result<()>), // create_dir_all(path, fake_result)
        mkfifo(PathBuf, Mode, nix::Result<()>),  // mkfifo(path, mode, fake_result)
        remove_dir_all(PathBuf, io::Result<()>), // remove_dir_all(path, fake_result)
        remove_file(PathBuf, io::Result<()>),    // remove_file(path, fake_result)
        metadata(PathBuf, io::Result<Metadata>), // metadata(path, fake_result)
        set_permissions(PathBuf, u32, io::Result<()>), // set_permissions(path, mode, fake_result)
        write(PathBuf, Vec<u8>, io::Result<()>), // write(path, content, fake_result)
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

    pub fn remove_dir_all(path: &Path) -> io::Result<()> {
        if let Some(FakeCall::remove_dir_all(fake_path, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == *path {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call remove_dir_all({})",
            path.to_string_lossy()
        );
    }

    pub async fn remove_dir_all_async(path: &Path) -> io::Result<()> {
        remove_dir_all(path)
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

        panic!("No mock specified for call set_permissions({path:?}, {perm:?})");
    }

    pub async fn write<C>(path: &Path, file_content: C) -> io::Result<()>
    where
        C: AsRef<[u8]> + 'static,
    {
        if let Some(FakeCall::write(fake_path, fake_content, fake_result)) =
            FAKE_CALL_LIST.lock().unwrap().pop_front()
        {
            if fake_path == *path && fake_content == file_content.as_ref() {
                return fake_result;
            }
        }

        panic!(
            "No mock specified for call write({})",
            path.to_string_lossy()
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

        assert!(filesystem::set_permissions(Path::new("test_dir"), 0o777).is_ok());
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
                Err(Error::new(ErrorKind::PermissionDenied, "some error")),
            ));

        assert_eq!(
            filesystem::set_permissions(Path::new("test_dir"), 0o777),
            Err(FileSystemError::Permissions(
                Path::new("test_dir").as_os_str().to_owned(),
                ErrorKind::PermissionDenied
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
        assert!(filesystem::make_dir(Path::new("test_dir")).is_ok());
    }

    #[test]
    fn utest_filesystem_make_dir_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::create_dir_all(
                Path::new("test_dir").to_path_buf(),
                Err(Error::other("some error")),
            ));

        assert_eq!(
            filesystem::make_dir(Path::new("test_dir")),
            Err(FileSystemError::CreateDirectory(
                Path::new("test_dir").as_os_str().to_owned(),
                ErrorKind::Other
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

        assert!(filesystem::make_fifo(Path::new("test_fifo")).is_ok());
    }

    #[test]
    fn utest_filesystem_make_fifo_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::mkfifo(
            Path::new("test_fifo").to_path_buf(),
            Mode::S_IRWXU,
            Err(nix::Error::EACCES),
        ));

        assert!(matches!(
            filesystem::make_fifo(Path::new("test_fifo")),
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

        assert!(filesystem::is_fifo(Path::new("test_fifo")));
    }

    #[test]
    fn utest_filesystem_is_fifo_ok_false() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::metadata(
            Path::new("test_fifo").to_path_buf(),
            Ok(Metadata::new(FileType::new(false))),
        ));

        assert!(!filesystem::is_fifo(Path::new("test_fifo")));
    }

    #[test]
    fn utest_filesystem_is_fifo_nok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::metadata(
            Path::new("test_fifo").to_path_buf(),
            Err(Error::other("oh no!")),
        ));

        assert!(!filesystem::is_fifo(Path::new("test_fifo")));
    }

    #[test]
    fn utest_filesystem_remove_dir_ok() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_dir_all(
                Path::new("test_dir").to_path_buf(),
                Ok(()),
            ));

        assert!(filesystem::remove_dir_all(Path::new("test_dir")).is_ok());
    }

    #[test]
    fn utest_filesystem_remove_dir_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_dir_all(
                Path::new("test_dir").to_path_buf(),
                Err(Error::other("Some Error!")),
            ));

        assert!(matches!(
            filesystem::remove_dir_all(Path::new("test_dir")),
            Err(FileSystemError::RemoveDirectory(_, _))
        ));
    }

    #[tokio::test]
    async fn utest_filesystem_remove_dir_async_ok() {
        let _test_lock = TEST_LOCK.lock();
        let path = Path::new("test_dir");
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_dir_all(path.to_path_buf(), Ok(())));

        assert!(filesystem_async::remove_dir_all(path).await.is_ok());
    }

    #[tokio::test]
    async fn utest_filesystem_remove_dir_async_fails_with_path_not_found() {
        let _test_lock = TEST_LOCK.lock();
        let path = Path::new("test_dir");
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_dir_all(
                path.to_path_buf(),
                Err(Error::new(ErrorKind::NotFound, "Path not found!")),
            ));

        let result = filesystem_async::remove_dir_all(path).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            matches!(&error, FileSystemError::NotFoundDirectory(_)),
            "Expected FileSystemError::NotFoundDirectory, got {error:?}"
        );
    }

    #[tokio::test]
    async fn utest_filesystem_remove_dir_async_fails_with_generic_reason() {
        let _test_lock = TEST_LOCK.lock();
        let path = Path::new("test_dir");
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_dir_all(
                path.to_path_buf(),
                Err(Error::other("Some Error!")),
            ));

        let result = filesystem_async::remove_dir_all(path).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(
            matches!(&error, FileSystemError::RemoveDirectory(_, _)),
            "Expected FileSystemError::RemoveDirectory, got {error:?}"
        );
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

        assert!(filesystem::remove_fifo(Path::new("test_file")).is_ok());
    }

    #[test]
    fn utest_filesystem_remove_fifo_failed() {
        let _test_lock = TEST_LOCK.lock();
        FAKE_CALL_LIST
            .lock()
            .unwrap()
            .push_back(FakeCall::remove_file(
                Path::new("test_file").to_path_buf(),
                Err(Error::other("Some Error!")),
            ));

        assert!(matches!(
            filesystem::remove_fifo(Path::new("test_file")),
            Err(FileSystemError::RemoveFifo(_, _))
        ));
    }

    #[tokio::test]
    async fn utest_write_file_async_ok() {
        let _test_lock = TEST_LOCK.lock();
        let path = Path::new("test_file");
        let file_content = vec![1, 2, 3];
        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::write(
            path.to_path_buf(),
            file_content.clone(),
            Ok(()),
        ));

        assert!(
            filesystem_async::write_file(path, file_content)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn utest_write_file_async_fails() {
        let _test_lock = TEST_LOCK.lock();

        let path = Path::new("test_file");
        let io_error_kind = ErrorKind::Other;
        let file_content = vec![1, 2, 3];

        FAKE_CALL_LIST.lock().unwrap().push_back(FakeCall::write(
            path.to_path_buf(),
            file_content.clone(),
            Err(Error::other("Some Error!")),
        ));

        let result = filesystem_async::write_file(path, file_content).await;
        assert_eq!(
            result,
            Err(FileSystemError::Write(
                path.as_os_str().to_os_string(),
                io_error_kind,
            )),
        );
    }
}

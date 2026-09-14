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

use super::fs::FileSystemError;
#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::filesystem;

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Directory {
    path: PathBuf,
}

impl Directory {
    pub fn new(path: PathBuf) -> Result<Self, FileSystemError> {
        ensure_dir_exists_and_secure(&path)?;
        Ok(Self { path })
    }

    pub fn get_path(&self) -> PathBuf {
        self.path.to_path_buf()
    }
}

// Ensures `path` exists and is exclusively owned by the current user, creating it
// (with an explicit, umask-independent mode) if missing.
// [impl->swdd~agent-rejects-insecure-reused-run-folder-paths~1]
pub fn ensure_dir_exists_and_secure(path: &Path) -> Result<(), FileSystemError> {
    if path.exists() {
        ensure_secure(path)?;
        log::trace!("Reusing existing directory '{path:?}'");
        return Ok(());
    }

    // `create_dir_all` can silently create several missing ancestor levels in one
    // call (e.g. a workload's own directory together with a subdirectory below it).
    // Walk up to the first already-existing ancestor, validate it, then create and
    // explicitly secure every missing level individually, one directory at a time,
    // so none of them is left with umask-dependent (and possibly insecure) permissions.
    let mut missing_levels = vec![path];
    let mut current = path;
    while let Some(parent) = current.parent() {
        if filesystem::exists(parent) {
            ensure_secure(parent)?;
            break;
        }
        missing_levels.push(parent);
        current = parent;
    }

    for level in missing_levels.into_iter().rev() {
        filesystem::make_dir(level)?;
        // Set the mode explicitly: relying on the process umask could leave the
        // directory group/other-writable.
        filesystem::set_permissions(level, 0o700)?;
    }
    Ok(())
}

fn ensure_secure(path: &Path) -> Result<(), FileSystemError> {
    if filesystem::is_owner_exclusive(path) {
        Ok(())
    } else {
        Err(FileSystemError::InsecurePermissions(
            path.as_os_str().to_owned(),
        ))
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        log::debug!("Deleting directory '{:?}'", self.path);
        if let Err(err) = filesystem::remove_dir_all(&self.path) {
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
    use super::Directory;
    use crate::io_utils::{mock_filesystem, FileSystemError};
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

    use mockall::predicate;
    use std::{
        ffi::OsString,
        str::FromStr,
        sync::{Arc, Mutex},
    };

    #[test]
    fn utest_directory_new_ok_and_get_path_valid() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let tempdir = tempfile::tempdir().unwrap();
        let parent = tempdir.path().to_path_buf();
        let path = parent.join("child_dir");

        let exists_context = mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);
        let is_secure_context = mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_once(|_| Ok(()));

        let set_permissions_context = mock_filesystem::set_permissions_context();
        set_permissions_context
            .expect()
            .with(predicate::eq(path.clone()), predicate::eq(0o700))
            .return_once(|_, _| Ok(()));

        let rm_dir_context = mock_filesystem::remove_dir_all_context();
        rm_dir_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_once(|_| Ok(()));

        let directory = Directory::new(path.clone());
        assert!(directory.is_ok());
        assert_eq!(path, directory.as_ref().unwrap().path);
        assert_eq!(path, directory.unwrap().get_path());
    }
    #[test]
    fn utest_directory_new_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let tempdir = tempfile::tempdir().unwrap();
        let parent = tempdir.path().to_path_buf();
        let path = parent.join("child_dir");

        let exists_context = mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);
        let is_secure_context = mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_once(|_| {
                Err(FileSystemError::CreateDirectory(
                    OsString::from_str("Could not create directory").unwrap(),
                    std::io::ErrorKind::Other,
                ))
            });

        let rm_dir_context = mock_filesystem::remove_dir_all_context();
        rm_dir_context.expect().never();

        let directory = Directory::new(path.clone());

        assert_eq!(
            directory.unwrap_err(),
            FileSystemError::CreateDirectory(
                OsString::from_str("Could not create directory").unwrap(),
                std::io::ErrorKind::Other,
            )
        );
    }

    // Directories must not rely on the process umask, which can leave them
    // group/other-writable and fail ensure_secure() on the next reuse.
    #[test]
    fn utest_directory_new_set_permissions_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let tempdir = tempfile::tempdir().unwrap();
        let parent = tempdir.path().to_path_buf();
        let path = parent.join("child_dir");

        let exists_context = mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);
        let is_secure_context = mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_once(|_| Ok(()));

        let set_permissions_context = mock_filesystem::set_permissions_context();
        set_permissions_context
            .expect()
            .with(predicate::eq(path.clone()), predicate::eq(0o700))
            .return_once(|_, _| {
                Err(FileSystemError::Permissions(
                    OsString::from_str("Could not set permissions").unwrap(),
                    std::io::ErrorKind::Other,
                ))
            });

        let rm_dir_context = mock_filesystem::remove_dir_all_context();
        rm_dir_context.expect().never();

        let directory = Directory::new(path);

        assert_eq!(
            directory.unwrap_err(),
            FileSystemError::Permissions(
                OsString::from_str("Could not set permissions").unwrap(),
                std::io::ErrorKind::Other,
            )
        );
    }
    #[test]
    fn utest_directory_new_remove_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let tempdir = tempfile::tempdir().unwrap();
        let parent = tempdir.path().to_path_buf();
        let path = parent.join("child_dir");

        let actual_error_list: Arc<Mutex<Vec<Result<(), FileSystemError>>>> =
            Arc::new(Mutex::from(vec![]));
        let actual_error_list_clone = actual_error_list.clone();

        let exists_context = mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);
        let is_secure_context = mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_once(|_| Ok(()));

        let set_permissions_context = mock_filesystem::set_permissions_context();
        set_permissions_context
            .expect()
            .with(predicate::eq(path.clone()), predicate::eq(0o700))
            .return_once(|_, _| Ok(()));

        let rm_dir_context = mock_filesystem::remove_dir_all_context();
        rm_dir_context
            .expect()
            .with(predicate::eq(path.clone()))
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

        let directory = Directory::new(path);
        assert!(directory.is_ok());
        drop(directory);

        let result = actual_error_list.lock().unwrap();
        assert!(matches!(
            result.first().unwrap(),
            Err(FileSystemError::RemoveDirectory(msg,_)) if msg == &OsString::from_str("Could not remove directory").unwrap()));
    }

    // [utest->swdd~agent-rejects-insecure-reused-run-folder-paths~1]
    #[test]
    fn utest_directory_new_reuse_existing_secure_ok() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().to_path_buf();

        let is_secure_context = mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_const(true);

        let rm_dir_context = mock_filesystem::remove_dir_all_context();
        rm_dir_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_once(|_| Ok(()));

        let directory = Directory::new(path.clone());
        assert!(directory.is_ok());
        assert_eq!(path, directory.unwrap().get_path());
    }

    #[test]
    fn utest_directory_new_reuse_existing_insecure_rejected() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().to_path_buf();

        let is_secure_context = mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(predicate::eq(path.clone()))
            .return_const(false);

        let directory = Directory::new(path.clone());
        assert_eq!(
            directory.unwrap_err(),
            FileSystemError::InsecurePermissions(path.into_os_string())
        );
    }

    #[test]
    fn utest_directory_new_insecure_existing_parent_rejected() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();
        let tempdir = tempfile::tempdir().unwrap();
        let parent = tempdir.path().to_path_buf();
        let path = parent.join("child_dir");

        let exists_context = mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(true);

        let is_secure_context = mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(predicate::eq(parent.clone()))
            .return_const(false);

        let directory = Directory::new(path);
        assert_eq!(
            directory.unwrap_err(),
            FileSystemError::InsecurePermissions(parent.into_os_string())
        );
    }
}

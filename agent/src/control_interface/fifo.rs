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

use crate::io_utils::FileSystemError;
#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::filesystem;

#[derive(Debug)]
pub struct Fifo {
    path: PathBuf,
}

impl Fifo {
    pub fn new(path: PathBuf) -> Result<Self, FileSystemError> {
        if filesystem::is_fifo(&path) {
            log::trace!("Reusing existing fifo file '{path:?}'");
            Ok(Fifo { path })
        } else {
            match filesystem::make_fifo(&path) {
                Ok(_) => Ok(Fifo { path }),
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
        if let Err(err) = filesystem::remove_fifo(&self.path) {
            log::error!("{err}");
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
    use super::{Fifo, FileSystemError};
    use crate::{io_utils::mock_filesystem, test_helper::MOCKALL_CONTEXT_SYNC};

    use mockall::predicate;
    use std::{io::ErrorKind, path::Path};

    #[test]
    fn utest_fifo_reuse_existing_and_remove_ok() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();

        let is_fifo_context = mock_filesystem::is_fifo_context();
        is_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(true);

        let mk_fifo_context = mock_filesystem::make_fifo_context();
        mk_fifo_context.expect().never();

        let rm_fifo_context = mock_filesystem::remove_fifo_context();
        rm_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));

        assert!(Fifo::new(test_path_buffer).is_ok());
    }

    #[test]
    fn utest_fifo_new_create_and_remove_ok() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();

        let is_fifo_context = mock_filesystem::is_fifo_context();
        is_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);

        let mk_fifo_context = mock_filesystem::make_fifo_context();
        mk_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));

        let rm_fifo_context = mock_filesystem::remove_fifo_context();
        rm_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));

        assert!(Fifo::new(test_path_buffer).is_ok());
    }

    #[test]
    fn utest_fifo_new_create_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();

        let is_fifo_context = mock_filesystem::is_fifo_context();
        is_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);

        let mk_fifo_context = mock_filesystem::make_fifo_context();
        mk_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo")))
            .times(1)
            .return_once(|path| {
                Err(FileSystemError::CreateFifo(
                    path.to_path_buf().into_os_string(),
                    nix::errno::Errno::EACCES,
                ))
            });

        let rm_fifo_context = mock_filesystem::remove_fifo_context();
        rm_fifo_context.expect().never();

        assert!(matches!(
            Fifo::new(test_path_buffer),
            Err(FileSystemError::CreateFifo(_, nix::errno::Errno::EACCES,))
        ));
    }
    #[test]
    fn utest_fifo_drop_remove_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();

        let is_fifo_context = mock_filesystem::is_fifo_context();
        is_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);

        let mk_fifo_context = mock_filesystem::make_fifo_context();
        mk_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo")))
            .times(1)
            .return_once(|_| Ok(()));

        let rm_fifo_context = mock_filesystem::remove_fifo_context();
        rm_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo")))
            .times(1)
            .return_once(|path| {
                Err(FileSystemError::RemoveFifo(
                    path.to_path_buf().into_os_string(),
                    ErrorKind::Other,
                ))
            });

        assert!(Fifo::new(test_path_buffer).is_ok());
    }

    #[test]
    fn utest_fifo_get_path() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let test_path_buffer = Path::new("test_fifo").to_path_buf();

        let is_fifo_context = mock_filesystem::is_fifo_context();
        is_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_const(false);

        let mk_fifo_context = mock_filesystem::make_fifo_context();
        mk_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));

        let rm_fifo_context = mock_filesystem::remove_fifo_context();
        rm_fifo_context
            .expect()
            .with(predicate::eq(Path::new("test_fifo").to_path_buf()))
            .times(1)
            .return_once(|_| Ok(()));

        let fifo = Fifo::new(test_path_buffer);
        assert!(fifo.is_ok());
        assert_eq!(
            &Path::new("test_fifo").to_path_buf(),
            fifo.unwrap().get_path()
        );
    }
}

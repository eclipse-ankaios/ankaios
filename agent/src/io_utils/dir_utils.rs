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

use std::path::Path;

use crate::io_utils::FileSystemError;
#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::{filesystem, Directory};

use super::DEFAULT_RUN_FOLDER;
const RUNFOLDER_SUFFIX: &str = "_io";

// [impl->swdd~agent-prepares-dedicated-run-folder~1]
pub fn prepare_agent_run_directory(
    run_folder: &str,
    agent_name: &str,
) -> Result<Directory, FileSystemError> {
    let base_path = Path::new(run_folder);
    let agent_run_folder = base_path.join(format!("{}{}", agent_name, RUNFOLDER_SUFFIX));

    // If the default base dir is used, we need to take care of its creation
    if !filesystem::exists(base_path) {
        if Some(DEFAULT_RUN_FOLDER) == base_path.to_str() {
            filesystem::make_dir(base_path)?;

            filesystem::set_permissions(base_path, 0o777)?;
        } else {
            return Err(FileSystemError::NotFoundDirectory(base_path.into()));
        }
    }

    Directory::new(agent_run_folder)
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
    use super::{FileSystemError, Path, DEFAULT_RUN_FOLDER};
    use crate::io_utils::generate_test_directory_mock;
    use crate::io_utils::mock_filesystem;
    use crate::io_utils::prepare_agent_run_directory;

    use mockall::predicate;

    // [utest->swdd~agent-prepares-dedicated-run-folder~1]
    #[test]
    fn utest_arguments_get_run_directory_use_default_directory_create() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let agent_name = "test_agent_name";
        let run_folder = DEFAULT_RUN_FOLDER;

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_const(false);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_once(|_| Ok(()));

        let set_permissions_mock_context = mock_filesystem::set_permissions_context();
        set_permissions_mock_context
            .expect()
            .with(
                predicate::eq(Path::new(run_folder).to_path_buf()),
                predicate::eq(0o777),
            )
            .return_once(|_, _| Ok(()));

        let _directory_mock_context =
            generate_test_directory_mock(DEFAULT_RUN_FOLDER, "test_agent_name_io");

        assert!(prepare_agent_run_directory(run_folder, agent_name).is_ok());
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~1]
    #[test]
    fn utest_arguments_get_run_directory_use_default_directory_create_fails() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let agent_name = "test_agent_name";
        let run_folder = DEFAULT_RUN_FOLDER;

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_const(false);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_once(|_| {
                Err(FileSystemError::CreateDirectory(
                    Path::new("/tmp/x").as_os_str().to_os_string(),
                    std::io::ErrorKind::Other,
                ))
            });

        let set_permissions_mock_context = mock_filesystem::set_permissions_context();
        set_permissions_mock_context.expect().never();

        assert_eq!(
            prepare_agent_run_directory(run_folder, agent_name),
            Err(FileSystemError::CreateDirectory(
                Path::new("/tmp/x").as_os_str().to_os_string(),
                std::io::ErrorKind::Other
            ))
        );
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~1]
    #[test]
    fn utest_arguments_get_run_directory_use_default_directory_create_permissions_fail() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let agent_name = "test_agent_name";
        let run_folder = DEFAULT_RUN_FOLDER;

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_const(false);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_once(|_| Ok(()));

        let set_permissions_mock_context = mock_filesystem::set_permissions_context();
        set_permissions_mock_context
            .expect()
            .with(
                predicate::eq(Path::new(run_folder).to_path_buf()),
                predicate::eq(0o777),
            )
            .return_once(|_, _| {
                Err(FileSystemError::Permissions(
                    Path::new("/tmp/x").as_os_str().to_os_string(),
                    std::io::ErrorKind::Other,
                ))
            });

        assert_eq!(
            prepare_agent_run_directory(run_folder, agent_name),
            Err(FileSystemError::Permissions(
                Path::new("/tmp/x").as_os_str().to_os_string(),
                std::io::ErrorKind::Other,
            ))
        );
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~1]
    #[test]
    fn utest_arguments_get_run_directory_use_default_directory_exists() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let agent_name = "test_agent_name";
        let run_folder = DEFAULT_RUN_FOLDER;

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_const(true);

        let _directory_mock_context =
            generate_test_directory_mock(run_folder, "test_agent_name_io");

        assert!(prepare_agent_run_directory(run_folder, agent_name).is_ok());
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~1]
    #[test]
    fn utest_arguments_get_run_directory_given_directory_not_found() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let agent_name = "test_agent_name";
        let run_folder = "/tmp/x";

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_const(false);

        assert_eq!(
            prepare_agent_run_directory(run_folder, agent_name),
            Err(FileSystemError::NotFoundDirectory(
                Path::new(run_folder).as_os_str().to_os_string()
            ))
        );
    }
}

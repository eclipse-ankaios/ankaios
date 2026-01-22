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

use crate::io_utils::FileSystemError;
#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::{Directory, filesystem};

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const RUNFOLDER_SUFFIX: &str = "_io";

// Cache the default run folder to avoid inconsistent behavior if $TMPDIR is changed at runtime.
static DEFAULT_RUN_FOLDER: OnceLock<PathBuf> = OnceLock::new();

pub fn default_run_folder() -> PathBuf {
    DEFAULT_RUN_FOLDER
        .get_or_init(|| std::env::temp_dir().join("ankaios"))
        .clone()
}

pub fn default_run_folder_string() -> String {
    default_run_folder().to_string_lossy().to_string()
}

// [impl->swdd~agent-prepares-dedicated-run-folder~2]
pub fn prepare_agent_run_directory(
    run_folder: &str,
    agent_name: &str,
) -> Result<Directory, FileSystemError> {
    let base_path = Path::new(run_folder);
    let agent_run_folder = base_path.join(format!("{agent_name}{RUNFOLDER_SUFFIX}"));
    let default_base_path = default_run_folder();

    // If the default base dir is used, we need to take care of its creation
    if !filesystem::exists(base_path) {
        if base_path == default_base_path.as_path() {
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
    use super::{FileSystemError, Path};
    use crate::io_utils::{
        default_run_folder_string, generate_test_directory_mock, mock_filesystem,
        prepare_agent_run_directory,
    };
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;
    use ankaios_api::test_utils::fixtures;

    use mockall::predicate;

    // [utest->swdd~agent-prepares-dedicated-run-folder~2]
    #[test]
    fn utest_arguments_prepare_agent_run_directory_use_default_directory_create() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let run_folder = default_run_folder_string();

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(&run_folder).to_path_buf()))
            .return_const(false);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(Path::new(&run_folder).to_path_buf()))
            .return_once(|_| Ok(()));

        let set_permissions_mock_context = mock_filesystem::set_permissions_context();
        set_permissions_mock_context
            .expect()
            .with(
                predicate::eq(Path::new(&run_folder).to_path_buf()),
                predicate::eq(0o777),
            )
            .return_once(|_, _| Ok(()));

        let _directory_mock_context = generate_test_directory_mock(
            run_folder.as_str(),
            format!("{}_io", fixtures::AGENT_NAMES[0]).as_str(),
        );

        assert!(prepare_agent_run_directory(&run_folder, fixtures::AGENT_NAMES[0]).is_ok());
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~2]
    #[test]
    fn utest_arguments_prepare_agent_run_directory_use_default_directory_create_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let run_folder = default_run_folder_string();

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(&run_folder).to_path_buf()))
            .return_const(false);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(Path::new(&run_folder).to_path_buf()))
            .return_once(|_| {
                Err(FileSystemError::CreateDirectory(
                    Path::new("test_dir").as_os_str().to_os_string(),
                    std::io::ErrorKind::Other,
                ))
            });

        let set_permissions_mock_context = mock_filesystem::set_permissions_context();
        set_permissions_mock_context.expect().never();

        assert_eq!(
            prepare_agent_run_directory(&run_folder, fixtures::AGENT_NAMES[0]),
            Err(FileSystemError::CreateDirectory(
                Path::new("test_dir").as_os_str().to_os_string(),
                std::io::ErrorKind::Other
            ))
        );
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~2]
    #[test]
    fn utest_arguments_prepare_agent_run_directory_use_default_directory_create_permissions_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let run_folder = default_run_folder_string();

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(&run_folder).to_path_buf()))
            .return_const(false);

        let mk_dir_context = mock_filesystem::make_dir_context();
        mk_dir_context
            .expect()
            .with(predicate::eq(Path::new(&run_folder).to_path_buf()))
            .return_once(|_| Ok(()));

        let set_permissions_mock_context = mock_filesystem::set_permissions_context();
        set_permissions_mock_context
            .expect()
            .with(
                predicate::eq(Path::new(&run_folder).to_path_buf()),
                predicate::eq(0o777),
            )
            .return_once(|_, _| {
                Err(FileSystemError::Permissions(
                    Path::new("test_dir").as_os_str().to_os_string(),
                    std::io::ErrorKind::Other,
                ))
            });

        assert_eq!(
            prepare_agent_run_directory(&run_folder, fixtures::AGENT_NAMES[0]),
            Err(FileSystemError::Permissions(
                Path::new("test_dir").as_os_str().to_os_string(),
                std::io::ErrorKind::Other,
            ))
        );
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~2]
    #[test]
    fn utest_arguments_prepare_agent_run_directory_use_default_directory_exists() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let run_folder = default_run_folder_string();

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(&run_folder).to_path_buf()))
            .return_const(true);

        let _directory_mock_context = generate_test_directory_mock(
            run_folder.as_str(),
            format!("{}_io", fixtures::AGENT_NAMES[0]).as_str(),
        );

        assert!(prepare_agent_run_directory(&run_folder, fixtures::AGENT_NAMES[0]).is_ok());
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~2]
    #[test]
    fn utest_arguments_prepare_agent_run_directory_given_directory_not_found() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock();

        let run_folder = "/some_temp/non_existing_directory";

        let exists_mock_context = mock_filesystem::exists_context();
        exists_mock_context
            .expect()
            .with(predicate::eq(Path::new(run_folder).to_path_buf()))
            .return_const(false);

        assert_eq!(
            prepare_agent_run_directory(run_folder, fixtures::AGENT_NAMES[0]),
            Err(FileSystemError::NotFoundDirectory(
                Path::new(run_folder).as_os_str().to_os_string()
            ))
        );
    }
}

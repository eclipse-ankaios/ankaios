// Copyright (c) 2026 Elektrobit Automotive GmbH
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

use common::communications_error::CommunicationMiddlewareError;

#[cfg(test)]
use self::tests::shim::{Group, chown, fs};
#[cfg(not(test))]
use nix::unistd::{Group, chown};
#[cfg(not(test))]
use std::fs;
#[cfg(not(test))]
use std::os::unix::fs::FileTypeExt;
#[cfg(not(test))]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::net::UnixListener;

pub(crate) fn prepare_unix_listener(
    socket_path: &Path,
    socket_group: Option<&str>,
) -> Result<UnixListener, CommunicationMiddlewareError> {
    prepare_socket_path(socket_path, socket_path.exists())?;

    let listener = UnixListener::bind(socket_path).map_err(|err| {
        CommunicationMiddlewareError(format!(
            "Could not bind unix socket '{}': {err}",
            socket_path.display()
        ))
    })?;

    configure_socket_group(socket_path, socket_group)?;

    Ok(listener)
}

fn prepare_socket_path(
    socket_path: &Path,
    path_exists: bool,
) -> Result<(), CommunicationMiddlewareError> {
    if !path_exists {
        return Ok(());
    }

    let metadata = fs::metadata(socket_path).map_err(|err| {
        CommunicationMiddlewareError(format!(
            "Could not access existing unix socket path '{}': {err}",
            socket_path.display()
        ))
    })?;

    if metadata.file_type().is_socket() {
        fs::remove_file(socket_path).map_err(|err| {
            CommunicationMiddlewareError(format!(
                "Could not remove stale unix socket '{}': {err}",
                socket_path.display()
            ))
        })?;
    } else {
        return Err(CommunicationMiddlewareError(format!(
            "Unix socket path '{}' exists and is not a socket file",
            socket_path.display()
        )));
    }

    Ok(())
}

fn configure_socket_group(
    socket_path: &Path,
    socket_group: Option<&str>,
) -> Result<(), CommunicationMiddlewareError> {
    if let Some(group_name) = socket_group {
        // [impl->swdd~server-configures-unix-domain-socket-group~1]
        let group = Group::from_name(group_name)
            .map_err(|err| {
                CommunicationMiddlewareError(format!(
                    "Could not resolve unix socket group '{}': {err}",
                    group_name
                ))
            })?
            .ok_or_else(|| {
                CommunicationMiddlewareError(format!(
                    "Could not resolve unix socket group '{}': group does not exist",
                    group_name
                ))
            })?;

        chown(socket_path, None, Some(group.gid)).map_err(|err| {
            CommunicationMiddlewareError(format!(
                "Could not set unix socket group '{}' on '{}': {err}",
                group_name,
                socket_path.display()
            ))
        })?;

        fs::set_permissions(socket_path, fs::Permissions::from_mode(0o660)).map_err(|err| {
            CommunicationMiddlewareError(format!(
                "Could not set unix socket permissions on '{}': {err}",
                socket_path.display()
            ))
        })?;
    }

    Ok(())
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
    use self::shim::{GROUP_GID, GROUP_NAME};
    use super::configure_socket_group;
    use super::prepare_socket_path;
    use nix::errno::Errno;
    use nix::unistd::Gid;
    use std::io;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn socket_path() -> PathBuf {
        PathBuf::from(shim::SHARED_SOCKET_PATH)
    }

    fn lock_shim() -> std::sync::MutexGuard<'static, ()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test lock poisoned")
    }

    // [utest->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
    #[test]
    fn utest_prepare_unix_listener_skips_existing_path_checks_when_path_is_missing() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();

        let result = prepare_socket_path(&socket_path, false);

        assert!(result.is_ok());
        shim::assert_calls(&[]);
        shim::reset_overrides();
    }

    // [utest->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
    #[test]
    fn utest_prepare_unix_listener_replaces_stale_socket_file() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();
        shim::expect_metadata_once(&socket_path, Ok(shim::fs::Metadata::socket()));
        shim::expect_remove_file_once(&socket_path, Ok(()));

        let result = prepare_socket_path(&socket_path, true);

        assert!(result.is_ok());
        shim::assert_calls(&["metadata", "remove_file"]);
        shim::reset_overrides();
    }

    // [utest->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
    #[test]
    fn utest_prepare_unix_listener_fails_when_removing_stale_socket_fails() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();
        shim::expect_metadata_once(&socket_path, Ok(shim::fs::Metadata::socket()));

        shim::expect_remove_file_once(&socket_path, Err(io::Error::other("remove failed")));

        let result = prepare_socket_path(&socket_path, true);

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .0
                .contains("Could not remove stale unix socket")
        );
        shim::assert_calls(&["metadata", "remove_file"]);

        shim::reset_overrides();
    }

    // [utest->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
    #[test]
    fn utest_prepare_unix_listener_fails_if_path_exists_and_is_not_socket() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let file_path = socket_path();
        shim::expect_metadata_once(&file_path, Ok(shim::fs::Metadata::not_socket()));

        let result = prepare_socket_path(&file_path, true);

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .0
                .contains("exists and is not a socket file")
        );
        shim::assert_calls(&["metadata"]);

        shim::reset_overrides();
    }

    // [utest->swdd~grpc-server-supports-unix-domain-socket-endpoints~1]
    #[test]
    fn utest_prepare_unix_listener_fails_when_existing_path_metadata_fails() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();

        shim::expect_metadata_once(
            &socket_path,
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
        );

        let result = prepare_socket_path(&socket_path, true);

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .0
                .contains("Could not access existing unix socket path")
        );
        shim::assert_calls(&["metadata"]);

        shim::reset_overrides();
    }

    // [utest->swdd~server-configures-unix-domain-socket-group~1]
    #[test]
    fn utest_prepare_unix_listener_skips_group_configuration_without_group() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();

        let result = configure_socket_group(&socket_path, None);

        assert!(result.is_ok());
        shim::assert_calls(&[]);

        shim::reset_overrides();
    }

    // [utest->swdd~server-configures-unix-domain-socket-group~1]
    #[test]
    fn utest_prepare_unix_listener_fails_for_unknown_group() {
        let _lock = lock_shim();
        shim::reset_overrides();
        let socket_path = socket_path();
        shim::expect_group_from_name_once(GROUP_NAME, Ok(None));

        let result = configure_socket_group(&socket_path, Some(GROUP_NAME));

        assert!(result.is_err());
        assert!(result.err().unwrap().0.contains("group does not exist"));
        shim::assert_calls(&["group_from_name"]);

        shim::reset_overrides();
    }

    // [utest->swdd~server-configures-unix-domain-socket-group~1]
    #[test]
    fn utest_prepare_unix_listener_configures_group_permissions() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();
        let gid = Gid::from_raw(GROUP_GID);

        shim::expect_group_from_name_once(GROUP_NAME, Ok(Some(shim::Group { gid })));
        shim::expect_chown_once(&socket_path, gid, Ok(()));
        shim::expect_set_permissions_once(&socket_path, Ok(()));

        let result = configure_socket_group(&socket_path, Some(GROUP_NAME));

        assert!(result.is_ok());
        shim::assert_calls(&["group_from_name", "chown", "set_permissions"]);

        shim::reset_overrides();
    }

    // [utest->swdd~server-configures-unix-domain-socket-group~1]
    #[test]
    fn utest_prepare_unix_listener_fails_when_chown_fails() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();
        let gid = Gid::from_raw(GROUP_GID);

        shim::expect_group_from_name_once(GROUP_NAME, Ok(Some(shim::Group { gid })));
        shim::expect_chown_once(&socket_path, gid, Err(Errno::EPERM));

        let result = configure_socket_group(&socket_path, Some(GROUP_NAME));

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .0
                .contains("Could not set unix socket group")
        );
        shim::assert_calls(&["group_from_name", "chown"]);

        shim::reset_overrides();
    }

    #[test]
    fn utest_prepare_unix_listener_fails_when_group_lookup_fails() {
        let _lock = lock_shim();
        shim::reset_overrides();
        let socket_path = socket_path();
        shim::expect_group_from_name_once(GROUP_NAME, Err(Errno::EINVAL));

        let result = configure_socket_group(&socket_path, Some(GROUP_NAME));

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .0
                .contains("Could not resolve unix socket group")
        );
        shim::assert_calls(&["group_from_name"]);

        shim::reset_overrides();
    }

    // [utest->swdd~server-configures-unix-domain-socket-group~1]
    #[test]
    fn utest_prepare_unix_listener_fails_when_setting_permissions_fails() {
        let _lock = lock_shim();
        shim::reset_overrides();

        let socket_path = socket_path();
        let gid = Gid::from_raw(GROUP_GID);

        shim::expect_group_from_name_once(GROUP_NAME, Ok(Some(shim::Group { gid })));
        shim::expect_chown_once(&socket_path, gid, Ok(()));
        shim::expect_set_permissions_once(
            &socket_path,
            Err(io::Error::other("set permissions failed")),
        );

        let result = configure_socket_group(&socket_path, Some(GROUP_NAME));

        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .0
                .contains("Could not set unix socket permissions")
        );
        shim::assert_calls(&["group_from_name", "chown", "set_permissions"]);

        shim::reset_overrides();
    }

    pub(crate) mod shim {
        use nix::unistd::Gid;
        use std::io;
        use std::path::Path;
        use std::sync::{Mutex, MutexGuard, OnceLock};

        pub(crate) const SHARED_SOCKET_PATH: &str = "/unit-test/shared.sock";
        pub(crate) const GROUP_NAME: &str = "ankaios";
        pub(crate) const GROUP_GID: u32 = 1000;

        struct Ops {
            metadata: Option<io::Result<fs::Metadata>>,
            remove_file: Option<io::Result<()>>,
            set_permissions: Option<io::Result<()>>,
            group_from_name: Option<Result<Option<Group>, nix::Error>>,
            chown: Option<Result<(), nix::Error>>,
            calls: Vec<&'static str>,
        }

        fn ops_store() -> MutexGuard<'static, Ops> {
            static STORE: OnceLock<Mutex<Ops>> = OnceLock::new();
            STORE
                .get_or_init(|| {
                    Mutex::new(Ops {
                        metadata: None,
                        remove_file: None,
                        set_permissions: None,
                        group_from_name: None,
                        chown: None,
                        calls: Vec::new(),
                    })
                })
                .lock()
                .expect("test shim lock poisoned")
        }

        pub(crate) fn reset_overrides() {
            let mut ops = ops_store();
            *ops = Ops {
                metadata: None,
                remove_file: None,
                set_permissions: None,
                group_from_name: None,
                chown: None,
                calls: Vec::new(),
            };
        }

        pub(crate) fn assert_calls(expected: &[&'static str]) {
            let ops = ops_store();
            assert_eq!(ops.calls.as_slice(), expected, "unexpected call sequence");
        }

        pub(crate) fn expect_metadata_once(path: &Path, result: io::Result<fs::Metadata>) {
            assert_eq!(
                path,
                Path::new(SHARED_SOCKET_PATH),
                "unexpected metadata path"
            );
            ops_store().metadata = Some(result);
        }

        pub(crate) fn expect_remove_file_once(path: &Path, result: io::Result<()>) {
            assert_eq!(
                path,
                Path::new(SHARED_SOCKET_PATH),
                "unexpected remove_file path"
            );
            ops_store().remove_file = Some(result);
        }

        pub(crate) fn expect_set_permissions_once(path: &Path, result: io::Result<()>) {
            assert_eq!(
                path,
                Path::new(SHARED_SOCKET_PATH),
                "unexpected set_permissions path"
            );
            ops_store().set_permissions = Some(result);
        }

        pub(crate) fn expect_group_from_name_once(
            name: &str,
            result: Result<Option<Group>, nix::Error>,
        ) {
            assert_eq!(name, GROUP_NAME, "unexpected group name lookup");
            ops_store().group_from_name = Some(result);
        }

        pub(crate) fn expect_chown_once(path: &Path, gid: Gid, result: Result<(), nix::Error>) {
            assert_eq!(path, Path::new(SHARED_SOCKET_PATH), "unexpected chown path");
            assert_eq!(gid, Gid::from_raw(GROUP_GID), "unexpected chown gid");
            ops_store().chown = Some(result);
        }

        pub(crate) mod fs {
            use super::{Path, SHARED_SOCKET_PATH, ops_store};
            use std::io;

            #[derive(Clone, Copy)]
            pub struct Permissions {
                pub mode: u32,
            }

            impl Permissions {
                pub fn from_mode(mode: u32) -> Self {
                    Self { mode }
                }
            }

            #[derive(Clone, Copy)]
            pub struct FileType {
                pub is_socket: bool,
            }

            impl FileType {
                pub fn is_socket(&self) -> bool {
                    self.is_socket
                }
            }

            #[derive(Clone, Copy)]
            pub struct Metadata {
                pub is_socket: bool,
            }

            impl Metadata {
                pub fn socket() -> Self {
                    Self { is_socket: true }
                }

                pub fn not_socket() -> Self {
                    Self { is_socket: false }
                }

                pub fn file_type(&self) -> FileType {
                    FileType {
                        is_socket: self.is_socket,
                    }
                }
            }

            pub fn metadata(path: &Path) -> io::Result<Metadata> {
                let mut ops = ops_store();
                ops.calls.push("metadata");
                assert_eq!(
                    path,
                    Path::new(SHARED_SOCKET_PATH),
                    "unexpected metadata path"
                );
                if let Some(result) = ops.metadata.take() {
                    return result;
                }
                panic!("missing metadata expectation for '{}'", path.display());
            }

            pub fn remove_file(path: &Path) -> io::Result<()> {
                let mut ops = ops_store();
                ops.calls.push("remove_file");
                assert_eq!(
                    path,
                    Path::new(SHARED_SOCKET_PATH),
                    "unexpected remove_file path"
                );
                if let Some(result) = ops.remove_file.take() {
                    return result;
                }
                panic!("missing remove_file expectation for '{}'", path.display());
            }

            pub fn set_permissions(path: &Path, perms: Permissions) -> io::Result<()> {
                let mut ops = ops_store();
                ops.calls.push("set_permissions");
                assert_eq!(perms.mode, 0o660, "unexpected permission mode");
                assert_eq!(
                    path,
                    Path::new(SHARED_SOCKET_PATH),
                    "unexpected set_permissions path"
                );
                if let Some(result) = ops.set_permissions.take() {
                    return result;
                }
                panic!(
                    "missing set_permissions expectation for '{}'",
                    path.display()
                );
            }
        }

        #[derive(Clone, Copy)]
        pub(crate) struct Group {
            pub gid: Gid,
        }

        impl Group {
            pub fn from_name(group_name: &str) -> Result<Option<Group>, nix::Error> {
                let mut ops = ops_store();
                ops.calls.push("group_from_name");
                assert_eq!(group_name, GROUP_NAME, "unexpected group name lookup");
                if let Some(result) = ops.group_from_name.take() {
                    return result;
                }
                panic!("missing group lookup expectation for '{group_name}'");
            }
        }

        pub fn chown(
            path: &Path,
            _uid: Option<nix::unistd::Uid>,
            gid: Option<Gid>,
        ) -> Result<(), nix::Error> {
            let gid = gid.expect("gid is required for test shim chown");
            let mut ops = ops_store();
            ops.calls.push("chown");
            if let Some(result) = ops.chown.take() {
                assert_eq!(path, Path::new(SHARED_SOCKET_PATH), "unexpected chown path");
                assert_eq!(gid, Gid::from_raw(GROUP_GID), "unexpected chown gid");
                return result;
            }
            panic!("missing chown expectation for '{}'", path.display());
        }
    }
}

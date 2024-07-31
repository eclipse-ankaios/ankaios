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

pub mod ankaios_streaming {
    use tonic::async_trait;

    #[async_trait]
    pub trait GRPCStreaming<T> {
        async fn message(&mut self) -> Result<Option<T>, tonic::Status>;
    }
}

pub mod grpc_middleware_error;

pub mod security {
    use crate::grpc_middleware_error::GrpcMiddlewareError;
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    #[derive(Debug, Default, Clone)]
    pub struct TLSConfig {
        pub path_to_ca_pem: String,
        pub path_to_crt_pem: String,
        pub path_to_key_pem: String,
    }

    impl TLSConfig {
        pub fn is_config_conflicting(
            insecure: bool,
            ca_pem: &Option<String>,
            crt_pem: &Option<String>,
            key_pem: &Option<String>,
        ) -> Result<(), String> {
            if insecure && (ca_pem.is_some() || crt_pem.is_some() || key_pem.is_some()) {
                return Err("Insecure and secure flags specified at the same time. Defaulting to secure communication.".to_string());
            }
            Ok(())
        }

        pub fn new(
            insecure: bool,
            ca_pem: Option<String>,
            crt_pem: Option<String>,
            key_pem: Option<String>,
        ) -> Result<Option<TLSConfig>, String> {
            match (insecure, ca_pem, crt_pem, key_pem) {
                // [impl->swdd~cli-provides-file-paths-to-communication-middleware~1]
                (_, Some(path_to_ca_pem), Some(path_to_crt_pem), Some(path_to_key_pem)) => {

                    Ok(Some(TLSConfig {
                        path_to_ca_pem,
                        path_to_crt_pem,
                        path_to_key_pem,
                    }))
                }
                // [impl->swdd~cli-establishes-insecure-communication-based-on-provided-insecure-cli-argument~1]
                (true, None, None, None) => Ok(None),
                // [impl->swdd~cli-fails-on-missing-file-paths-and-insecure-cli-arguments~1]
                (_, ca_pem, crt_pem, key_pem) => Err(format!(
                    "Either provide mTLS config via the '--ca_pem {}', '--crt_pem {}' and '--key_pem {}' options or deactivate mTLS with the '--insecure' option!",
                    ca_pem.unwrap_or(String::from("\"\"")),
                    crt_pem.unwrap_or(String::from("\"\"")),
                    key_pem.unwrap_or(String::from("\"\""))
                )),
            }
        }
    }

    // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
    pub fn read_pem_file(
        path_of_pem_file: &Path,
        check_permissions: bool,
    ) -> Result<String, GrpcMiddlewareError> {
        let mut file = File::open(path_of_pem_file).map_err(|err| {
            GrpcMiddlewareError::CertificateError(format!(
                "Error during opening the given file {:?}: {}",
                path_of_pem_file, err
            ))
        })?;

        let mut owner_readable = true;
        let mut group_not_readable = true;
        let mut others_not_readable = true;

        if check_permissions {
            let permissions = file
                .metadata()
                .map_err(|err| {
                    GrpcMiddlewareError::CertificateError(format!(
                        "Error during getting permissions of the given file {:?}: {}",
                        path_of_pem_file, err
                    ))
                })?
                .permissions();

            // [impl->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~1]
            owner_readable = (permissions.mode() & 0o400) == 0o400; // read for the owner
            group_not_readable = (permissions.mode() & 0o040) != 0o040; // not read for the group
            others_not_readable = (permissions.mode() & 0o004) != 0o004; // not read for others
        }

        if owner_readable && group_not_readable && others_not_readable {
            let mut buffer = String::new();
            file.read_to_string(&mut buffer).map_err(|err| {
                GrpcMiddlewareError::CertificateError(format!(
                    "Error during reading the given file {:?}: {:?}",
                    path_of_pem_file, err
                ))
            })?;
            Ok(buffer)
        } else {
            Err(GrpcMiddlewareError::CertificateError(format!(
                "The given certificate file {:?} has incorrect permissions!",
                path_of_pem_file
            )))
        }
    }
}

mod agent_senders_map;
pub mod client;
mod from_server_proxy;
mod grpc_agent_connection;
mod grpc_cli_connection;
pub mod server;
mod to_server_proxy;

use api::ank_base;
pub mod grpc_api;
pub use crate::grpc_api::*;

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::security::*;
    use crate::grpc_middleware_error::GrpcMiddlewareError;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::NamedTempFile;

    static TEST_PEM_CONTENT: &str = r#"-----BEGIN CERTIFICATE-----
MIIDrzCCAkGgAwIBAgIQBzANBgkqhkiG9w0BAQUFADCBiDELMAkGA1UEBhMCVVMx
...blabla
-----END CERTIFICATE-----"#;

    // [utest->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
    // [utest->swdd~grpc-supports-pem-file-format-for-keys~1]
    #[test]
    fn utest_read_pem_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        // Test with check_permissions set to false (no permission checks)
        let result = read_pem_file(temp_file.path(), false).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~1]
    #[test]
    fn test_read_pem_file_and_check_permissions() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let mut permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        // Test with check_permissions set to true and correct permissions (rw for owner, no rw for group and others)
        permissions.set_mode(0o600);
        let _ = temp_file.as_file_mut().set_permissions(permissions);
        let result = read_pem_file(temp_file.path(), true).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);

        // Test with check_permissions set to true and incorrect permissions (readable by groups and others)
        permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        permissions.set_mode(0o644);
        let _ = temp_file.as_file_mut().set_permissions(permissions);
        let error = read_pem_file(temp_file.path(), true).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }
}

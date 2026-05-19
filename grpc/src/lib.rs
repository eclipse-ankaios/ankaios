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
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    #[derive(Debug, Default, Clone)]
    pub struct TLSConfig {
        pub ca_pem: String,
        pub crt_pem: String,
        pub key_pem: String,
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
                (_, Some(ca_pem), Some(crt_pem), Some(key_pem)) => Ok(Some(TLSConfig {
                    ca_pem,
                    crt_pem,
                    key_pem,
                })),
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

    /// Type of PEM file being read, used to determine appropriate permission requirements
    #[derive(Debug, Clone, Copy)]
    pub enum PemFileType {
        PrivateKey,
        Certificate,
    }

    // [impl->swdd~grpc-supports-pem-file-format-for-X509-certificates~1]
    pub fn read_pem_file<S: AsRef<OsStr>>(
        pem_file_path: S,
        file_type: PemFileType,
    ) -> Result<String, GrpcMiddlewareError> {
        let path_of_pem_file = Path::new(&pem_file_path);

        let mut file = File::open(path_of_pem_file).map_err(|err| {
            GrpcMiddlewareError::CertificateError(format!(
                "Error during opening the given file {path_of_pem_file:?}: {err}"
            ))
        })?;

        let metadata = file.metadata().map_err(|err| {
            GrpcMiddlewareError::CertificateError(format!(
                "Error during getting permissions of the given file {path_of_pem_file:?}: {err}"
            ))
        })?;

        let permissions = metadata.permissions();
        let mode = permissions.mode() & 0o777;

        // [impl->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
        // Early exit if permissions are incorrect
        match file_type {
            PemFileType::PrivateKey => {
                // Private keys must be exactly 0600 or 0400
                if mode != 0o600 && mode != 0o400 {
                    return Err(GrpcMiddlewareError::CertificateError(format!(
                        "Private key file {path_of_pem_file:?} has insecure permissions {mode:o}. \
                         Private keys must have permissions 0600 or 0400. Use 'chmod 0600 <file>' to fix this."
                    )));
                }
            }
            PemFileType::Certificate => {
                // Certificates must be owner-readable, not writable by group or others, and not executable
                if (mode & 0o400) == 0 || (mode & 0o022) != 0 || (mode & 0o111) != 0 {
                    return Err(GrpcMiddlewareError::CertificateError(format!(
                        "Certificate file {path_of_pem_file:?} has insecure permissions {mode:o}. \
                         Certificates must have permissions 0600 or 0400. Use 'chmod 0600 <file>' to fix this."
                    )));
                }
            }
        }

        let mut buffer = String::new();
        file.read_to_string(&mut buffer).map_err(|err| {
            GrpcMiddlewareError::CertificateError(format!(
                "Error during reading the given file {path_of_pem_file:?}: {err:?}"
            ))
        })?;
        Ok(buffer)
    }
}

mod agent_senders_map;
pub mod client;
mod from_server_proxy;
mod grpc_agent_connection;
mod grpc_cli_connection;
pub mod server;
mod to_server_proxy;

use ankaios_api::ank_base;
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

        // Test reading certificate file with correct permissions (0644)
        let mut permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        permissions.set_mode(0o644);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let result = read_pem_file(temp_file.path(), PemFileType::Certificate).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_read_pem_file_and_check_permissions() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let mut permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        // Test private key with correct permissions (rw for owner only)
        permissions.set_mode(0o600);
        let _ = temp_file.as_file_mut().set_permissions(permissions);
        let result = read_pem_file(temp_file.path(), PemFileType::PrivateKey).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);

        // Test private key with incorrect permissions (readable by others)
        permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        permissions.set_mode(0o644);
        let _ = temp_file.as_file_mut().set_permissions(permissions);
        let error = read_pem_file(temp_file.path(), PemFileType::PrivateKey).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_private_key_fails_with_0777() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o777);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::PrivateKey).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_private_key_fails_with_0722() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o722);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::PrivateKey).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_private_key_fails_with_0610() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o610);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::PrivateKey).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_private_key_fails_with_0660() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o660);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::PrivateKey).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_private_key_fails_with_0602() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o602);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::PrivateKey).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_private_key_succeeds_with_0400() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o400);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let result = read_pem_file(temp_file.path(), PemFileType::PrivateKey).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_succeeds_with_0644() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o644);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let result = read_pem_file(temp_file.path(), PemFileType::Certificate).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_succeeds_with_0600() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o600);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let result = read_pem_file(temp_file.path(), PemFileType::Certificate).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_succeeds_with_0400() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o400);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let result = read_pem_file(temp_file.path(), PemFileType::Certificate).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_succeeds_with_0444() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o444);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let result = read_pem_file(temp_file.path(), PemFileType::Certificate).unwrap();
        assert_eq!(result, TEST_PEM_CONTENT);
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_fails_with_0666() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o666);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::Certificate).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_fails_with_0620() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o620);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::Certificate).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_fails_with_0602() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o602);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::Certificate).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_fails_with_0755() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o755);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::Certificate).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_fails_with_0744() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o744);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::Certificate).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_fails_with_0645() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o645);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::Certificate).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }

    // [utest->swdd~grpc-checks-given-PEM-file-for-proper-unix-file-permission~2]
    #[test]
    fn test_certificate_fails_with_0654() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(TEST_PEM_CONTENT.as_bytes()).unwrap();

        let permissions = temp_file.as_file_mut().metadata().unwrap().permissions();
        let mut permissions = permissions;
        permissions.set_mode(0o654);
        let _ = temp_file.as_file_mut().set_permissions(permissions);

        let error = read_pem_file(temp_file.path(), PemFileType::Certificate).err().unwrap();
        assert!(matches!(error, GrpcMiddlewareError::CertificateError(_)));
    }
}

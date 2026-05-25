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

// [impl->swdd~cli-generate-keypairs~1]

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use base64::{Engine as _, engine::general_purpose};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub struct KeygenCommand {
    output_path: String,
    force: bool,
}

impl KeygenCommand {
    pub fn new(output_path: String, force: bool) -> Self {
        Self { output_path, force }
    }

    pub async fn execute(&self) -> Result<(), String> {
        let private_key_path = &self.output_path;
        let public_key_path = format!("{}.pub", self.output_path);

        // Check if files exist (unless --force is specified)
        if !self.force {
            if Path::new(private_key_path).exists() {
                return Err(format!(
                    "Private key file already exists: {}. Use --force to overwrite",
                    private_key_path
                ));
            }
            if Path::new(&public_key_path).exists() {
                return Err(format!(
                    "Public key file already exists: {}. Use --force to overwrite",
                    public_key_path
                ));
            }
        }

        // Generate Ed25519 keypair
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key: VerifyingKey = signing_key.verifying_key();

        // Save private key with secure permissions (0600 - owner read/write only)
        let private_key_pem = self.encode_private_key_to_pem(&signing_key)?;
        self.write_private_key(private_key_path, &private_key_pem)?;

        println!("Private key written to: {}", private_key_path);

        // Save public key (standard permissions are fine for public keys)
        let public_key_pem = self.encode_public_key_to_pem(&verifying_key)?;
        fs::write(&public_key_path, public_key_pem)
            .map_err(|e| format!("Failed to write public key: {}", e))?;

        println!("Public key written to: {}", public_key_path);

        Ok(())
    }

    /// Write private key with secure permissions (0600 on Unix)
    fn write_private_key(&self, path: &str, content: &str) -> Result<(), String> {
        #[cfg(unix)]
        {
            // On Unix, create file with owner-only read/write permissions (0600)
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600) // Owner read/write only
                .open(path)
                .map_err(|e| format!("Failed to create private key file: {}", e))?;

            file.write_all(content.as_bytes())
                .map_err(|e| format!("Failed to write private key: {}", e))?;
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, use standard write with warning
            fs::write(path, content)
                .map_err(|e| format!("Failed to write private key: {}", e))?;
            eprintln!("WARNING: On non-Unix systems, private key file permissions may not be secure.");
            eprintln!("Please manually set appropriate permissions on: {}", path);
        }

        Ok(())
    }

    fn encode_private_key_to_pem(&self, signing_key: &SigningKey) -> Result<String, String> {
        // Encode Ed25519 private key in PKCS#8 PEM format
        let key_bytes = signing_key.to_bytes();

        // PKCS#8 wrapper for Ed25519 private key
        // This is a minimal PKCS#8 structure for Ed25519
        let mut pkcs8_bytes = vec![
            0x30, 0x2e, // SEQUENCE, length 46
            0x02, 0x01, 0x00, // INTEGER version (0)
            0x30, 0x05, // SEQUENCE, length 5
            0x06, 0x03, 0x2b, 0x65, 0x70, // OID for Ed25519
            0x04, 0x22, // OCTET STRING, length 34
            0x04, 0x20, // OCTET STRING, length 32 (the actual key)
        ];
        pkcs8_bytes.extend_from_slice(&key_bytes);

        let base64_content = general_purpose::STANDARD.encode(&pkcs8_bytes);

        // Format as PEM
        let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
        for chunk in base64_content.as_bytes().chunks(64) {
            pem.push_str(&String::from_utf8_lossy(chunk));
            pem.push('\n');
        }
        pem.push_str("-----END PRIVATE KEY-----\n");

        Ok(pem)
    }

    fn encode_public_key_to_pem(&self, verifying_key: &VerifyingKey) -> Result<String, String> {
        // Encode Ed25519 public key in SPKI PEM format
        let key_bytes = verifying_key.to_bytes();

        // SubjectPublicKeyInfo wrapper for Ed25519 public key
        let mut spki_bytes = vec![
            0x30, 0x2a, // SEQUENCE, length 42
            0x30, 0x05, // SEQUENCE, length 5
            0x06, 0x03, 0x2b, 0x65, 0x70, // OID for Ed25519
            0x03, 0x21, // BIT STRING, length 33
            0x00, // no unused bits
        ];
        spki_bytes.extend_from_slice(&key_bytes);

        let base64_content = general_purpose::STANDARD.encode(&spki_bytes);

        // Format as PEM
        let mut pem = String::from("-----BEGIN PUBLIC KEY-----\n");
        for chunk in base64_content.as_bytes().chunks(64) {
            pem.push_str(&String::from_utf8_lossy(chunk));
            pem.push('\n');
        }
        pem.push_str("-----END PUBLIC KEY-----\n");

        Ok(pem)
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
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    // [utest->swdd~cli-generate-keypairs~1]
    #[tokio::test]
    async fn utest_keygen_creates_keypair() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test-key");

        let keygen_cmd = KeygenCommand::new(output_path.to_str().unwrap().to_string(), true);
        let result = keygen_cmd.execute().await;

        assert!(result.is_ok(), "Keygen should succeed");
        assert!(output_path.exists(), "Private key file should exist");

        let public_key_path = format!("{}.pub", output_path.to_str().unwrap());
        assert!(
            Path::new(&public_key_path).exists(),
            "Public key file should exist"
        );
    }

    // [utest->swdd~cli-generate-keypairs~1]
    #[tokio::test]
    async fn utest_keygen_creates_valid_pem_format() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test-key");

        let keygen_cmd = KeygenCommand::new(output_path.to_str().unwrap().to_string(), true);
        keygen_cmd.execute().await.unwrap();

        let private_key_content = fs::read_to_string(&output_path).unwrap();
        assert!(
            private_key_content.starts_with("-----BEGIN PRIVATE KEY-----"),
            "Private key should be in PEM format"
        );
        assert!(
            private_key_content.contains("-----END PRIVATE KEY-----"),
            "Private key should have PEM footer"
        );

        let public_key_path = format!("{}.pub", output_path.to_str().unwrap());
        let public_key_content = fs::read_to_string(&public_key_path).unwrap();
        assert!(
            public_key_content.starts_with("-----BEGIN PUBLIC KEY-----"),
            "Public key should be in PEM format"
        );
        assert!(
            public_key_content.contains("-----END PUBLIC KEY-----"),
            "Public key should have PEM footer"
        );
    }

    // [utest->swdd~cli-generate-keypairs~1]
    #[test]
    fn utest_encode_private_key_to_pem() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        let keygen_cmd = KeygenCommand::new("dummy".to_string(), false);
        let pem = keygen_cmd.encode_private_key_to_pem(&signing_key);

        assert!(pem.is_ok(), "Should encode private key to PEM");
        let pem_str = pem.unwrap();
        assert!(
            pem_str.starts_with("-----BEGIN PRIVATE KEY-----"),
            "PEM should have correct header"
        );
    }

    // [utest->swdd~cli-generate-keypairs~1]
    #[test]
    fn utest_encode_public_key_to_pem() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        let keygen_cmd = KeygenCommand::new("dummy".to_string(), false);
        let pem = keygen_cmd.encode_public_key_to_pem(&verifying_key);

        assert!(pem.is_ok(), "Should encode public key to PEM");
        let pem_str = pem.unwrap();
        assert!(
            pem_str.starts_with("-----BEGIN PUBLIC KEY-----"),
            "PEM should have correct header"
        );
    }
}

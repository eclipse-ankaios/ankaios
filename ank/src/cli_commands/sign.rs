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

// [impl->swdd~cli-sign-workload-manifests~1]

use ankaios_api::ank_base::{CompleteState, SignatureMetadata, State, UpdateStateRequest};
use common::objects::canonical::Canonical;
use common::objects::signed_payload::SignedPayload;
use ed25519_dalek::{Signature, Signer, SigningKey};
use fs2::FileExt;
use prost::Message;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use base64::{Engine as _, engine::general_purpose};

pub struct SignCommand {
    input_yaml: String,
    key_id: String,
    private_key_path: String,
    counter: Option<u64>,
}

impl SignCommand {
    pub fn new(
        input_yaml: String,
        key_id: String,
        private_key_path: String,
        counter: Option<u64>,
    ) -> Self {
        Self {
            input_yaml,
            key_id,
            private_key_path,
            counter,
        }
    }

    pub async fn execute(&self) -> Result<(), String> {
        // 1. Parse YAML to State object
        let yaml_content = fs::read_to_string(&self.input_yaml)
            .map_err(|e| format!("Failed to read input file: {}", e))?;

        // Parse YAML to serde_yaml::Value first for validation
        let yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml_content)
            .map_err(|e| format!("Failed to parse YAML: {}", e))?;

        // Convert to State - we expect the YAML to have a State structure
        let state: State = serde_yaml::from_value(yaml_value)
            .map_err(|e| format!("Failed to convert to State: {}", e))?;

        // 2. Create canonical bytes
        let canonical = state
            .to_canonical_bytes()
            .map_err(|e| format!("Failed to canonicalize state: {}", e))?;

        // 3. Load private key
        let key_pem = fs::read_to_string(&self.private_key_path)
            .map_err(|e| format!("Failed to read private key: {}", e))?;
        let signing_key = self
            .parse_ed25519_private_key(&key_pem)
            .map_err(|e| format!("Failed to parse private key: {}", e))?;

        // 4. Create signed payload
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Failed to get system time: {}", e))?
            .as_secs();
        let counter_value = if let Some(c) = self.counter {
            c
        } else {
            self.load_next_counter()?
        };

        let payload = SignedPayload::new(counter_value, timestamp, canonical);
        let payload_bytes = payload
            .to_deterministic_bytes()
            .map_err(|e| format!("Failed to serialize payload: {}", e))?;

        // 5. Sign the payload
        let signature: Signature = signing_key.sign(&payload_bytes);

        // 6. Create UpdateStateRequest with signature metadata
        let request = UpdateStateRequest {
            new_state: Some(CompleteState {
                desired_state: Some(state),
                ..Default::default()
            }),
            update_mask: vec![],
            signature_metadata: Some(SignatureMetadata {
                signature: signature.to_bytes().to_vec(),
                key_id: self.key_id.clone(),
                counter: counter_value,
                timestamp,
            }),
        };

        // 7. Output as BINARY PROTOBUF (not YAML to avoid round-trip issues)
        let mut output = Vec::new();
        request
            .encode(&mut output)
            .map_err(|e| format!("Failed to encode protobuf: {}", e))?;

        // Write to file with .pb extension
        let input_path = std::path::Path::new(&self.input_yaml);
        let output_path = input_path.with_extension("pb");
        fs::write(&output_path, &output)
            .map_err(|e| format!("Failed to write output file: {}", e))?;

        println!("Signed manifest written to: {}", output_path.display());

        Ok(())
    }

    fn parse_ed25519_private_key(&self, pem_content: &str) -> Result<SigningKey, String> {
        // Parse PEM-formatted Ed25519 private key
        // Expected format: "-----BEGIN PRIVATE KEY-----" ... "-----END PRIVATE KEY-----"
        let pem_content = pem_content.trim();

        // Remove PEM headers/footers
        let base64_content = pem_content
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect::<String>();

        // Decode base64
        let key_bytes = general_purpose::STANDARD.decode(&base64_content)
            .map_err(|e| format!("Failed to decode base64: {}", e))?;

        // Ed25519 private keys in PKCS#8 format have a specific structure
        // We need to extract the actual 32-byte private key from the PKCS#8 wrapper
        if key_bytes.len() < 32 {
            return Err("Private key too short".to_string());
        }

        // For PKCS#8 format, the actual key is at the end
        let key_start = key_bytes.len() - 32;
        let private_key_bytes = &key_bytes[key_start..];

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(private_key_bytes);

        SigningKey::from_bytes(&key_array);
        Ok(SigningKey::from_bytes(&key_array))
    }

    /// Load and increment counter with file locking to prevent race conditions
    ///
    /// # Security
    /// Uses exclusive file locking to ensure the read-increment-write operation
    /// is atomic. Without locking, concurrent signing operations could read the
    /// same counter value, both increment it, and both write the same new value,
    /// breaking monotonic counter guarantees and enabling replay attacks.
    ///
    /// # Implementation
    /// 1. Open/create counter file with read+write access
    /// 2. Acquire exclusive lock (blocks other processes)
    /// 3. Read current counter value (or initialize to 0)
    /// 4. Increment counter
    /// 5. Write new value back to file
    /// 6. Release lock automatically on file close
    fn load_next_counter(&self) -> Result<u64, String> {
        let counter_file = format!(".ank-counter-{}", self.key_id);

        // Open file with read+write+create flags
        // This ensures we can both read the current value and write the new one
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&counter_file)
            .map_err(|e| format!("Failed to open counter file {}: {}", counter_file, e))?;

        // Acquire exclusive lock - blocks until no other process holds the lock
        // This prevents the race condition where two concurrent signs could
        // read the same counter, increment it, and both use the same value
        file.lock_exclusive()
            .map_err(|e| format!("Failed to lock counter file {}: {}", counter_file, e))?;

        // Read current counter value inside the lock
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("Failed to read counter from {}: {}", counter_file, e))?;

        // Parse current value or initialize to 0 if empty/invalid
        let current: u64 = if content.trim().is_empty() {
            0
        } else {
            content
                .trim()
                .parse()
                .map_err(|e| format!("Invalid counter in {}: {}", counter_file, e))?
        };

        // Increment counter with overflow check
        // While reaching u64::MAX would take 584 million years at 1000 signatures/second,
        // production code should not have panic paths
        let next = current.checked_add(1).ok_or_else(|| {
            format!(
                "Counter overflow: reached maximum value (u64::MAX = {}). \
                 This key has been used for the maximum number of signatures. \
                 Please rotate to a new signing key with a different key_id.",
                u64::MAX
            )
        })?;

        // Write new value back to file (still holding exclusive lock)
        file.set_len(0)
            .map_err(|e| format!("Failed to truncate counter file {}: {}", counter_file, e))?;
        // Seek to start after truncation (set_len doesn't change file position)
        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("Failed to seek in counter file {}: {}", counter_file, e))?;
        file.write_all(next.to_string().as_bytes())
            .map_err(|e| format!("Failed to write counter to {}: {}", counter_file, e))?;

        // Sync to disk to ensure durability
        file.sync_all()
            .map_err(|e| format!("Failed to sync counter file {}: {}", counter_file, e))?;

        // Explicitly unlock (though it auto-unlocks on drop)
        file.unlock()
            .map_err(|e| format!("Failed to unlock counter file {}: {}", counter_file, e))?;

        Ok(next)
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
    use tempfile::NamedTempFile;
    use std::io::Write;

    // [utest->swdd~cli-sign-workload-manifests~1]
    #[tokio::test]
    async fn utest_sign_command_creates_valid_signature() {
        let yaml_content = r#"
apiVersion: v0.1
workloads:
  test-workload:
    runtime: podman
    agent: agent_A
    restartPolicy: ALWAYS
    runtimeConfig: |
      image: alpine:latest
      commandOptions: ["-i", "-t"]
"#;

        let key_pem = r#"-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIJ+DYvh6SEqVTm50DFtMDoQikTmiCqirVv9mWG9qfSnF
-----END PRIVATE KEY-----"#;

        let mut yaml_file = NamedTempFile::new().unwrap();
        yaml_file.write_all(yaml_content.as_bytes()).unwrap();

        let mut key_file = NamedTempFile::new().unwrap();
        key_file.write_all(key_pem.as_bytes()).unwrap();

        let sign_cmd = SignCommand::new(
            yaml_file.path().to_str().unwrap().to_string(),
            "test-key".to_string(),
            key_file.path().to_str().unwrap().to_string(),
            Some(1),
        );

        let result = sign_cmd.execute().await;
        assert!(result.is_ok(), "Signing should succeed");
    }

    // [utest->swdd~cli-sign-workload-manifests~1]
    #[test]
    fn utest_parse_ed25519_private_key_valid() {
        let key_pem = r#"-----BEGIN PRIVATE KEY-----
MC4CAQAwBQYDK2VwBCIEIJ+DYvh6SEqVTm50DFtMDoQikTmiCqirVv9mWG9qfSnF
-----END PRIVATE KEY-----"#;

        let sign_cmd = SignCommand::new(
            "dummy.yaml".to_string(),
            "test".to_string(),
            "dummy.key".to_string(),
            None,
        );

        let result = sign_cmd.parse_ed25519_private_key(key_pem);
        assert!(result.is_ok(), "Should parse valid private key");
    }

    // [utest->swdd~cli-sign-workload-manifests~1]
    #[test]
    fn utest_parse_ed25519_private_key_invalid() {
        let invalid_key = "not a valid key";

        let sign_cmd = SignCommand::new(
            "dummy.yaml".to_string(),
            "test".to_string(),
            "dummy.key".to_string(),
            None,
        );

        let result = sign_cmd.parse_ed25519_private_key(invalid_key);
        assert!(result.is_err(), "Should fail on invalid key");
    }

    // [utest->swdd~cli-sign-workload-manifests~1]
    #[test]
    fn utest_load_next_counter_increments() {
        let sign_cmd = SignCommand::new(
            "dummy.yaml".to_string(),
            "test-counter".to_string(),
            "dummy.key".to_string(),
            None,
        );

        let counter1 = sign_cmd.load_next_counter();
        let counter2 = sign_cmd.load_next_counter();
        assert!(counter2 > counter1, "Counter should increment");

        // Cleanup
        let _ = fs::remove_file(format!(".ank-counter-{}", sign_cmd.key_id));
    }

    // Test counter overflow handling (u64::MAX edge case)
    #[test]
    fn utest_load_next_counter_overflow_protection() {
        let sign_cmd = SignCommand::new(
            "dummy.yaml".to_string(),
            "test-overflow".to_string(),
            "dummy.key".to_string(),
            None,
        );

        let counter_file = format!(".ank-counter-{}", sign_cmd.key_id);

        // Test 1: Set counter to u64::MAX - 1, should successfully increment to MAX
        fs::write(&counter_file, (u64::MAX - 1).to_string()).unwrap();
        let result = sign_cmd.load_next_counter();
        assert!(result.is_ok(), "MAX-1 + 1 should succeed");
        assert_eq!(result.unwrap(), u64::MAX, "Counter should be at MAX");

        // Test 2: Now counter file contains MAX, next increment should fail
        let overflow_result = sign_cmd.load_next_counter();
        assert!(
            overflow_result.is_err(),
            "MAX + 1 should fail with overflow error"
        );
        let error_msg = overflow_result.unwrap_err();
        assert!(
            error_msg.contains("Counter overflow"),
            "Error message should mention counter overflow, got: {}",
            error_msg
        );

        // Cleanup
        let _ = fs::remove_file(&counter_file);
    }
}

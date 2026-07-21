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

// [impl->swdd~cli-verify-signed-manifests~1]

use ankaios_api::ank_base::UpdateStateRequest;
use common::objects::canonical::Canonical;
use common::objects::signed_payload::SignedPayload;
use ed25519_dalek::{Signature, VerifyingKey};
use prost::Message;
use std::fs;

pub struct VerifyCommand {
    input_file: String,
    public_key_path: String,
}

impl VerifyCommand {
    pub fn new(input_file: String, public_key_path: String) -> Self {
        Self {
            input_file,
            public_key_path,
        }
    }

    pub async fn execute(&self) -> Result<(), String> {
        // 1. Read and decode the binary protobuf file
        let data = fs::read(&self.input_file)
            .map_err(|e| format!("Failed to read input file: {}", e))?;

        let request = UpdateStateRequest::decode(&data[..])
            .map_err(|e| format!("Failed to decode protobuf: {}", e))?;

        // 2. Check if signature metadata is present
        let metadata = request
            .signature_metadata
            .as_ref()
            .ok_or("No signature metadata found in file")?;

        println!("📄 Manifest Information:");
        println!("  File: {}", self.input_file);
        println!();
        println!("🔑 Signature Metadata:");
        println!("  Key ID:    {}", metadata.key_id);
        println!("  Counter:   {}", metadata.counter);
        println!("  Timestamp: {} ({})", metadata.timestamp, format_timestamp(metadata.timestamp));
        println!("  Signature: {} bytes", metadata.signature.len());
        println!();

        // 3. Extract and canonicalize the state
        let state = request
            .new_state
            .as_ref()
            .and_then(|cs| cs.desired_state.as_ref())
            .ok_or("Missing state in UpdateStateRequest")?;

        let canonical = state
            .to_canonical_bytes()
            .map_err(|e| format!("Failed to canonicalize state: {}", e))?;

        println!("📦 Canonical Payload:");
        println!("  Size: {} bytes", canonical.len());
        println!();

        // 4. Reconstruct the signed payload
        let payload = SignedPayload::new(metadata.counter, metadata.timestamp, canonical);
        let payload_bytes = payload
            .to_deterministic_bytes()
            .map_err(|e| format!("Failed to serialize payload: {}", e))?;

        println!("🔐 Signed Payload:");
        println!("  Counter:   {}", metadata.counter);
        println!("  Timestamp: {}", metadata.timestamp);
        println!("  Total size: {} bytes", payload_bytes.len());
        println!();

        // 5. Load public key
        let public_key = self.load_public_key(&self.public_key_path)?;

        // 6. Verify signature
        let signature = Signature::from_slice(&metadata.signature)
            .map_err(|_| "Invalid signature format")?;

        match public_key.verify_strict(&payload_bytes, &signature) {
            Ok(()) => {
                println!("✅ SIGNATURE VALID");
                println!();
                println!("The signature is valid and the manifest has not been tampered with.");
                println!("Verified with public key: {}", self.public_key_path);
                Ok(())
            }
            Err(_) => {
                println!("❌ SIGNATURE INVALID");
                println!();
                println!("The signature verification failed. Possible reasons:");
                println!("  • The manifest has been modified after signing");
                println!("  • Wrong public key used for verification");
                println!("  • The signature is corrupted");
                Err("Signature verification failed".to_string())
            }
        }
    }

    fn load_public_key(&self, path: &str) -> Result<VerifyingKey, String> {
        let pem_content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read public key: {}", e))?;

        self.parse_ed25519_public_key(&pem_content)
    }

    fn parse_ed25519_public_key(&self, pem_content: &str) -> Result<VerifyingKey, String> {
        let key_array = common::pem_utils::parse_ed25519_public_key_pem(pem_content)?;
        VerifyingKey::from_bytes(&key_array)
            .map_err(|e| format!("Failed to parse public key: {}", e))
    }
}

fn format_timestamp(unix_timestamp: u64) -> String {
    let secs = unix_timestamp;
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    // Simple UTC format
    format!("{} days, {:02}:{:02}:{:02} UTC", days, hours, minutes, seconds)
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

    // [utest->swdd~cli-verify-signed-manifests~1]
    #[test]
    fn utest_parse_ed25519_public_key_valid() {
        let key_pem = r#"-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE=
-----END PUBLIC KEY-----"#;

        let verify_cmd = VerifyCommand::new(
            "dummy.pb".to_string(),
            "dummy.pub".to_string(),
        );

        let result = verify_cmd.parse_ed25519_public_key(key_pem);
        assert!(result.is_ok(), "Should parse valid public key");
    }

    // [utest->swdd~cli-verify-signed-manifests~1]
    #[test]
    fn utest_parse_ed25519_public_key_invalid() {
        let invalid_key = "not a valid key";

        let verify_cmd = VerifyCommand::new(
            "dummy.pb".to_string(),
            "dummy.pub".to_string(),
        );

        let result = verify_cmd.parse_ed25519_public_key(invalid_key);
        assert!(result.is_err(), "Should fail on invalid key");
    }

    // [utest->swdd~cli-verify-signed-manifests~1]
    #[test]
    fn utest_format_timestamp() {
        let timestamp = 1700000000u64; // Some timestamp
        let formatted = format_timestamp(timestamp);
        assert!(!formatted.is_empty(), "Timestamp should format");
        assert!(formatted.contains("UTC"), "Should include UTC");
    }
}

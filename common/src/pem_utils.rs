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

use base64::{engine::general_purpose, Engine as _};

// Ed25519 OID: 1.3.101.112 encoded as DER: 06 03 2b 65 70
const ED25519_OID: [u8; 5] = [0x06, 0x03, 0x2b, 0x65, 0x70];
const ED25519_KEY_LENGTH: usize = 32;

fn decode_pem_base64(pem_content: &str, expected_header: &str) -> Result<Vec<u8>, String> {
    let pem_content = pem_content.trim();

    let mut in_block = false;
    let mut base64_content = String::new();

    for line in pem_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("-----BEGIN ") && trimmed.ends_with("-----") {
            if !trimmed.contains(expected_header) {
                return Err(format!(
                    "Expected PEM header containing '{}', got: {}",
                    expected_header, trimmed
                ));
            }
            in_block = true;
            continue;
        }
        if trimmed.starts_with("-----END ") {
            break;
        }
        if in_block {
            base64_content.push_str(trimmed);
        }
    }

    if base64_content.is_empty() {
        return Err("No PEM content found".to_string());
    }

    general_purpose::STANDARD
        .decode(&base64_content)
        .map_err(|e| format!("Failed to decode base64: {}", e))
}

fn find_ed25519_oid(der_bytes: &[u8]) -> bool {
    der_bytes
        .windows(ED25519_OID.len())
        .any(|w| w == ED25519_OID)
}

/// Parse a PEM-encoded Ed25519 private key (PKCS#8 format) and return the raw 32-byte key.
pub fn parse_ed25519_private_key_pem(pem_content: &str) -> Result<[u8; 32], String> {
    let der_bytes = decode_pem_base64(pem_content, "PRIVATE KEY")?;

    if !find_ed25519_oid(&der_bytes) {
        return Err(
            "Not an Ed25519 key: OID 1.3.101.112 not found in DER encoding".to_string(),
        );
    }

    // PKCS#8 Ed25519 private key: the 32-byte key is wrapped in an OCTET STRING
    // at the end. The structure is: SEQUENCE { version, AlgorithmIdentifier, OCTET STRING { OCTET STRING { key } } }
    // The raw key is the last 32 bytes of the DER.
    if der_bytes.len() < ED25519_KEY_LENGTH {
        return Err(format!(
            "Private key DER too short: expected at least {} bytes, got {}",
            ED25519_KEY_LENGTH,
            der_bytes.len()
        ));
    }

    let key_start = der_bytes.len() - ED25519_KEY_LENGTH;
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&der_bytes[key_start..]);
    Ok(key_array)
}

/// Parse a PEM-encoded Ed25519 public key (SPKI format) and return the raw 32-byte key.
pub fn parse_ed25519_public_key_pem(pem_content: &str) -> Result<[u8; 32], String> {
    let der_bytes = decode_pem_base64(pem_content, "PUBLIC KEY")?;

    if !find_ed25519_oid(&der_bytes) {
        return Err(
            "Not an Ed25519 key: OID 1.3.101.112 not found in DER encoding".to_string(),
        );
    }

    if der_bytes.len() < ED25519_KEY_LENGTH {
        return Err(format!(
            "Public key DER too short: expected at least {} bytes, got {}",
            ED25519_KEY_LENGTH,
            der_bytes.len()
        ));
    }

    let key_start = der_bytes.len() - ED25519_KEY_LENGTH;
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&der_bytes[key_start..]);
    Ok(key_array)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
        MC4CAQAwBQYDK2VwBCIEIJ+DYvh6SEqVTm50DFtMDoQikTmiCqirVv9mWG9qfSnF\n\
        -----END PRIVATE KEY-----";

    const VALID_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\n\
        MCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE=\n\
        -----END PUBLIC KEY-----";

    #[test]
    fn test_parse_valid_private_key() {
        let result = parse_ed25519_private_key_pem(VALID_PRIVATE_KEY_PEM);
        assert!(result.is_ok(), "Should parse valid Ed25519 private key");
        assert_eq!(result.unwrap().len(), 32);
    }

    #[test]
    fn test_parse_valid_public_key() {
        let result = parse_ed25519_public_key_pem(VALID_PUBLIC_KEY_PEM);
        assert!(result.is_ok(), "Should parse valid Ed25519 public key");
        assert_eq!(result.unwrap().len(), 32);
    }

    #[test]
    fn test_reject_non_ed25519_key() {
        // RSA key (won't contain Ed25519 OID)
        let rsa_pem = "-----BEGIN PRIVATE KEY-----\n\
            MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7\n\
            -----END PRIVATE KEY-----";
        let result = parse_ed25519_private_key_pem(rsa_pem);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not an Ed25519 key"));
    }

    #[test]
    fn test_reject_wrong_pem_header() {
        let wrong_header = "-----BEGIN CERTIFICATE-----\n\
            MC4CAQAwBQYDK2VwBCIEIJ+DYvh6SEqVTm50DFtMDoQikTmiCqirVv9mWG9qfSnF\n\
            -----END CERTIFICATE-----";
        let result = parse_ed25519_private_key_pem(wrong_header);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected PEM header"));
    }

    #[test]
    fn test_reject_invalid_base64() {
        let invalid = "-----BEGIN PRIVATE KEY-----\n\
            not-valid-base64!!!\n\
            -----END PRIVATE KEY-----";
        let result = parse_ed25519_private_key_pem(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_empty_pem() {
        let result = parse_ed25519_private_key_pem("not a PEM file");
        assert!(result.is_err());
    }
}

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

// [impl->swdd~server-signature-includes-counter-timestamp~1]

use serde::{Deserialize, Serialize};

/// Payload structure that gets signed and verified
///
/// This structure contains the counter, timestamp, and canonical workload bytes.
/// The entire structure is serialized and then signed with Ed25519, ensuring that
/// counter and timestamp values cannot be modified without invalidating the signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPayload {
    /// Monotonic counter for replay attack prevention
    pub counter: u64,
    /// Unix timestamp (seconds since epoch)
    pub timestamp: u64,
    /// Canonical protobuf bytes of the workload/state
    #[serde(with = "serde_bytes")]
    pub workload_canonical: Vec<u8>,
}

impl SignedPayload {
    /// Create a new SignedPayload
    pub fn new(counter: u64, timestamp: u64, workload_canonical: Vec<u8>) -> Self {
        Self {
            counter,
            timestamp,
            workload_canonical,
        }
    }

    /// Serialize to deterministic bytes for signing/verification
    ///
    /// Uses bincode for deterministic serialization. The output is a
    /// canonical byte sequence that can be cryptographically signed.
    pub fn to_deterministic_bytes(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self)
            .map_err(|e| format!("Failed to serialize SignedPayload: {}", e))
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        bincode::deserialize(bytes)
            .map_err(|e| format!("Failed to deserialize SignedPayload: {}", e))
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

    // [utest->swdd~server-signature-includes-counter-timestamp~1]
    #[test]
    fn utest_signed_payload_serialization_deterministic() {
        let payload1 = SignedPayload::new(
            42,
            1234567890,
            vec![1, 2, 3, 4, 5],
        );

        let payload2 = SignedPayload::new(
            42,
            1234567890,
            vec![1, 2, 3, 4, 5],
        );

        let bytes1 = payload1.to_deterministic_bytes().unwrap();
        let bytes2 = payload2.to_deterministic_bytes().unwrap();

        assert_eq!(bytes1, bytes2, "Identical payloads should produce identical bytes");
    }

    // [utest->swdd~server-signature-includes-counter-timestamp~1]
    #[test]
    fn utest_signed_payload_different_counter() {
        let payload1 = SignedPayload::new(42, 1234567890, vec![1, 2, 3]);
        let payload2 = SignedPayload::new(43, 1234567890, vec![1, 2, 3]);

        let bytes1 = payload1.to_deterministic_bytes().unwrap();
        let bytes2 = payload2.to_deterministic_bytes().unwrap();

        assert_ne!(bytes1, bytes2, "Different counters should produce different bytes");
    }

    // [utest->swdd~server-signature-includes-counter-timestamp~1]
    #[test]
    fn utest_signed_payload_different_timestamp() {
        let payload1 = SignedPayload::new(42, 1234567890, vec![1, 2, 3]);
        let payload2 = SignedPayload::new(42, 1234567891, vec![1, 2, 3]);

        let bytes1 = payload1.to_deterministic_bytes().unwrap();
        let bytes2 = payload2.to_deterministic_bytes().unwrap();

        assert_ne!(bytes1, bytes2, "Different timestamps should produce different bytes");
    }

    // [utest->swdd~server-signature-includes-counter-timestamp~1]
    #[test]
    fn utest_signed_payload_roundtrip() {
        let original = SignedPayload::new(
            100,
            9876543210,
            vec![0xde, 0xad, 0xbe, 0xef],
        );

        let bytes = original.to_deterministic_bytes().unwrap();
        let deserialized = SignedPayload::from_bytes(&bytes).unwrap();

        assert_eq!(original, deserialized, "Roundtrip should preserve all fields");
        assert_eq!(deserialized.counter, 100);
        assert_eq!(deserialized.timestamp, 9876543210);
        assert_eq!(deserialized.workload_canonical, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    // [utest->swdd~server-signature-includes-counter-timestamp~1]
    #[test]
    fn utest_signed_payload_empty_workload() {
        let payload = SignedPayload::new(1, 2, vec![]);
        let bytes = payload.to_deterministic_bytes();

        assert!(bytes.is_ok(), "Empty workload should be serializable");

        let deserialized = SignedPayload::from_bytes(&bytes.unwrap()).unwrap();
        assert_eq!(deserialized.workload_canonical, Vec::<u8>::new());
    }
}

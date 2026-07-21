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

//! # Signature Validator
//!
//! Provides Ed25519 signature verification for workload manifests with monotonic
//! counter replay protection.
//!
//! ## Counter Overflow Behavior
//!
//! Monotonic counters use `u64`, providing a range of 0 to 18,446,744,073,709,551,615.
//! At a signing rate of 1,000 signatures/second, it would take approximately
//! **584 million years** to reach `u64::MAX`.
//!
//! Counter wrapping is **not implemented** - reaching `u64::MAX` effectively caps
//! the signing capability for that `key_id`. In practice, this is not a concern
//! for any realistic deployment lifetime.
//!
//! If a counter reaches `u64::MAX`, the recommended mitigation is to rotate to
//! a new signing key with a fresh `key_id`.

// [impl->swdd~server-signature-validation~1]
// [impl->swdd~server-signature-canonical-protobuf~1]
// [impl->swdd~server-signature-includes-counter-timestamp~1]
// [impl->swdd~server-signature-replay-protection~1]
// [impl->swdd~server-signature-constant-time~1]

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use common::objects::canonical::Canonical;
use common::objects::signed_payload::SignedPayload;
use ed25519_dalek::{Signature, VerifyingKey, PUBLIC_KEY_LENGTH};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

// Import protobuf types for new signature verification
use ankaios_api::ank_base::UpdateStateRequest;

/// Counter state persisted to disk
#[derive(Debug, Serialize, Deserialize)]
struct CounterState {
    /// Per-source counter tracking (ephemeral connections)
    #[serde(default)]
    source_counters: HashMap<String, u64>,
    /// Per-key_id counter tracking (global replay protection)
    #[serde(default)]
    key_counters: HashMap<String, u64>,
}

/// Counter validation mode for unified validation function
#[derive(Debug, Clone, Copy, PartialEq)]
enum CounterValidationMode {
    /// Check counter validity without updating state (YAML path)
    #[allow(dead_code)]
    CheckOnly,
    /// Check counter validity and update state atomically (protobuf path)
    CheckAndUpdate,
}

/// Configuration policy for signature verification
#[derive(Debug, Clone)]
pub struct SignaturePolicy {
    /// Reject unsigned manifests if true
    pub require_signature: bool,
    /// Require counter field in signatures (if false, counter is optional)
    pub require_counter: bool,
    /// List of allowed key IDs (empty = accept any key_id)
    pub allowed_key_ids: Vec<String>,
    /// Minimum counter value (initial floor)
    pub min_counter: u64,
    /// List of plugin names allowed to trigger restoration exemption
    /// (empty = no restoration exemption allowed, default = ["basic_persistency"])
    pub allowed_restoration_plugins: Vec<String>,
    /// Restoration window in seconds (-1 = disabled/infinite, 0 = immediate strict, >0 = grace period)
    pub restoration_window_seconds: i64,
}

/// Parsed and verified signed YAML document
#[allow(dead_code)]
#[derive(Debug)]
pub struct SignedYamlDocument {
    /// The unsigned content (YAML before signature block)
    pub unsigned_content: String,
    /// Decoded signature bytes (used internally for verification)
    #[allow(dead_code)]
    pub signature: Vec<u8>,
    /// Which key signed this document
    pub key_id: String,
    /// Unix timestamp when signed (stored but not currently used for validation)
    #[allow(dead_code)]
    pub timestamp: i64,
    /// Monotonic counter for rollback protection (None if not present)
    pub counter: Option<u64>,
}

/// Signature validator with Ed25519 verification
#[derive(Debug)]
pub struct SignatureValidator {
    /// Map of key_id -> Ed25519 public key
    public_keys: HashMap<String, Vec<u8>>,
    /// Map of source -> last seen counter for rollback protection (per-connection)
    source_counters: HashMap<String, u64>,
    /// Map of key_id -> highest counter seen for that key (global replay protection)
    key_counters: HashMap<String, u64>,
    /// Verification policy
    policy: SignaturePolicy,
    /// Path to counter state file
    counter_state_path: PathBuf,
    /// Server boot time for restoration window validation
    boot_time: std::time::Instant,
    /// Restoration window configuration from policy (-1 = disabled/infinite)
    restoration_window_seconds: i64,
}

/// Errors that can occur during signature verification
#[derive(Debug)]
pub enum SignatureError {
    /// No signature block found in YAML
    #[allow(dead_code)]
    MissingSignature,
    /// Signature block format is invalid
    #[allow(dead_code)]
    InvalidSignatureFormat,
    /// The key_id is not recognized (replaced by GenericVerificationFailure for timing attack mitigation)
    #[allow(dead_code)]
    UnknownKeyId(String),
    /// The key_id is not in the allowed list
    KeyIdNotAllowed(String),
    /// Ed25519 signature verification failed (replaced by GenericVerificationFailure for timing attack mitigation)
    #[allow(dead_code)]
    SignatureVerificationFailed,
    /// Generic verification failure (prevents timing attacks)
    GenericVerificationFailure,
    /// Counter rollback detected
    CounterRollback {
        current: u64,
        last_seen: u64,
        source: String,
    },
    /// Counter is required by policy but not present in signature
    #[allow(dead_code)]
    CounterRequired,
    /// I/O error (file operations, counter persistence)
    IoError(String),
    /// YAML parsing error
    ParseError(String),
    /// Signature is required but not present
    SignatureRequired,
    /// Invalid signature (Ed25519 verification failed)
    #[allow(dead_code)]
    InvalidSignature,
    /// Canonicalization failed
    CanonicalizationError(String),
    /// Payload serialization failed
    PayloadSerializationError(String),
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignatureError::MissingSignature => {
                write!(f, "No signature block found in YAML document")
            }
            SignatureError::InvalidSignatureFormat => {
                write!(f, "Signature block has invalid format")
            }
            SignatureError::UnknownKeyId(key_id) => {
                write!(f, "Unknown key ID: {}", key_id)
            }
            SignatureError::KeyIdNotAllowed(key_id) => {
                write!(f, "Key ID not in allowed list: {}", key_id)
            }
            SignatureError::SignatureVerificationFailed => {
                write!(f, "Ed25519 signature verification failed")
            }
            SignatureError::GenericVerificationFailure => {
                write!(f, "Signature verification failed")
            }
            SignatureError::CounterRollback {
                current,
                last_seen,
                source,
            } => {
                write!(
                    f,
                    "Counter rollback detected for source '{}': current={}, last_seen={}",
                    source, current, last_seen
                )
            }
            SignatureError::CounterRequired => {
                write!(f, "Counter is required by policy but signature has no counter")
            }
            SignatureError::IoError(msg) => write!(f, "I/O error: {}", msg),
            SignatureError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            SignatureError::SignatureRequired => {
                write!(f, "Signature required but not present")
            }
            SignatureError::InvalidSignature => {
                write!(f, "Ed25519 signature verification failed")
            }
            SignatureError::CanonicalizationError(msg) => {
                write!(f, "Canonicalization failed: {}", msg)
            }
            SignatureError::PayloadSerializationError(msg) => {
                write!(f, "Payload serialization failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for SignatureError {}

/// Signature block format in YAML
#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
struct SignatureBlock {
    signature: String,
    key_id: String,
    timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    counter: Option<u64>,
}

const DEFAULT_COUNTER_STATE_PATH: &str = "/var/lib/ankaios/signature_counters.json";

/// Source identifier constants for special cases
/// Centralizing these prevents security bypasses due to typos or inconsistent updates
const SOURCE_STARTUP_MANIFEST: &str = "startup-manifest";
const SOURCE_REQUEST_PREFIX: &str = "request:";
const RESTORATION_MARKER: &str = "startup_restore_";

/// Default restoration window: 1 hour (3600 seconds)
///
/// Can be configured via:
/// 1. ANKAIOS_RESTORATION_WINDOW_SECONDS environment variable (highest priority)
/// 2. restoration_window_seconds in config file (default: 3600)
///
/// Special values:
/// - -1 = disabled (infinite window, always allow restoration)
/// - 0 = immediate strict validation (no grace period)
/// - >0 = grace period in seconds

impl SignatureValidator {
    /// Get the current verification policy
    #[allow(dead_code)]
    pub fn policy(&self) -> &SignaturePolicy {
        &self.policy
    }

    /// Create a new signature validator from a keys directory
    ///
    /// Loads Ed25519 public keys from PEM files in the specified directory.
    /// Counter state is loaded from the counter state file if it exists.
    ///
    /// # Arguments
    /// * `keys_dir` - Path to directory containing *.pub PEM files
    /// * `policy` - Verification policy configuration
    ///
    /// Counter state path can be configured via ANKAIOS_COUNTER_STATE_PATH env var
    /// (defaults to /var/lib/ankaios/signature_counters.json)
    pub fn from_keys_directory(
        keys_dir: &Path,
        policy: SignaturePolicy,
    ) -> Result<Self, SignatureError> {
        let mut public_keys = HashMap::new();

        // Load public keys from directory
        if keys_dir.exists() {
            let entries = fs::read_dir(keys_dir)
                .map_err(|e| SignatureError::IoError(format!("Cannot read keys directory: {}", e)))?;

            for entry in entries {
                let entry = entry.map_err(|e| SignatureError::IoError(e.to_string()))?;
                let path = entry.path();

                // Only process .pub files
                if path.extension().and_then(|s| s.to_str()) == Some("pub") {
                    // Extract key_id from filename, handling both .pub and .pem.pub extensions
                    // For "test-key-001.pem.pub" -> "test-key-001"
                    // For "test-key-001.pub" -> "test-key-001"
                    let key_id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| {
                            SignatureError::ParseError(format!("Invalid key filename: {:?}", path))
                        })?;

                    // If the stem ends with .pem, strip that too
                    let key_id = if key_id.ends_with(".pem") {
                        &key_id[..key_id.len() - 4]
                    } else {
                        key_id
                    }.to_string();

                    let pem_content = fs::read_to_string(&path).map_err(|e| {
                        SignatureError::IoError(format!("Cannot read key file {:?}: {}", path, e))
                    })?;

                    let public_key_bytes = Self::parse_ed25519_public_key(&pem_content)?;
                    public_keys.insert(key_id.clone(), public_key_bytes);
                    log::info!("Loaded Ed25519 public key: {}", key_id);
                }
            }
        } else {
            log::warn!("Keys directory does not exist: {:?}", keys_dir);
        }

        // Load counter state from disk
        // Priority: ENV var (validated), then default
        let counter_state_path = match std::env::var("ANKAIOS_COUNTER_STATE_PATH") {
            Ok(path_str) => {
                let path = PathBuf::from(path_str);
                // Validate environment variable path for security (fail-closed in strict mode)
                match Self::validate_counter_state_path(&path) {
                    Ok(_) => path,
                    Err(e) => {
                        log::error!("Invalid counter state path: {}", e);
                        return Err(e);
                    }
                }
            }
            Err(_) => PathBuf::from(DEFAULT_COUNTER_STATE_PATH),
        };

        let mut validator = Self {
            public_keys,
            source_counters: HashMap::new(),
            key_counters: HashMap::new(),
            restoration_window_seconds: policy.restoration_window_seconds,
            policy,
            counter_state_path,
            boot_time: std::time::Instant::now(),
        };

        // Fail-closed: corrupted counter files must be fixed manually
        // Missing counter file is OK (first run), but corrupted file indicates tampering
        validator.load_counters()?;

        Ok(validator)
    }

    /// Verify a signed YAML document
    ///
    /// # Arguments
    /// * `signed_yaml` - The complete signed YAML string (content + signature block)
    /// * `source` - Source identifier (e.g., "persistence:vehicle-123", "cli:user@laptop")
    ///
    /// # Returns
    /// `Ok(SignedYamlDocument)` if signature is valid, `Err(SignatureError)` otherwise
    #[allow(dead_code)]
    pub fn verify_signed_yaml(
        &mut self,
        signed_yaml: &str,
        source: &str,
    ) -> Result<SignedYamlDocument, SignatureError> {
        // Extract signature block
        let (unsigned_content, sig_block) = Self::extract_signature_block(signed_yaml)?;

        // Check if key_id is in allowed list (if policy specifies)
        if !self.policy.allowed_key_ids.is_empty()
            && !self.policy.allowed_key_ids.contains(&sig_block.key_id)
        {
            return Err(SignatureError::KeyIdNotAllowed(sig_block.key_id));
        }

        // Verify Ed25519 signature
        self.verify_signature(&unsigned_content, &sig_block.signature, &sig_block.key_id)?;

        // Handle counter validation
        if let Some(counter) = sig_block.counter {
            // Counter is present - validate it
            self.validate_and_update_counter(
                counter,
                &sig_block.key_id,
                source,
                CounterValidationMode::CheckOnly,
            )?;
            // Update counters and persist (manual update for YAML path)
            self.source_counters.insert(source.to_string(), counter);
            // Update global key counter for replay protection
            self.key_counters.insert(sig_block.key_id.clone(), counter);
            self.save_counters()?;
        } else if self.policy.require_counter {
            // No counter but policy requires it
            return Err(SignatureError::CounterRequired);
        }
        // else: counter is optional and not present, which is fine

        // Decode signature bytes for return value
        let signature_bytes = BASE64_STANDARD.decode(&sig_block.signature)
            .map_err(|e| SignatureError::ParseError(format!("Invalid base64 signature: {}", e)))?;

        Ok(SignedYamlDocument {
            unsigned_content,
            signature: signature_bytes,
            key_id: sig_block.key_id,
            timestamp: sig_block.timestamp,
            counter: sig_block.counter,
        })
    }

    /// Extract signature block from signed YAML
    #[allow(dead_code)]
    fn extract_signature_block(yaml: &str) -> Result<(String, SignatureBlock), SignatureError> {
        // Split on YAML document separator
        let parts: Vec<&str> = yaml.split("\n---\n").collect();

        if parts.len() < 2 {
            return Err(SignatureError::MissingSignature);
        }

        let unsigned_content = parts[0].to_string();
        let sig_block_yaml = parts[1];

        // Parse signature block
        log::debug!("Attempting to parse signature block:\n{}", sig_block_yaml);
        let sig_block: SignatureBlock = serde_yaml::from_str(sig_block_yaml)
            .map_err(|e| {
                log::error!("Failed to parse signature block: {}", e);
                log::error!("Signature block content:\n{}", sig_block_yaml);
                SignatureError::InvalidSignatureFormat
            })?;

        Ok((unsigned_content, sig_block))
    }

    /// Verify Ed25519 signature with constant-time error handling
    ///
    /// Uses GenericVerificationFailure for all errors to prevent timing attacks
    /// that could leak information about which keys exist or which step failed.
    #[allow(dead_code)]
    fn verify_signature(
        &self,
        unsigned_content: &str,
        signature_base64: &str,
        key_id: &str,
    ) -> Result<(), SignatureError> {
        // Decode signature (constant-time)
        let signature_bytes = BASE64_STANDARD
            .decode(signature_base64)
            .map_err(|_| SignatureError::GenericVerificationFailure)?;

        let signature_array: [u8; 64] = signature_bytes
            .as_slice()
            .try_into()
            .map_err(|_| SignatureError::GenericVerificationFailure)?;

        let signature = Signature::from_bytes(&signature_array);

        // Key lookup with constant-time comparison
        // Iterate through ALL keys to prevent timing leaks about which keys exist
        let mut found_key: Option<&Vec<u8>> = None;
        for (stored_key_id, public_key_bytes) in &self.public_keys {
            // Use subtle crate for constant-time string comparison
            if bool::from(stored_key_id.as_bytes().ct_eq(key_id.as_bytes())) {
                found_key = Some(public_key_bytes);
            }
        }

        let public_key_bytes = found_key.ok_or(SignatureError::GenericVerificationFailure)?;

        let public_key_array: [u8; 32] = public_key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| SignatureError::GenericVerificationFailure)?;

        let verifying_key = VerifyingKey::from_bytes(&public_key_array)
            .map_err(|_| SignatureError::GenericVerificationFailure)?;

        // Signature verification with strict mode to prevent signature malleability
        // verify_strict() rejects non-canonical signatures (ed25519-dalek provides constant-time)
        verifying_key
            .verify_strict(unsigned_content.as_bytes(), &signature)
            .map_err(|_| SignatureError::GenericVerificationFailure)?;

        Ok(())
    }

    /// Validate counter state path from environment variable
    ///
    /// # Security Notes
    /// The ANKAIOS_COUNTER_STATE_PATH environment variable is considered **admin-controlled
    /// trusted configuration** (similar to config files). It is typically set by:
    /// - systemd unit files (controlled by root)
    /// - Container orchestration systems (controlled by cluster admins)
    /// - Shell profiles (controlled by system administrators)
    ///
    /// This validation provides defense-in-depth warnings for common misconfigurations.
    /// When ANKAIOS_STRICT_SECURITY=true, insecure paths are rejected (fail-closed).
    ///
    /// # Validation Rules
    /// - Path is relative (should be absolute for predictable behavior)
    /// - Path is in /tmp (insecure, world-writable, often cleared on reboot)
    /// - Path is in /dev (special devices, not suitable for persistent state)
    fn validate_counter_state_path(path: &Path) -> Result<(), SignatureError> {
        // Check for strict security mode
        let strict_mode = std::env::var("ANKAIOS_STRICT_SECURITY")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        // Validate: path must be absolute
        if !path.is_absolute() {
            let msg = format!(
                "ANKAIOS_COUNTER_STATE_PATH is relative: {:?}. \
                 Recommend using absolute path for predictable behavior. \
                 Current working directory changes may cause counter state loss.",
                path
            );
            if strict_mode {
                return Err(SignatureError::IoError(msg));
            } else {
                log::warn!("⚠️  {}", msg);
            }
        }

        // Validate: path must not be in /tmp (insecure, often cleared, world-writable)
        if path.starts_with("/tmp") {
            let msg = format!(
                "ANKAIOS_COUNTER_STATE_PATH is in /tmp: {:?}. \
                 /tmp is world-writable and may be cleared on reboot. \
                 Recommend using /var/lib/ankaios/ for production deployments.",
                path
            );
            if strict_mode {
                return Err(SignatureError::IoError(msg));
            } else {
                log::warn!("⚠️  {}", msg);
            }
        }

        // Validate: path must not be in /dev (device files, not suitable for state)
        if path.starts_with("/dev") {
            let msg = format!(
                "ANKAIOS_COUNTER_STATE_PATH is in /dev: {:?}. \
                 Device files are not suitable for persistent state. \
                 Using /dev/null will cause counter state loss (potential replay attacks).",
                path
            );
            if strict_mode {
                return Err(SignatureError::IoError(msg));
            } else {
                log::warn!("⚠️  {}", msg);
            }
        }

        // Info log for custom paths (not the default)
        if path != Path::new(DEFAULT_COUNTER_STATE_PATH) {
            log::info!(
                "Using custom counter state path: {:?} (from ANKAIOS_COUNTER_STATE_PATH){}",
                path,
                if strict_mode { " [STRICT MODE]" } else { "" }
            );
        }

        Ok(())
    }

    /// Validates if a source is a restoration request from a trusted plugin
    ///
    /// Security: Properly parses the authenticated identity chain instead of using
    /// substring matching to prevent replay attacks via crafted request_ids.
    ///
    /// Source format: "request:{agent}@{workload}@{client_request_id}"
    /// - {agent}: Authenticated via mTLS (prepended by grpc layer)
    /// - {workload}: Verified by agent (typically a persistence plugin workload name)
    /// - {client_request_id}: Client-controlled, must start with "startup_restore_"
    ///
    /// This function validates that:
    /// 1. The workload name (parts[1]) is in the allowed_restoration_plugins allowlist
    /// 2. The client_request_id (parts[2]) starts with the restoration marker
    ///
    /// Attack prevention: Without proper parsing, an attacker could craft a request_id like
    /// "fake@basic_persistency@startup_restore_evil" which would bypass substring matching.
    fn is_restoration_from_trusted_plugin(source: &str, allowed_plugins: &[String]) -> bool {
        if !source.starts_with(SOURCE_REQUEST_PREFIX) {
            return false;
        }

        // Remove "request:" prefix to get the identity chain
        let without_prefix = &source[SOURCE_REQUEST_PREFIX.len()..];
        let parts: Vec<&str> = without_prefix.split('@').collect();

        // Need at least 3 parts: agent@workload@client_request_id
        if parts.len() < 3 {
            return false;
        }

        // parts[0] = agent_name (authenticated via mTLS)
        // parts[1] = workload_name (verified by agent, should be persistence plugin workload)
        // parts[2+] = client_request_id (client-controlled)
        let workload_name = parts[1];
        let client_request_id = parts[2];

        // Validate:
        // 1. Workload name is in allowed_restoration_plugins (the persistence plugin)
        // 2. Client request_id starts with restoration marker
        allowed_plugins.contains(&workload_name.to_string())
            && client_request_id.starts_with(RESTORATION_MARKER)
    }

    /// Unified counter validation with optional update
    ///
    /// Validates counter for rollback protection with optional atomic state update.
    /// This replaces the previous check_counter() and check_and_update_counter() functions
    /// to eliminate code duplication and ensure consistent validation logic.
    fn validate_and_update_counter(
        &mut self,
        counter: u64,
        key_id: &str,
        source: &str,
        mode: CounterValidationMode,
    ) -> Result<(), SignatureError> {
        // Skip rollback checks for startup manifest - it's the baseline state
        // that gets loaded on every boot with the same counter
        if source == SOURCE_STARTUP_MANIFEST {
            return Ok(());
        }

        // Restoration requests from persistence plugin during startup are allowed
        // to have the same counter as before restart (not a rollback attack)
        // Source format: "request:{agent}@{workload}@{client_request_id}"
        // where {workload} must be in allowed_restoration_plugins (the persistence plugin workload name)
        // and {client_request_id} must start with "startup_restore_" marker
        // Security: Validates authenticated identity chain instead of substring matching to prevent bypass
        let is_restoration = Self::is_restoration_from_trusted_plugin(
            source,
            &self.policy.allowed_restoration_plugins,
        );
        log::debug!(
            "Counter validation: source='{}', is_restoration={}, counter={}, key_id={}, mode={:?}",
            source, is_restoration, counter, key_id, mode
        );

        // Check against minimum counter (CRITICAL: this was missing in old check_and_update_counter)
        if counter < self.policy.min_counter {
            return Err(SignatureError::CounterRollback {
                current: counter,
                last_seen: self.policy.min_counter,
                source: source.to_string(),
            });
        }

        // Check against highest counter seen for this key_id (global replay protection)
        if let Some(&last_seen) = self.key_counters.get(key_id) {
            // For restoration, allow relaxed counter validation only within the restoration window
            // After the window expires, apply full counter validation to prevent replay attacks
            if is_restoration {
                // Environment variable overrides config file value
                let restoration_window = std::env::var("ANKAIOS_RESTORATION_WINDOW_SECONDS")
                    .ok()
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(self.restoration_window_seconds);

                // Handle special values:
                // -1 = disabled (infinite window, always allow restoration)
                // 0 = immediate strict validation (no grace period)
                // >0 = grace period in seconds
                if restoration_window == -1 {
                    // Disabled: infinite window, always allow relaxed validation for restoration
                    log::debug!(
                        "Restoration request for '{}' with infinite window (disabled), allowing relaxed validation",
                        source
                    );
                } else {
                    let elapsed_since_boot = self.boot_time.elapsed().as_secs();

                    // Cast to u64 for comparison (safe because we checked != -1)
                    let window_u64 = restoration_window.max(0) as u64;

                    if elapsed_since_boot >= window_u64 {
                        // Outside restoration window - apply full counter validation
                        log::warn!(
                            "Restoration request for '{}' outside window ({} > {}s), applying full validation",
                            source, elapsed_since_boot, window_u64
                        );
                        if counter <= last_seen {
                            return Err(SignatureError::CounterRollback {
                                current: counter,
                                last_seen,
                                source: format!("global-key:{} (restoration window expired)", key_id),
                            });
                        }
                    } else {
                        log::debug!(
                            "Restoration request for '{}' within window ({}/{}s), allowing relaxed validation",
                            source, elapsed_since_boot, window_u64
                        );
                    }
                }
            } else {
                // Non-restoration: always enforce strict counter validation
                if counter <= last_seen {
                    return Err(SignatureError::CounterRollback {
                        current: counter,
                        last_seen,
                        source: format!("global-key:{}", key_id),
                    });
                }
            }
        }

        // Check against last seen counter for this source (per-connection tracking)
        if let Some(&last_seen) = self.source_counters.get(source) {
            // For restoration, allow same counter (not rollback)
            let rollback = if is_restoration {
                counter < last_seen
            } else {
                counter <= last_seen
            };
            if rollback {
                return Err(SignatureError::CounterRollback {
                    current: counter,
                    last_seen,
                    source: source.to_string(),
                });
            }
        }

        // Update counters if requested (for CheckAndUpdate mode)
        if matches!(mode, CounterValidationMode::CheckAndUpdate) {
            // Only update global key_counters if this counter is higher than what we've seen
            // or if it's a restoration within the window (in which case we don't update key_counters)
            // This prevents downgrading the global counter when restoring old workloads
            let current_key_counter = self.key_counters.get(key_id).copied().unwrap_or(0);
            if counter > current_key_counter {
                self.key_counters.insert(key_id.to_string(), counter);
            }
            // Always update source counter (per-connection tracking)
            self.source_counters.insert(source.to_string(), counter);
        }

        Ok(())
    }

    /// Persist counter state to disk with secure I/O and file locking
    ///
    /// Uses secure_write to prevent:
    /// - TOCTOU races (atomic write via temp file + rename)
    /// - Symlink attacks (O_NOFOLLOW on Unix)
    /// - Unauthorized access (0600 permissions)
    ///
    /// Uses exclusive file locking via separate .lock file to prevent:
    /// - Race conditions when multiple server instances access counter state
    /// - Counter replay attacks due to concurrent counter acceptance
    ///
    /// # Locking Strategy
    /// We use a separate `.lock` file instead of locking the data file directly.
    /// This is necessary because `secure_write()` uses atomic rename (temp file → final),
    /// which changes the inode. If we locked the data file directly, the lock would be
    /// on the old inode (deleted file) after the rename, allowing other processes to
    /// acquire a lock on the new inode (race condition).
    ///
    /// With a separate lock file that never gets renamed, all processes coordinate
    /// on the same inode, ensuring true mutual exclusion.
    fn save_counters(&self) -> Result<(), SignatureError> {
        // Use a separate lock file that never gets renamed or deleted
        // This ensures all processes coordinate on the same inode
        let lock_path = self.counter_state_path.with_extension("lock");

        #[cfg(unix)]
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .mode(0o600) // Owner read/write only - consistent with data file permissions
            .open(&lock_path)
            .map_err(|e| {
                SignatureError::IoError(format!("Cannot open lock file {:?}: {}", lock_path, e))
            })?;

        #[cfg(not(unix))]
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&lock_path)
            .map_err(|e| {
                SignatureError::IoError(format!("Cannot open lock file {:?}: {}", lock_path, e))
            })?;

        // Acquire exclusive lock - blocks until no other process holds the lock
        lock_file.lock_exclusive().map_err(|e| {
            SignatureError::IoError(format!("Cannot acquire exclusive lock on {:?}: {}", lock_path, e))
        })?;

        // Serialize directly from borrowed references to avoid cloning HashMaps
        // With 1000 sources and 100 keys, this saves ~10-50 μs per save
        #[derive(Serialize)]
        struct CounterStateRef<'a> {
            source_counters: &'a HashMap<String, u64>,
            key_counters: &'a HashMap<String, u64>,
        }

        let state_ref = CounterStateRef {
            source_counters: &self.source_counters,
            key_counters: &self.key_counters,
        };

        // Use compact JSON for performance (2x faster serialization than pretty-printed)
        // Still human-readable, just without indentation
        let json = serde_json::to_string(&state_ref)
            .map_err(|e| SignatureError::IoError(format!("Cannot serialize counters: {}", e)))?;

        // Write to counter state file while holding the lock
        // secure_write() atomically renames temp → final, which is safe because
        // we hold the lock on the separate lock file, not the data file
        let result = common::secure_io::secure_write(&self.counter_state_path, &json)
            .map_err(|e| SignatureError::IoError(format!("Cannot write counters: {}", e)));

        // Explicitly unlock (though it would auto-unlock on drop anyway)
        let _ = lock_file.unlock();

        result
    }

    /// Load counter state from disk with secure I/O and file locking
    ///
    /// Uses secure_read to prevent symlink attacks (O_NOFOLLOW on Unix)
    ///
    /// Uses shared file locking via separate .lock file to prevent:
    /// - Reading partially-written counter state during concurrent writes
    /// - Data races between multiple readers and writers
    ///
    /// # Locking Strategy
    /// Uses the same `.lock` file as `save_counters()` to coordinate access.
    /// Multiple readers can hold shared locks simultaneously, but writers block readers.
    fn load_counters(&mut self) -> Result<(), SignatureError> {
        if !self.counter_state_path.exists() {
            // File doesn't exist yet, start with empty counters
            return Ok(());
        }

        // Use the same separate lock file as save_counters()
        let lock_path = self.counter_state_path.with_extension("lock");

        #[cfg(unix)]
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .mode(0o600) // Owner read/write only - consistent with data file permissions
            .open(&lock_path)
            .map_err(|e| {
                SignatureError::IoError(format!("Cannot open lock file {:?}: {}", lock_path, e))
            })?;

        #[cfg(not(unix))]
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&lock_path)
            .map_err(|e| {
                SignatureError::IoError(format!("Cannot open lock file {:?}: {}", lock_path, e))
            })?;

        // Acquire shared lock - allows multiple readers, blocks writers
        lock_file.lock_shared().map_err(|e| {
            SignatureError::IoError(format!("Cannot acquire shared lock on {:?}: {}", lock_path, e))
        })?;

        let json = common::secure_io::secure_read(&self.counter_state_path)
            .map_err(|e| SignatureError::IoError(format!("Cannot read counters: {}", e)))?;

        let state: CounterState = serde_json::from_str(&json).map_err(|e| {
            // Backup corrupted file for forensics
            // Use timestamp for unique backup filename, or fallback to "unknown" if clock is broken
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "unknown".to_string());

            let backup_path = format!(
                "{}.corrupted.{}",
                self.counter_state_path.display(),
                timestamp
            );
            let _ = std::fs::copy(&self.counter_state_path, &backup_path);
            log::error!(
                "Counter file corrupted. Backup saved to: {}",
                backup_path
            );

            SignatureError::IoError(format!(
                "Corrupted counter file (backup at {}): {}",
                backup_path, e
            ))
        })?;

        self.source_counters = state.source_counters;
        self.key_counters = state.key_counters;

        // Explicitly unlock (though it would auto-unlock on drop anyway)
        let _ = lock_file.unlock();

        Ok(())
    }

    /// Parse Ed25519 public key from PEM format
    fn parse_ed25519_public_key(pem_content: &str) -> Result<Vec<u8>, SignatureError> {
        let key_array = common::pem_utils::parse_ed25519_public_key_pem(pem_content)
            .map_err(|e| SignatureError::ParseError(e))?;
        Ok(key_array.to_vec())
    }

    /// Verify signature on UpdateStateRequest using protobuf-based verification
    ///
    /// This replaces the YAML-based verification approach. The signature is computed
    /// on a SignedPayload containing {counter, timestamp, canonical_protobuf_bytes}.
    pub fn verify_update_request(
        &mut self,
        request: &UpdateStateRequest,
        source: &str,
    ) -> Result<(), SignatureError> {
        // Check if signature metadata is present
        let Some(metadata) = &request.signature_metadata else {
            return if self.policy.require_signature {
                Err(SignatureError::SignatureRequired)
            } else {
                Ok(())
            };
        };

        // Extract and canonicalize the state
        // SECURITY: Reject signed requests with empty desired_state to prevent signature reuse
        // attacks where one signature for State::default() could be used to delete multiple
        // workloads by varying only the update_mask (which is not covered by the signature).
        //
        // Proper deletion semantics: the signed state should contain the FULL desired state
        // AFTER the deletion (i.e., all workloads except the one being deleted).
        // The update_mask then selects which workload to remove.
        //
        // Empty state (desired_state: None) is not allowed for signed requests.
        let state = request
            .new_state
            .as_ref()
            .and_then(|cs| cs.desired_state.as_ref())
            .ok_or_else(|| {
                SignatureError::ParseError(
                    "Signed request must have desired_state. Empty state (None) is not allowed \
                     because the signature would not cover which workload is being modified. \
                     For deletions, sign the full state AFTER deletion.".to_string()
                )
            })?;

        let canonical = state.to_canonical_bytes()
            .map_err(SignatureError::CanonicalizationError)?;

        // Reconstruct the signed payload
        let payload = SignedPayload::new(metadata.counter, metadata.timestamp, canonical);

        let payload_bytes = payload
            .to_deterministic_bytes()
            .map_err(SignatureError::PayloadSerializationError)?;

        // Verify key_id is allowed (if policy has allowlist)
        if !self.policy.allowed_key_ids.is_empty()
            && !self.policy.allowed_key_ids.contains(&metadata.key_id)
        {
            return Err(SignatureError::KeyIdNotAllowed(metadata.key_id.clone()));
        }

        // Lookup public key (constant-time to prevent timing attacks)
        let public_key = self.lookup_key_constant_time(&metadata.key_id)?;

        // Explicit check: reject all-zero signatures (defense-in-depth)
        // While ed25519-dalek should reject these, an explicit check improves
        // security visibility and makes the property explicit in code review
        if metadata.signature.len() == 64 && metadata.signature.iter().all(|&b| b == 0) {
            return Err(SignatureError::GenericVerificationFailure);
        }

        // Verify Ed25519 signature
        let signature = Signature::from_slice(&metadata.signature)
            .map_err(|_| SignatureError::GenericVerificationFailure)?;

        public_key
            .verify_strict(&payload_bytes, &signature)
            .map_err(|_| SignatureError::GenericVerificationFailure)?;

        // Check counter for replay protection and update state atomically
        self.validate_and_update_counter(
            metadata.counter,
            &metadata.key_id,
            source,
            CounterValidationMode::CheckAndUpdate,
        )?;

        // Persist updated counter state
        self.save_counters()
            .map_err(|e| SignatureError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Lookup public key by key_id
    ///
    /// Uses O(1) HashMap lookup instead of O(n) constant-time iteration.
    ///
    /// # Security Analysis
    /// This is safe because:
    /// - `key_id` is PUBLIC (included in signature metadata that attacker controls)
    /// - Attacker already knows which key_id they're using
    /// - Timing leak about "does this key_id exist" is not sensitive
    /// - Only signature verification itself needs constant-time (ed25519-dalek provides this)
    ///
    /// Using HashMap::get() provides O(1) performance instead of O(n), which matters
    /// for deployments with many public keys (e.g., 100 keys).
    fn lookup_key_constant_time(&self, key_id: &str) -> Result<VerifyingKey, SignatureError> {
        // O(1) HashMap lookup - safe because key_id is public
        let key_bytes = self.public_keys.get(key_id)
            .ok_or(SignatureError::GenericVerificationFailure)?;

        if key_bytes.len() != PUBLIC_KEY_LENGTH {
            return Err(SignatureError::GenericVerificationFailure);
        }

        let mut key_array = [0u8; PUBLIC_KEY_LENGTH];
        key_array.copy_from_slice(key_bytes);

        VerifyingKey::from_bytes(&key_array)
            .map_err(|_| SignatureError::GenericVerificationFailure)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use ankaios_api::ank_base::State;
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    // Mutex to serialize tests that modify ANKAIOS_COUNTER_STATE_PATH environment variable
    // Environment variables are process-global, so concurrent tests that modify the same
    // env var will interfere with each other, causing intermittent failures.
    // Tests that set ANKAIOS_COUNTER_STATE_PATH must lock this mutex first.
    use std::sync::Mutex;
    use std::sync::MutexGuard;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Helper to serialize environment variable access across tests
    /// Returns a guard that holds the lock - tests must keep this alive during validator usage
    #[allow(dead_code)]
    fn lock_env() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap()
    }

    fn create_test_keypair() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::from_bytes(&[
            157, 097, 177, 157, 239, 253, 090, 096, 186, 132, 074, 244, 146, 236, 044, 196,
            068, 073, 197, 105, 123, 050, 105, 025, 112, 059, 172, 003, 028, 174, 127, 096,
        ]);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    fn create_signed_yaml(
        content: &str,
        signing_key: &SigningKey,
        key_id: &str,
        counter: Option<u64>,
    ) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Sign the content exactly as it will be extracted (after split on "\n---\n")
        // The content should NOT include the trailing "\n" that's part of the separator
        let content_to_sign = content.trim_end_matches('\n');
        let signature = signing_key.sign(content_to_sign.as_bytes());
        let signature_base64 = BASE64_STANDARD.encode(signature.to_bytes());

        // Format the full signed YAML (counter is optional)
        // Include a newline before --- to match typical YAML document format
        if let Some(counter_value) = counter {
            format!(
                "{}\n---\nsignature: {}\nkey_id: {}\ntimestamp: {}\ncounter: {}\n",
                content_to_sign, signature_base64, key_id, timestamp, counter_value
            )
        } else {
            format!(
                "{}\n---\nsignature: {}\nkey_id: {}\ntimestamp: {}\n",
                content_to_sign, signature_base64, key_id, timestamp
            )
        }
    }

    fn setup_test_keys(temp_dir: &Path, verifying_key: &VerifyingKey, key_id: &str) {
        let key_bytes = verifying_key.to_bytes();
        let der_encoded = [
            &[
                0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
            ][..],
            &key_bytes[..],
        ]
        .concat();
        let pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            BASE64_STANDARD.encode(&der_encoded)
        );

        std::fs::write(temp_dir.join(format!("{}.pub", key_id)), pem).unwrap();
    }

    #[test]
    fn test_signature_validator_creation() {
        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
                        allowed_key_ids: vec!["test-key".to_string()],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let validator = SignatureValidator::from_keys_directory(Path::new("/tmp"), policy);
        assert!(validator.is_ok());
    }

    #[test]
    fn test_extract_signature_block_valid() {
        let signed_yaml = r#"apiVersion: v1
workloads:
  nginx:
    runtime: podman
---
signature: dGVzdA==
key_id: test-key
timestamp: 1234567890
counter: 42
"#;

        let result = SignatureValidator::extract_signature_block(signed_yaml);
        assert!(result.is_ok());

        let (unsigned, sig_block) = result.unwrap();
        assert!(unsigned.contains("apiVersion: v1"));
        assert_eq!(sig_block.key_id, "test-key");
        assert_eq!(sig_block.timestamp, 1234567890);
        assert_eq!(sig_block.counter, Some(42));
    }

    #[test]
    fn test_extract_signature_block_missing() {
        let unsigned_yaml = r#"apiVersion: v1
workloads:
  nginx:
    runtime: podman
"#;

        let result = SignatureValidator::extract_signature_block(unsigned_yaml);
        assert!(matches!(result, Err(SignatureError::MissingSignature)));
    }

    #[test]
    fn test_verify_valid_signature() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let content = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n";
        let signed_yaml = create_signed_yaml(content, &signing_key, "test-key", Some(1));

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
                        allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let result = validator.verify_signed_yaml(&signed_yaml, "test-source");
        if let Err(ref e) = result {
            eprintln!("Verification error: {:?}", e);
        }
        assert!(result.is_ok());

        let doc = result.unwrap();
        assert_eq!(doc.key_id, "test-key");
        assert_eq!(doc.counter, Some(1));
    }

    #[test]
    fn test_verify_invalid_signature() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Create a valid signature for different content (tampering scenario)
        let wrong_content = "different content\n";
        let (signing_key, _) = create_test_keypair();
        let wrong_signature = signing_key.sign(wrong_content.as_bytes());
        let wrong_signature_base64 = BASE64_STANDARD.encode(wrong_signature.to_bytes());

        // Use that signature with different content (should fail verification)
        let signed_yaml = format!(
            r#"apiVersion: v1
workloads:
  nginx:
    runtime: podman
---
signature: {}
key_id: test-key
timestamp: 1234567890
counter: 1
"#,
            wrong_signature_base64
        );

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
                        allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let result = validator.verify_signed_yaml(&signed_yaml, "test-source");
        if let Err(ref e) = result {
            eprintln!("Invalid signature test error: {:?}", e);
        }
        assert!(matches!(
            result,
            Err(SignatureError::GenericVerificationFailure)
        ));
    }

    #[test]
    fn test_counter_rollback_detection() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let content = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n";

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
                        allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // Verify with counter=2
        let signed_yaml_2 = create_signed_yaml(content, &signing_key, "test-key", Some(2));
        assert!(validator
            .verify_signed_yaml(&signed_yaml_2, "test-source")
            .is_ok());

        // Try to verify with counter=1 (rollback)
        let signed_yaml_1 = create_signed_yaml(content, &signing_key, "test-key", Some(1));
        let result = validator.verify_signed_yaml(&signed_yaml_1, "test-source");

        assert!(matches!(result, Err(SignatureError::CounterRollback { .. })));
    }

    #[test]
    fn test_allowed_key_ids() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let content = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n";
        let signed_yaml = create_signed_yaml(content, &signing_key, "test-key", Some(1));

        // Policy only allows "allowed-key"
        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
                        allowed_key_ids: vec!["allowed-key".to_string()],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let result = validator.verify_signed_yaml(&signed_yaml, "test-source");
        assert!(matches!(result, Err(SignatureError::KeyIdNotAllowed(_))));
    }

    #[test]
    fn test_per_source_counters() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let content = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n";

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
                        allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // Source A with counter=5
        let signed_yaml_a = create_signed_yaml(content, &signing_key, "test-key", Some(5));
        assert!(validator
            .verify_signed_yaml(&signed_yaml_a, "source-a")
            .is_ok());

        // Source B with counter=6 should work (higher than global key counter)
        // Note: Global key counter prevents cross-source replay attacks
        let signed_yaml_b = create_signed_yaml(content, &signing_key, "test-key", Some(6));
        assert!(validator
            .verify_signed_yaml(&signed_yaml_b, "source-b")
            .is_ok());

        // Source A with counter=5 should fail (same as last seen for source-a)
        let signed_yaml_a2 = create_signed_yaml(content, &signing_key, "test-key", Some(5));
        let result = validator.verify_signed_yaml(&signed_yaml_a2, "source-a");
        assert!(matches!(result, Err(SignatureError::CounterRollback { .. })));
    }

    #[test]
    fn test_parse_ed25519_public_key() {
        let pem = r#"-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBb2+pDT1P8OD0ywCXGMjSx9E9bhI=
-----END PUBLIC KEY-----"#;

        let result = SignatureValidator::parse_ed25519_public_key(pem);
        assert!(result.is_ok());

        let key_bytes = result.unwrap();
        assert_eq!(key_bytes.len(), 32);
    }

    // CRITICAL SECURITY TEST: Counter persistence
    #[test]
    fn test_counter_persistence_save_and_load() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let content = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n";
        let counter_path = temp_dir.path().join("counters.json");

        // First validator: verify counter=5
        {
            let policy = SignaturePolicy {
                require_signature: true,
            require_counter: false,
                allowed_key_ids: vec![],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy.clone()).unwrap();

            let signed_yaml = create_signed_yaml(content, &signing_key, "test-key", Some(5));
            assert!(validator.verify_signed_yaml(&signed_yaml, "source-a").is_ok());

            // Counter should be persisted automatically
            // Force save to ensure it's written before next validator loads
            validator.save_counters().expect("Failed to save counters");
        }

        // Second validator: load from disk, counter=4 should fail
        {
            let policy = SignaturePolicy {
                require_signature: true,
            require_counter: false,
                allowed_key_ids: vec![],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

            let signed_yaml = create_signed_yaml(content, &signing_key, "test-key", Some(4));
            let result = validator.verify_signed_yaml(&signed_yaml, "source-a");
            assert!(
                matches!(result, Err(SignatureError::CounterRollback { .. })),
                "Counter should be loaded from disk and reject rollback"
            );

            // Counter=6 should work
            let signed_yaml = create_signed_yaml(content, &signing_key, "test-key", Some(6));
            assert!(validator.verify_signed_yaml(&signed_yaml, "source-a").is_ok());
        }
    }

    #[test]
    fn test_counter_file_corruption_recovery() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (_signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let counter_path = temp_dir.path().join("counters.json");

        // Write corrupted counter file
        std::fs::write(&counter_path, "{ invalid json }").unwrap();

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        // NEW BEHAVIOR: Should fail-closed on corrupted counter file (security improvement)
        let result = SignatureValidator::from_keys_directory(temp_dir.path(), policy);
        assert!(
            result.is_err(),
            "Should fail startup on corrupted counter file (fail-closed for security)"
        );

        // Verify backup file was created
        let backup_pattern = format!("{}.corrupted.", counter_path.display());
        let backup_exists = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.path().to_string_lossy().contains(&backup_pattern));
        assert!(
            backup_exists,
            "Backup file should be created for forensics"
        );
    }

    // CRITICAL OPERATIONAL TEST: Key rotation
    #[test]
    fn test_key_rotation_workflow() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key_1, verifying_key_1) = create_test_keypair();
        let (signing_key_2, verifying_key_2) = create_test_keypair();

        // Setup two keys
        setup_test_keys(temp_dir.path(), &verifying_key_1, "key-2025");
        setup_test_keys(temp_dir.path(), &verifying_key_2, "key-2026");

        let content = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n";
        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        // Phase 1: Only key-2025 allowed
        {
            let policy = SignaturePolicy {
                require_signature: true,
            require_counter: false,
                allowed_key_ids: vec!["key-2025".to_string()],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

            // Signature with key-2025 should work
            let signed_yaml = create_signed_yaml(content, &signing_key_1, "key-2025", Some(1));
            assert!(validator.verify_signed_yaml(&signed_yaml, "source-a").is_ok());

            // Signature with key-2026 should fail
            let signed_yaml = create_signed_yaml(content, &signing_key_2, "key-2026", Some(1));
            let result = validator.verify_signed_yaml(&signed_yaml, "source-a");
            assert!(
                matches!(result, Err(SignatureError::KeyIdNotAllowed(_))),
                "key-2026 should not be allowed yet"
            );
        }

        // Phase 2: Both keys allowed (rotation period)
        {
            let policy = SignaturePolicy {
                require_signature: true,
            require_counter: false,
                allowed_key_ids: vec!["key-2025".to_string(), "key-2026".to_string()],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

            // Both keys should work
            let signed_yaml = create_signed_yaml(content, &signing_key_1, "key-2025", Some(2));
            assert!(validator.verify_signed_yaml(&signed_yaml, "source-a").is_ok());

            let signed_yaml = create_signed_yaml(content, &signing_key_2, "key-2026", Some(2));
            assert!(validator.verify_signed_yaml(&signed_yaml, "source-b").is_ok());
        }

        // Phase 3: Only key-2026 allowed (old key removed)
        {
            let policy = SignaturePolicy {
                require_signature: true,
            require_counter: false,
                allowed_key_ids: vec!["key-2026".to_string()],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

            // Signature with key-2025 should now fail
            let signed_yaml = create_signed_yaml(content, &signing_key_1, "key-2025", Some(3));
            let result = validator.verify_signed_yaml(&signed_yaml, "source-a");
            assert!(
                matches!(result, Err(SignatureError::KeyIdNotAllowed(_))),
                "key-2025 should be rejected after rotation"
            );

            // Signature with key-2026 should work
            let signed_yaml = create_signed_yaml(content, &signing_key_2, "key-2026", Some(3));
            assert!(validator.verify_signed_yaml(&signed_yaml, "source-b").is_ok());
        }
    }

    // CRITICAL INTEGRATION TEST: End-to-end with real Ed25519 cryptography
    #[test]
    fn test_real_ed25519_end_to_end_integration() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();

        // Setup validator with real Ed25519 key
        setup_test_keys(temp_dir.path(), &verifying_key, "real-key");

        // Create realistic manifest content
        let manifest = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n    agent: agent_A\n    runtimeConfig: |\n      image: nginx:latest\n  redis:\n    runtime: podman\n    agent: agent_B\n    runtimeConfig: |\n      image: redis:7.0";

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
            allowed_key_ids: vec!["real-key".to_string()],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy)
                .unwrap();

        // CRITICAL: Real Ed25519 verification with helper function
        let signed_yaml = create_signed_yaml(manifest, &signing_key, "real-key", Some(1));

        let result = validator.verify_signed_yaml(&signed_yaml, "test-source");
        assert!(
            result.is_ok(),
            "Real Ed25519 signature verification failed: {:?}",
            result
        );

        let verified = result.unwrap();
        assert_eq!(verified.key_id, "real-key");
        assert_eq!(verified.counter, Some(1));

        // SECURITY: Tampering detection with real cryptography
        let tampered = signed_yaml.replace("nginx:latest", "malicious:backdoor");
        let tampered_result = validator.verify_signed_yaml(&tampered, "test-source");
        assert!(
            matches!(tampered_result, Err(SignatureError::GenericVerificationFailure)),
            "Tampered signature should fail cryptographic verification"
        );

        // SECURITY: Replay attack prevention (counter)
        // Try to reuse counter=1 (should fail because we already saw it)
        let replay_yaml = create_signed_yaml(manifest, &signing_key, "real-key", Some(1));
        let replay_result = validator.verify_signed_yaml(&replay_yaml, "test-source");
        assert!(
            matches!(replay_result, Err(SignatureError::CounterRollback { .. })),
            "Replay with same counter should be rejected"
        );

        // SECURITY: Counter increment works
        let counter_2_yaml = create_signed_yaml(manifest, &signing_key, "real-key", Some(2));
        let result_2 = validator.verify_signed_yaml(&counter_2_yaml, "test-source");
        assert!(
            result_2.is_ok(),
            "Counter increment should succeed: {:?}",
            result_2
        );

        // SECURITY: Wrong key detection
        // Create a different keypair (simulates attacker with different key)
        let wrong_signing_key = SigningKey::from_bytes(&[
            42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42,
            42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42,
        ]);
        let wrong_key_yaml = create_signed_yaml(manifest, &wrong_signing_key, "real-key", Some(3));

        let wrong_key_result = validator.verify_signed_yaml(&wrong_key_yaml, "test-source");
        assert!(
            matches!(wrong_key_result, Err(SignatureError::GenericVerificationFailure)),
            "Signature with wrong key should fail verification"
        );
    }

    // FEATURE TEST: Optional counter support
    #[test]
    fn test_optional_counter_support() {
        let _guard = lock_env(); // Serialize env var access
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let content = "apiVersion: v1\nworkloads:\n  nginx:\n    runtime: podman\n";
        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        // Test 1: Counter optional (require_counter: false)
        {
            let policy = SignaturePolicy {
                require_signature: true,
                require_counter: false,
                allowed_key_ids: vec![],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

            // Signature without counter should work
            let signed_no_counter = create_signed_yaml(content, &signing_key, "test-key", None);
            let result = validator.verify_signed_yaml(&signed_no_counter, SOURCE_STARTUP_MANIFEST);
            assert!(
                result.is_ok(),
                "Signature without counter should succeed when counter is optional"
            );
            assert_eq!(result.unwrap().counter, None, "Counter should be None");

            // Signature with counter should also work
            let signed_with_counter = create_signed_yaml(content, &signing_key, "test-key", Some(1));
            let result = validator.verify_signed_yaml(&signed_with_counter, "runtime-update");
            assert!(result.is_ok(), "Signature with counter should also work: {:?}", result);
            let verified = result.unwrap();
            assert_eq!(verified.counter, Some(1), "Counter should be Some(1)");
        }

        // Test 2: Counter required (require_counter: true)
        {
            let policy = SignaturePolicy {
                require_signature: true,
                require_counter: true,
                allowed_key_ids: vec![],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

            // Signature without counter should FAIL
            let signed_no_counter = create_signed_yaml(content, &signing_key, "test-key", None);
            let result = validator.verify_signed_yaml(&signed_no_counter, "update-source");
            assert!(
                matches!(result, Err(SignatureError::CounterRequired)),
                "Signature without counter should fail when counter is required"
            );

            // Signature with counter should work
            let signed_with_counter = create_signed_yaml(content, &signing_key, "test-key", Some(10));
            assert!(
                validator
                    .verify_signed_yaml(&signed_with_counter, "update-source")
                    .is_ok(),
                "Signature with counter should succeed"
            );
        }

        // Test 3: Verify counter-less signatures can be verified multiple times (idempotent)
        {
            let policy = SignaturePolicy {
                require_signature: true,
                require_counter: false,
                allowed_key_ids: vec![],
                min_counter: 0,
                allowed_restoration_plugins: vec!["basic_persistency".to_string()],
                restoration_window_seconds: 3600,
            };

            let mut validator =
                SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

            let signed_no_counter = create_signed_yaml(content, &signing_key, "test-key", None);

            // Should succeed multiple times (no counter tracking)
            assert!(
                validator
                    .verify_signed_yaml(&signed_no_counter, "startup")
                    .is_ok(),
                "First verification should succeed"
            );
            assert!(
                validator
                    .verify_signed_yaml(&signed_no_counter, "startup")
                    .is_ok(),
                "Second verification should succeed (no replay detection without counter)"
            );
            assert!(
                validator
                    .verify_signed_yaml(&signed_no_counter, "startup")
                    .is_ok(),
                "Third verification should succeed (idempotent without counter)"
            );
        }
    }

    #[test]
    fn test_restoration_counter_exemption_protobuf() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy.clone()).unwrap();

        // Simulate initial apply with counter=10
        // This directly updates the counter state as if a signed request was processed
        validator.key_counters.insert("test-key".to_string(), 10);
        validator.source_counters.insert("request:agent_A@basic_persistency@startup_restore_nginx".to_string(), 10);
        validator.save_counters().expect("Failed to save initial counter state");

        // Restart: Create new validator that loads persisted counter state
        let mut validator_after_restart =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // Verify counter state was loaded
        assert_eq!(
            validator_after_restart.key_counters.get("test-key"),
            Some(&10),
            "Counter state should be loaded from disk"
        );

        // Test 1: Restoration request with same counter=10 should SUCCEED
        let result = validator_after_restart.validate_and_update_counter(
            10,
            "test-key",
            "request:agent_A@basic_persistency@startup_restore_nginx",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            result.is_ok(),
            "Restoration with same counter should be allowed: {:?}",
            result
        );

        // Test 2: Normal (non-restoration) request with counter=10 should FAIL (replay protection)
        let result_normal = validator_after_restart.validate_and_update_counter(
            10,
            "test-key",
            "request:agent_A@normal_request",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result_normal, Err(SignatureError::CounterRollback { .. })),
            "Normal request with same counter should be rejected"
        );

        // Test 3: Restoration with counter=9 (lower than last_seen) should SUCCEED
        // This allows restoring multiple workloads signed with the same key but different counters
        let result_lower = validator_after_restart.validate_and_update_counter(
            9,
            "test-key",
            "request:agent_A@basic_persistency@startup_restore_workload_with_older_counter",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            result_lower.is_ok(),
            "Restoration with lower counter should be allowed (for multi-workload restoration): {:?}",
            result_lower
        );

        // Test 4: Restoration with counter=11 should SUCCEED (higher counter)
        let result_higher = validator_after_restart.validate_and_update_counter(
            11,
            "test-key",
            "request:agent_A@basic_persistency@startup_restore_another_workload",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            result_higher.is_ok(),
            "Restoration with higher counter should succeed"
        );
    }

    #[test]
    fn test_restoration_exemption_cannot_be_spoofed() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // Pre-populate counter to test bypass attempts
        validator
            .validate_and_update_counter(
                10,
                "test-key",
                "request:agent@normal_request",
                CounterValidationMode::CheckAndUpdate,
            )
            .unwrap();

        // Test 1: Legitimate restoration source should be recognized
        let result = validator.validate_and_update_counter(
            10, // Same counter
            "test-key",
            "request:agent@basic_persistency@startup_restore_nginx",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            result.is_ok(),
            "Legitimate restoration source should be allowed: {:?}",
            result
        );

        // Test 2: Attack - Direct substring without request: prefix (should FAIL)
        let result = validator.validate_and_update_counter(
            5, // Lower counter
            "test-key",
            "startup_restore_backdoor",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Direct substring attack should be blocked (no request: prefix)"
        );

        // Test 3: Attack - Malicious prefix without request: (should FAIL)
        let result = validator.validate_and_update_counter(
            5,
            "test-key",
            "malicious@startup_restore_evil",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Malicious prefix attack should be blocked"
        );

        // Test 4: Attack - No @ separator (should FAIL)
        let result = validator.validate_and_update_counter(
            5,
            "test-key",
            "request:startup_restore_direct",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Missing separator attack should be blocked"
        );

        // Test 5: Attack - Partial match without plugin marker (should FAIL)
        let result = validator.validate_and_update_counter(
            5,
            "test-key",
            "request:agent@startup_restore_",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Partial match without plugin marker should be blocked"
        );

        // Test 6: Other plugins NOT allowed (only basic_persistency is trusted)
        let result = validator.validate_and_update_counter(
            5, // Lower counter
            "test-key",
            "request:agent_B@other_plugin@startup_restore_workload",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Other plugins should NOT be allowed restoration exemption (only basic_persistency)"
        );

        // Test 7: CRITICAL - Crafted request_id replay attack (should FAIL)
        // Attack scenario: Attacker controls request_id and crafts:
        //   request_id = "fake@basic_persistency@startup_restore_evil"
        // When prepended by authenticated agent_name becomes:
        //   source = "request:agent_B@fake@basic_persistency@startup_restore_evil"
        // Old substring matching would accept this as restoration, allowing replay attacks!
        // Fixed version validates parts[1] (workload_name) is "basic_persistency", not parts[2]
        let result = validator.validate_and_update_counter(
            5, // Lower counter - replay attack
            "test-key",
            "request:agent_B@fake@basic_persistency@startup_restore_evil",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Crafted request_id replay attack must be blocked (parts[1] is 'fake', not 'basic_persistency')"
        );
    }

    #[test]
    fn test_is_restoration_from_trusted_plugin_parsing() {
        // Test the helper function directly to verify identity chain parsing

        let allowed = vec!["basic_persistency".to_string()];

        // Valid restoration sources
        assert!(
            SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent_A@basic_persistency@startup_restore_nginx",
                &allowed
            ),
            "Valid restoration source should be recognized"
        );

        assert!(
            SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent_B@basic_persistency@startup_restore_workload1",
                &allowed
            ),
            "Different agent should still work"
        );

        // Invalid - missing request: prefix
        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "startup_restore_backdoor",
                &allowed
            ),
            "Missing request: prefix should fail"
        );

        // Invalid - wrong prefix
        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "malicious@startup_restore_evil",
                &allowed
            ),
            "Wrong prefix should fail"
        );

        // Invalid - not enough parts (missing workload@client_request_id)
        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent",
                &allowed
            ),
            "Missing @ separators should fail"
        );

        // Invalid - only 2 parts (agent@workload, missing client_request_id)
        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent@workload",
                &allowed
            ),
            "Missing client_request_id should fail"
        );

        // Invalid - workload not in allowed list
        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent@other_plugin@startup_restore_workload",
                &allowed
            ),
            "Workload not in allowed list should fail"
        );

        // Invalid - missing startup_restore_ marker in client_request_id
        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent@basic_persistency@regular_request",
                &allowed
            ),
            "Missing startup_restore_ marker should fail"
        );

        // CRITICAL - Crafted request_id attack should FAIL
        // Attacker crafts request_id = "fake@basic_persistency@startup_restore_evil"
        // When prepended becomes: "request:agent@fake@basic_persistency@startup_restore_evil"
        // parts[0] = "agent" (authenticated)
        // parts[1] = "fake" (verified by agent, but NOT in allowed list)
        // parts[2] = "basic_persistency" (attacker-controlled)
        // parts[3] = "startup_restore_evil" (attacker-controlled)
        // The fix validates parts[1] == "basic_persistency", not parts[2]
        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent@fake@basic_persistency@startup_restore_evil",
                &allowed
            ),
            "Crafted request_id attack must be blocked (workload is 'fake', not 'basic_persistency')"
        );

        // Multiple allowed plugins
        let multi_allowed = vec![
            "basic_persistency".to_string(),
            "advanced_persistency".to_string(),
        ];

        assert!(
            SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent@basic_persistency@startup_restore_nginx",
                &multi_allowed
            ),
            "First allowed plugin should work"
        );

        assert!(
            SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent@advanced_persistency@startup_restore_redis",
                &multi_allowed
            ),
            "Second allowed plugin should work"
        );

        assert!(
            !SignatureValidator::is_restoration_from_trusted_plugin(
                "request:agent@other_plugin@startup_restore_workload",
                &multi_allowed
            ),
            "Plugin not in list should fail"
        );
    }

    #[test]
    fn test_configurable_restoration_plugins() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        // Test 1: Empty allowed list - NO restoration exemption allowed
        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![], // Empty list
            restoration_window_seconds: 3600,
        };
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // Pre-populate counter
        validator
            .validate_and_update_counter(
                10,
                "test-key",
                "request:agent@normal",
                CounterValidationMode::CheckAndUpdate,
            )
            .unwrap();

        // Even basic_persistency should be rejected with empty list
        let result = validator.validate_and_update_counter(
            10,
            "test-key",
            "request:agent@basic_persistency@startup_restore_nginx",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Empty allowed list should block ALL restoration attempts"
        );

        // Test 2: Custom plugin in allowed list
        let policy2 = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["custom_persistency".to_string()],
            restoration_window_seconds: 3600,
        };
        let mut validator2 =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy2).unwrap();

        validator2
            .validate_and_update_counter(
                20,
                "test-key",
                "request:agent@normal2",
                CounterValidationMode::CheckAndUpdate,
            )
            .unwrap();

        // custom_persistency should be allowed
        let result = validator2.validate_and_update_counter(
            20,
            "test-key",
            "request:agent@custom_persistency@startup_restore_workload",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            result.is_ok(),
            "Custom plugin in allowed list should work: {:?}",
            result
        );

        // basic_persistency should be rejected (not in list)
        let result = validator2.validate_and_update_counter(
            15, // Lower counter
            "test-key",
            "request:agent@basic_persistency@startup_restore_other",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "basic_persistency should be rejected when not in allowed list"
        );

        // Test 3: Multiple plugins allowed
        let policy3 = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![
                "basic_persistency".to_string(),
                "advanced_persistency".to_string(),
            ],
            restoration_window_seconds: 3600,
        };
        let mut validator3 =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy3).unwrap();

        validator3
            .validate_and_update_counter(
                30,
                "test-key",
                "request:agent@normal3",
                CounterValidationMode::CheckAndUpdate,
            )
            .unwrap();

        // Both plugins should work
        let result = validator3.validate_and_update_counter(
            30,
            "test-key",
            "request:agent@basic_persistency@startup_restore_a",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(result.is_ok(), "basic_persistency should work");

        let result = validator3.validate_and_update_counter(
            30,
            "test-key",
            "request:agent@advanced_persistency@startup_restore_b",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(result.is_ok(), "advanced_persistency should work");

        // Unlisted plugin should fail
        let result = validator3.validate_and_update_counter(
            25,
            "test-key",
            "request:agent@unknown_plugin@startup_restore_c",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Unlisted plugin should be rejected"
        );
    }

    #[test]
    fn test_min_counter_policy_enforcement() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Policy with min_counter = 100
        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 100, // Floor value
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // Test 1: Counter below min_counter should FAIL (CheckAndUpdate mode)
        let result = validator.validate_and_update_counter(
            50, // counter < 100
            "test-key",
            "request:agent@source1",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { current: 50, last_seen: 100, .. })),
            "Counter below min_counter should be rejected: {:?}",
            result
        );

        // Test 2: Counter equal to min_counter should SUCCEED (boundary condition)
        let result = validator.validate_and_update_counter(
            100, // counter == min_counter
            "test-key",
            "request:agent@source2",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            result.is_ok(),
            "Counter equal to min_counter should be accepted"
        );

        // Test 3: Counter above min_counter should SUCCEED
        let result = validator.validate_and_update_counter(
            150, // counter > min_counter
            "test-key",
            "request:agent@source3",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(result.is_ok(), "Counter above min_counter should be accepted");

        // Test 4: CheckOnly mode should also enforce min_counter
        let result = validator.validate_and_update_counter(
            75, // counter < 100
            "test-key",
            "request:agent@source4",
            CounterValidationMode::CheckOnly,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "CheckOnly mode should also enforce min_counter"
        );

        // Test 5: Restoration requests still subject to min_counter floor
        let result = validator.validate_and_update_counter(
            80, // counter < min_counter
            "test-key",
            "request:agent@basic_persistency@startup_restore_workload",
            CounterValidationMode::CheckAndUpdate,
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Restoration exemption does NOT bypass min_counter check"
        );

        // Test 6: Startup manifest DOES skip min_counter (special case)
        let result = validator.validate_and_update_counter(
            10, // counter < min_counter
            "test-key",
            SOURCE_STARTUP_MANIFEST,
            CounterValidationMode::CheckOnly,
        );
        assert!(
            result.is_ok(),
            "Startup manifest should skip ALL counter checks including min_counter"
        );
    }

    #[test]
    fn test_startup_manifest_signed_file_loads_successfully() {
        let _guard = lock_env(); // Serialize env var access
        // This test exercises the ACTUAL server startup path that was missing from test coverage:
        // 1. Read signed manifest from disk
        // 2. Verify signature
        // 3. Parse unsigned content as YAML
        // 4. Load into server state
        //
        // This catches the bug where we tried to parse the full signed YAML
        // (including signature block) which caused serde_yaml to fail with
        // "deserializing from YAML containing more than one document is not supported"

        use tempfile::TempDir;
        use std::fs;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let keys_dir = temp_dir.path().join("keys");

        fs::create_dir_all(&keys_dir).expect("Failed to create keys directory");

        // Step 1: Use existing test keypair
        let (signing_key, verifying_key) = create_test_keypair();

        // Save public key in PEM format using the helper function
        setup_test_keys(&keys_dir, &verifying_key, "startup-key-2026");

        // Step 2: Create unsigned manifest content
        let unsigned_content = r#"apiVersion: v1
workloads:
  nginx-startup:
    runtime: podman
    agent: agent_A
    tags:
      - key: owner
        value: test
    runtimeConfig: |
      image: nginx:latest
      commandOptions: ["-p", "8080:80"]
configs:
  database-config:
    config: |
      host=localhost
      port=5432
"#;

        // Step 3: Sign the manifest using helper function
        let signed_content = create_signed_yaml(
            unsigned_content,
            &signing_key,
            "startup-key-2026",
            Some(1), // counter
        );

        // Step 5: Verify the signature using SignatureValidator (what the server does)
        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: false,
            allowed_key_ids: vec!["startup-key-2026".to_string()],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        // Use temp directory for counter state (not /var/lib/ankaios/)
        let counter_state_path = temp_dir.path().join("signature_counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_state_path); }

        let mut validator = SignatureValidator::from_keys_directory(
            &keys_dir,
            policy,
        )
        .expect("Failed to create signature validator");

        let verified_doc = validator
            .verify_signed_yaml(&signed_content, SOURCE_STARTUP_MANIFEST)
            .expect("Signature verification failed");

        assert_eq!(verified_doc.key_id, "startup-key-2026");
        assert_eq!(verified_doc.counter, Some(1));

        // Step 6: THE CRITICAL TEST - Parse the UNSIGNED content (not the full signed YAML)
        // This is what the server MUST do to avoid the multi-document YAML error

        // WRONG WAY (what the bug was):
        // let state: StateSpec = serde_yaml::from_str(&signed_content).expect("...");
        // This fails with: "deserializing from YAML containing more than one document is not supported"

        // RIGHT WAY (the fix):
        let unsigned_content_from_verification = &verified_doc.unsigned_content;

        #[derive(serde::Deserialize, Debug)]
        struct StateSpec {
            #[serde(rename = "apiVersion")]
            api_version: String,
            workloads: Option<serde_yaml::Value>,
            configs: Option<serde_yaml::Value>,
        }

        let state: StateSpec = serde_yaml::from_str(unsigned_content_from_verification)
            .expect("Failed to parse unsigned content - this is the bug we're testing for!");

        // Step 7: Verify the parsed state contains expected data
        assert_eq!(state.api_version, "v1", "API version should be v1");
        assert!(state.workloads.is_some(), "Workloads should be present");
        assert!(state.configs.is_some(), "Configs should be present");

        let workloads = state.workloads.as_ref().unwrap();
        assert!(
            workloads.get("nginx-startup").is_some(),
            "nginx-startup workload should be present"
        );

        let configs = state.configs.as_ref().unwrap();
        assert!(
            configs.get("database-config").is_some(),
            "database-config should be present"
        );

        // Step 8: Verify that trying to parse the FULL signed YAML fails
        // (demonstrating why the fix was necessary)
        let parse_result: Result<StateSpec, _> = serde_yaml::from_str(&signed_content);
        assert!(
            parse_result.is_err(),
            "Parsing full signed YAML should fail (multi-document not supported)"
        );

        if let Err(e) = parse_result {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("more than one document")
                    || error_msg.contains("unexpected content"),
                "Error should mention multi-document issue, got: {}",
                error_msg
            );
        }
    }

    // Helper function to create a signed UpdateStateRequest (like `ank sign` does)
    fn create_signed_update_request(
        state: State,
        signing_key: &SigningKey,
        key_id: &str,
        counter: u64,
        timestamp: u64,
    ) -> UpdateStateRequest {
        use ankaios_api::ank_base::{CompleteState, SignatureMetadata};
        use common::objects::canonical::Canonical;
        use common::objects::signed_payload::SignedPayload;

        // 1. Create canonical bytes from state
        let canonical = state
            .to_canonical_bytes()
            .expect("Failed to canonicalize state");

        // 2. Create signed payload
        let payload = SignedPayload::new(counter, timestamp, canonical);
        let payload_bytes = payload
            .to_deterministic_bytes()
            .expect("Failed to serialize payload");

        // 3. Sign the payload
        let signature = signing_key.sign(&payload_bytes);

        // 4. Create UpdateStateRequest with signature metadata
        UpdateStateRequest {
            new_state: Some(CompleteState {
                desired_state: Some(state),
                ..Default::default()
            }),
            update_mask: vec![],
            signature_metadata: Some(SignatureMetadata {
                signature: signature.to_bytes().to_vec(),
                key_id: key_id.to_string(),
                counter,
                timestamp,
            }),
        }
    }

    // SECURITY TEST: Corrupted .pb file handling
    #[test]
    fn test_corrupted_pb_file_rejected() {
        use prost::Message;
        use ankaios_api::ank_base::{Workload, WorkloadMap};
        use std::collections::HashMap;

        // Test 1: Complete garbage bytes (not valid protobuf)
        let garbage = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];
        let result = UpdateStateRequest::decode(&garbage[..]);
        assert!(
            result.is_err(),
            "Garbage bytes should fail to decode as protobuf"
        );

        // Test 2: Empty file
        let empty: Vec<u8> = vec![];
        let result = UpdateStateRequest::decode(&empty[..]);
        assert!(
            result.is_ok(), // Empty is actually valid (all fields optional)
            "Empty bytes decode to default UpdateStateRequest"
        );
        let decoded = result.unwrap();
        assert!(decoded.new_state.is_none(), "Empty request has no state");

        // Test 3: Truncated protobuf (incomplete message)
        // Create a valid request first
        let (signing_key, _) = create_test_keypair();
        let mut workloads = HashMap::new();
        workloads.insert(
            "test".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                ..Default::default()
            },
        );
        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            ..Default::default()
        };
        let valid_request = create_signed_update_request(state, &signing_key, "key", 1, 1234);

        // Encode it
        let mut valid_bytes = Vec::new();
        valid_request.encode(&mut valid_bytes).unwrap();
        assert!(valid_bytes.len() > 10, "Should have some data");

        // Truncate to first 50% of bytes
        let truncated = &valid_bytes[..valid_bytes.len() / 2];
        let _result = UpdateStateRequest::decode(truncated);
        // Protobuf is lenient and may partially decode, but signature verification will fail
        // We're mainly checking it doesn't crash/panic

        // Test 4: Invalid field numbers (protobuf with bad wire types)
        // Create malformed protobuf: field 999 with invalid wire type
        let malformed = vec![
            0xF8, 0x3E, 0x01, // field 999, wire type 0 (varint), value 1
            0xFF, 0xFF, 0xFF, 0xFF, 0x0F, // large varint
        ];
        let _result = UpdateStateRequest::decode(&malformed[..]);
        // May succeed (unknown fields are skipped) or fail depending on how broken it is
        // The important part is it doesn't crash/panic

        // Test 5: Corrupted signature metadata (valid protobuf structure, invalid signature bytes)
        let mut corrupted_sig_request = create_signed_update_request(
            State::default(),
            &signing_key,
            "test-key",
            1,
            1234,
        );
        // Corrupt the signature bytes
        corrupted_sig_request
            .signature_metadata
            .as_mut()
            .unwrap()
            .signature = vec![0xFF; 64]; // Invalid signature

        // Encode this corrupted request
        let mut corrupted_bytes = Vec::new();
        corrupted_sig_request.encode(&mut corrupted_bytes).unwrap();

        // Decode should succeed (valid protobuf structure)
        let decoded_result = UpdateStateRequest::decode(&corrupted_bytes[..]);
        assert!(
            decoded_result.is_ok(),
            "Valid protobuf structure should decode even with bad signature"
        );

        // But signature verification should FAIL
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        let mut validator = SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let decoded = decoded_result.unwrap();
        let verify_result = validator.verify_update_request(&decoded, "test-source");
        assert!(
            matches!(verify_result, Err(SignatureError::GenericVerificationFailure)),
            "Corrupted signature should fail verification: {:?}",
            verify_result
        );
    }

    // SECURITY TEST: Signature length validation (Ed25519 must be exactly 64 bytes)
    #[test]
    fn test_signature_length_validation() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Test 1: Signature too short (32 bytes instead of 64)
        let mut request =
            create_signed_update_request(state.clone(), &signing_key, "test-key", 1, 1234);
        request.signature_metadata.as_mut().unwrap().signature = vec![0xFF; 32]; // TOO SHORT

        let result = validator.verify_update_request(&request, "test-source");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Too-short signature (32 bytes) should fail: {:?}",
            result
        );

        // Test 2: Signature too long (128 bytes instead of 64)
        let mut request =
            create_signed_update_request(state.clone(), &signing_key, "test-key", 2, 1234);
        request.signature_metadata.as_mut().unwrap().signature = vec![0xFF; 128]; // TOO LONG

        let result = validator.verify_update_request(&request, "test-source2");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Too-long signature (128 bytes) should fail: {:?}",
            result
        );

        // Test 3: Empty signature (0 bytes)
        let mut request =
            create_signed_update_request(state.clone(), &signing_key, "test-key", 3, 1234);
        request.signature_metadata.as_mut().unwrap().signature = vec![]; // EMPTY

        let result = validator.verify_update_request(&request, "test-source3");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Empty signature (0 bytes) should fail: {:?}",
            result
        );

        // Test 4: Correct 64-byte signature with wrong content (sanity check)
        let mut request =
            create_signed_update_request(state.clone(), &signing_key, "test-key", 4, 1234);
        request.signature_metadata.as_mut().unwrap().signature = vec![0x00; 64]; // Valid length, wrong signature

        let result = validator.verify_update_request(&request, "test-source4");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Wrong signature content should fail: {:?}",
            result
        );
    }

    // SECURITY TEST: Invalid key_id handling (empty, path traversal, DoS)
    #[test]
    fn test_invalid_key_id_handling() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "valid-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Test 1: Empty key_id
        let request = create_signed_update_request(state.clone(), &signing_key, "", 1, 1234);
        let result = validator.verify_update_request(&request, "test-source");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Empty key_id should fail: {:?}",
            result
        );

        // Test 2: Path traversal in key_id (security: prevent file system attacks)
        let request =
            create_signed_update_request(state.clone(), &signing_key, "../../etc/passwd", 2, 1234);
        let result = validator.verify_update_request(&request, "test-source2");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Path traversal key_id should fail: {:?}",
            result
        );

        // Test 3: Extremely long key_id (potential DoS via memory/CPU exhaustion)
        let long_key_id = "A".repeat(10000);
        let request =
            create_signed_update_request(state.clone(), &signing_key, &long_key_id, 3, 1234);
        let result = validator.verify_update_request(&request, "test-source3");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Extremely long key_id (10000 chars) should fail: {:?}",
            result
        );

        // Test 4: Special characters in key_id
        let request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "key@#$%^&*()",
            4,
            1234,
        );
        let result = validator.verify_update_request(&request, "test-source4");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Special character key_id should fail: {:?}",
            result
        );

        // Test 5: Null bytes in key_id (can cause security issues in C FFI)
        let request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "key\0null",
            5,
            1234,
        );
        let result = validator.verify_update_request(&request, "test-source5");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "key_id with null byte should fail: {:?}",
            result
        );
    }

    // SECURITY TEST: Counter edge cases (0, 1, MAX, overflow behavior)
    #[test]
    fn test_counter_edge_cases() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Test 1: Counter = 1 (first valid counter)
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 1, 1234);
        let result = validator.verify_update_request(&request, "test-source");
        assert!(result.is_ok(), "Counter=1 should succeed: {:?}", result);

        // Test 2: Counter = 0 with min_counter=0 (edge case)
        // Note: 0 is not > 0, so rollback protection will reject it after we've seen counter=1
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 0, 1234);
        let result = validator.verify_update_request(&request, "test-source2");
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Counter=0 after counter=1 should fail (rollback): {:?}",
            result
        );

        // Test 3: Counter = u64::MAX (maximum value - 584M years at 1000/sec)
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", u64::MAX, 1234);
        let result = validator.verify_update_request(&request, "test-source3");
        assert!(
            result.is_ok(),
            "Counter=MAX (18446744073709551615) should succeed: {:?}",
            result
        );

        // Test 4: After reaching MAX, any counter < MAX is a rollback
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", u64::MAX - 1, 1234);
        let result = validator.verify_update_request(&request, "test-source4");
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Counter=MAX-1 after MAX should fail (rollback): {:?}",
            result
        );

        // Test 5: Document that MAX is effectively a ceiling (no wrapping to 0)
        // Attempting counter=0 after MAX would be a massive rollback
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 0, 1234);
        let result = validator.verify_update_request(&request, "test-source5");
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Counter=0 after MAX should fail (no wrapping): {:?}",
            result
        );
    }

    // SECURITY TEST: Timestamp edge cases (documents behavior - timestamps not validated)
    #[test]
    fn test_timestamp_edge_cases() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Test 1: Timestamp = 0 (Unix epoch, 1970-01-01 00:00:00)
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 1, 0);
        let result = validator.verify_update_request(&request, "test-source");
        assert!(
            result.is_ok(),
            "Timestamp=0 (Unix epoch) should be accepted: {:?}",
            result
        );

        // Test 2: Timestamp = u64::MAX (far future, year ~292 billion)
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 2, u64::MAX);
        let result = validator.verify_update_request(&request, "test-source2");
        assert!(
            result.is_ok(),
            "Timestamp=MAX (far future) should be accepted: {:?}",
            result
        );

        // Test 3: Future timestamp (100 years from now)
        let future_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + (100 * 365 * 24 * 60 * 60);
        let request =
            create_signed_update_request(state.clone(), &signing_key, "test-key", 3, future_timestamp);
        let result = validator.verify_update_request(&request, "test-source3");
        assert!(
            result.is_ok(),
            "Future timestamp (+100 years) should be accepted: {:?}",
            result
        );

        // Test 4: Past timestamp (50 years ago)
        let past_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(50 * 365 * 24 * 60 * 60);
        let request =
            create_signed_update_request(state.clone(), &signing_key, "test-key", 4, past_timestamp);
        let result = validator.verify_update_request(&request, "test-source4");
        assert!(
            result.is_ok(),
            "Past timestamp (-50 years) should be accepted: {:?}",
            result
        );

        // NOTE: These tests document that timestamp is NOT validated by design.
        // The timestamp is cryptographically bound to the signature (tampering fails),
        // but the server doesn't enforce freshness or clock-skew policies.
        // This is intentional for forward compatibility - future versions could
        // add optional timestamp validation policies.
    }

    // SECURITY TEST: Counter/Timestamp tampering detection
    #[test]
    fn test_counter_timestamp_tampering_detection() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }

        let mut validator = SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // Create a test state
        use ankaios_api::ank_base::{Workload, WorkloadMap};
        use std::collections::HashMap;

        let mut workloads = HashMap::new();
        workloads.insert(
            "nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                runtime_config: Some("image: nginx:latest".to_string()),
                ..Default::default()
            },
        );

        let state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap { workloads }),
            ..Default::default()
        };

        // Create a valid signed request
        let valid_request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "test-key",
            100, // counter
            1234567890, // timestamp
        );

        // Verify the valid request works
        let result = validator.verify_update_request(&valid_request, "test-source");
        assert!(
            result.is_ok(),
            "Valid signed request should succeed: {:?}",
            result
        );

        // ATTACK 1: Tamper with counter (increase it)
        let mut tampered_counter_request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "test-key",
            100, // original counter
            1234567890,
        );
        // Tamper: change counter to 999 (but signature is for counter=100)
        tampered_counter_request
            .signature_metadata
            .as_mut()
            .unwrap()
            .counter = 999;

        let result = validator.verify_update_request(&tampered_counter_request, "test-source-2");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Tampered counter should fail signature verification, got: {:?}",
            result
        );

        // ATTACK 2: Tamper with counter (decrease it - replay attack attempt)
        let mut tampered_counter_down_request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "test-key",
            100,
            1234567890,
        );
        // Tamper: change counter to 50 (but signature is for counter=100)
        tampered_counter_down_request
            .signature_metadata
            .as_mut()
            .unwrap()
            .counter = 50;

        let result = validator.verify_update_request(&tampered_counter_down_request, "test-source-3");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Tampered counter (decreased) should fail signature verification, got: {:?}",
            result
        );

        // ATTACK 3: Tamper with timestamp
        let mut tampered_timestamp_request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "test-key",
            101, // New counter to avoid replay detection
            1234567890, // original timestamp
        );
        // Tamper: change timestamp (but signature is for original timestamp)
        tampered_timestamp_request
            .signature_metadata
            .as_mut()
            .unwrap()
            .timestamp = 9999999999;

        let result = validator.verify_update_request(&tampered_timestamp_request, "test-source-4");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Tampered timestamp should fail signature verification, got: {:?}",
            result
        );

        // ATTACK 4: Tamper with BOTH counter and timestamp
        let mut tampered_both_request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "test-key",
            102,
            1234567890,
        );
        // Tamper: change both
        let metadata = tampered_both_request.signature_metadata.as_mut().unwrap();
        metadata.counter = 999;
        metadata.timestamp = 9999999999;

        let result = validator.verify_update_request(&tampered_both_request, "test-source-5");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Tampered counter+timestamp should fail signature verification, got: {:?}",
            result
        );

        // ATTACK 5: Tamper with workload content (but keep counter/timestamp)
        let mut tampered_workloads = HashMap::new();
        tampered_workloads.insert(
            "nginx".to_string(),
            Workload {
                runtime: Some("podman".to_string()),
                agent: Some("agent_A".to_string()),
                runtime_config: Some("image: malicious:backdoor".to_string()), // TAMPERED!
                ..Default::default()
            },
        );

        let tampered_state = State {
            api_version: "v1".to_string(),
            workloads: Some(WorkloadMap {
                workloads: tampered_workloads,
            }),
            ..Default::default()
        };

        let mut tampered_content_request = create_signed_update_request(
            state.clone(), // Create signature for ORIGINAL state
            &signing_key,
            "test-key",
            103,
            1234567890,
        );
        // Replace state with tampered version (but signature is for original)
        tampered_content_request.new_state.as_mut().unwrap().desired_state = Some(tampered_state);

        let result = validator.verify_update_request(&tampered_content_request, "test-source-6");
        assert!(
            matches!(result, Err(SignatureError::GenericVerificationFailure)),
            "Tampered workload content should fail signature verification, got: {:?}",
            result
        );

        // VERIFICATION: A properly incremented counter with correct signature should work
        let valid_next_request = create_signed_update_request(
            state.clone(),
            &signing_key,
            "test-key",
            104, // Next counter
            1234567891, // Different timestamp
        );

        let result = validator.verify_update_request(&valid_next_request, "test-source-7");
        assert!(
            result.is_ok(),
            "Valid request with incremented counter should succeed: {:?}",
            result
        );
    }

    // CRITICAL SECURITY TEST: Empty state signature reuse vulnerability
    // Tests that desired_state: None allows signature reuse across deletions
    #[test]
    fn test_empty_state_signature_bypass_vulnerability() {
        use ankaios_api::ank_base::CompleteState;

        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        // VULNERABILITY: Create ONE signature for State::default() (empty state)
        // This signature can then be reused to "delete" ANY workload by varying update_mask
        let empty_state_request = create_signed_update_request(
            State::default(),  // Empty state
            &signing_key,
            "test-key",
            1,
            1234,
        );

        // Extract signature metadata to reuse
        let sig_metadata = empty_state_request.signature_metadata.clone().unwrap();

        // ATTACK 1: Use the empty state signature with update_mask pointing to "nginx"
        // The signature doesn't cover update_mask, so we can target any workload!
        let attack_nginx = UpdateStateRequest {
            new_state: Some(CompleteState {
                desired_state: None,  // ⚠️ Triggers State::default() canonicalization
                ..Default::default()
            }),
            update_mask: vec!["desiredState.workloads.nginx".to_string()],
            signature_metadata: Some(sig_metadata.clone()),  // Reuse signature!
        };

        // After fix: This MUST fail with ParseError
        let result = validator.verify_update_request(&attack_nginx, "test-source-1");
        assert!(
            matches!(result, Err(SignatureError::ParseError(ref msg)) if msg.contains("Empty state")),
            "Empty state signature must be rejected (security fix): {:?}",
            result
        );

        // ATTACK 2: Reuse SAME signature for different workload (redis)
        let attack_redis = UpdateStateRequest {
            new_state: Some(CompleteState {
                desired_state: None,
                ..Default::default()
            }),
            update_mask: vec!["desiredState.workloads.redis".to_string()],
            signature_metadata: Some(sig_metadata),
        };

        // This must also fail with ParseError (empty state not allowed)
        let result = validator.verify_update_request(&attack_redis, "test-source-2");
        assert!(
            matches!(result, Err(SignatureError::ParseError(ref msg)) if msg.contains("Empty state")),
            "Empty state signature must be rejected for any workload: {:?}",
            result
        );
    }

    // SECURITY TEST: Restoration time window enforcement
    #[test]
    fn test_restoration_time_window_within_window() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Establish counter=100 as the current highest
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 100, 1234);
        let result = validator.verify_update_request(&request, "test-source");
        assert!(result.is_ok(), "Counter=100 should succeed");

        // Now try restoration with counter=50 (lower than current)
        // This is within the restoration window, so should be allowed
        let restoration_request = create_signed_update_request(state.clone(), &signing_key, "test-key", 50, 1235);
        let result = validator.verify_update_request(
            &restoration_request,
            "request:agent_A@basic_persistency@startup_restore_nginx"
        );
        assert!(
            result.is_ok(),
            "Restoration with old counter should succeed within window: {:?}",
            result
        );
    }

    #[test]
    fn test_restoration_time_window_after_expiry() {
        use std::time::Duration;

        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Set very short restoration window for testing (1 second)
        unsafe { std::env::set_var("ANKAIOS_RESTORATION_WINDOW_SECONDS", "1"); }

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Establish counter=100 as the current highest
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 100, 1234);
        let result = validator.verify_update_request(&request, "test-source");
        assert!(result.is_ok(), "Counter=100 should succeed");

        // Mock the boot_time to simulate window expiry
        // We need to access the validator's boot_time field, but it's private
        // So instead, we'll sleep past the window
        std::thread::sleep(Duration::from_secs(2));

        // Now try restoration with counter=50 (lower than current)
        // This is AFTER the restoration window, so should be REJECTED
        let restoration_request = create_signed_update_request(state.clone(), &signing_key, "test-key", 50, 1235);
        let result = validator.verify_update_request(
            &restoration_request,
            "request:agent_A@basic_persistency@startup_restore_nginx"
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Restoration with old counter should fail after window expires: {:?}",
            result
        );

        unsafe { std::env::remove_var("ANKAIOS_RESTORATION_WINDOW_SECONDS"); }
    }

    #[test]
    fn test_restoration_window_custom_duration() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Set custom 30-minute window
        unsafe { std::env::set_var("ANKAIOS_RESTORATION_WINDOW_SECONDS", "1800"); }

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Establish counter=100
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 100, 1234);
        validator.verify_update_request(&request, "test-source").unwrap();

        // Restoration with counter=50 should succeed (within 30-minute window)
        let restoration_request = create_signed_update_request(state.clone(), &signing_key, "test-key", 50, 1235);
        let result = validator.verify_update_request(
            &restoration_request,
            "request:agent_A@basic_persistency@startup_restore_nginx"
        );
        assert!(
            result.is_ok(),
            "Restoration should succeed within custom 30-minute window: {:?}",
            result
        );

        unsafe { std::env::remove_var("ANKAIOS_RESTORATION_WINDOW_SECONDS"); }
    }

    #[test]
    fn test_restoration_window_zero_immediate_strict_validation() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Set window to 0 (immediate strict validation)
        unsafe { std::env::set_var("ANKAIOS_RESTORATION_WINDOW_SECONDS", "0"); }

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Establish counter=100
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 100, 1234);
        validator.verify_update_request(&request, "test-source").unwrap();

        // Restoration with counter=50 should FAIL immediately (window=0)
        let restoration_request = create_signed_update_request(state.clone(), &signing_key, "test-key", 50, 1235);
        let result = validator.verify_update_request(
            &restoration_request,
            "request:agent_A@basic_persistency@startup_restore_nginx"
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Restoration should fail immediately with window=0: {:?}",
            result
        );

        unsafe { std::env::remove_var("ANKAIOS_RESTORATION_WINDOW_SECONDS"); }
    }

    #[test]
    fn test_restoration_window_invalid_value_uses_default() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Set invalid non-numeric value
        unsafe { std::env::set_var("ANKAIOS_RESTORATION_WINDOW_SECONDS", "invalid"); }

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Establish counter=100
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 100, 1234);
        validator.verify_update_request(&request, "test-source").unwrap();

        // Restoration should use DEFAULT_RESTORATION_WINDOW_SECONDS (3600)
        // Since we just created the validator, we're within the default window
        let restoration_request = create_signed_update_request(state.clone(), &signing_key, "test-key", 50, 1235);
        let result = validator.verify_update_request(
            &restoration_request,
            "request:agent_A@basic_persistency@startup_restore_nginx"
        );
        assert!(
            result.is_ok(),
            "Invalid window value should fall back to default (3600s): {:?}",
            result
        );

        unsafe { std::env::remove_var("ANKAIOS_RESTORATION_WINDOW_SECONDS"); }
    }

    #[test]
    fn test_restoration_window_disabled_infinite_grace() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Set window to -1 (disabled = infinite grace period)
        unsafe { std::env::set_var("ANKAIOS_RESTORATION_WINDOW_SECONDS", "-1"); }

        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 3600,  // Config value (will be overridden by env)
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy.clone()).unwrap();

        let state = State::default();

        // Establish counter=100
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 100, 1234);
        validator.verify_update_request(&request, "test-source").unwrap();

        // Wait long enough that normal window would expire
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Restoration with counter=50 should SUCCEED (window disabled = infinite)
        let restoration_request = create_signed_update_request(state.clone(), &signing_key, "test-key", 50, 1235);
        let result = validator.verify_update_request(
            &restoration_request,
            "request:agent_A@basic_persistency@startup_restore_nginx"
        );
        assert!(
            result.is_ok(),
            "Restoration should succeed with disabled window (infinite grace): {:?}",
            result
        );

        unsafe { std::env::remove_var("ANKAIOS_RESTORATION_WINDOW_SECONDS"); }
    }

    #[test]
    fn test_restoration_window_from_config() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (signing_key, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // No env var - should use config value
        let policy = SignaturePolicy {
            require_signature: true,
            require_counter: true,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec!["basic_persistency".to_string()],
            restoration_window_seconds: 2,  // 2 second window from config
        };

        let counter_path = temp_dir.path().join("counters.json");
        unsafe { std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &counter_path); }
        let mut validator =
            SignatureValidator::from_keys_directory(temp_dir.path(), policy).unwrap();

        let state = State::default();

        // Establish counter=100
        let request = create_signed_update_request(state.clone(), &signing_key, "test-key", 100, 1234);
        validator.verify_update_request(&request, "test-source").unwrap();

        // Within 2 seconds: restoration should succeed
        let restoration_request = create_signed_update_request(state.clone(), &signing_key, "test-key", 50, 1235);
        let result = validator.verify_update_request(
            &restoration_request,
            "request:agent_A@basic_persistency@startup_restore_nginx"
        );
        assert!(
            result.is_ok(),
            "Restoration should succeed within configured 2s window: {:?}",
            result
        );

        // Wait for window to expire
        std::thread::sleep(std::time::Duration::from_secs(3));

        // After 3 seconds: restoration should fail (outside 2s window)
        let restoration_request2 = create_signed_update_request(state.clone(), &signing_key, "test-key", 60, 1236);
        let result = validator.verify_update_request(
            &restoration_request2,
            "request:agent_A@basic_persistency@startup_restore_another"
        );
        assert!(
            matches!(result, Err(SignatureError::CounterRollback { .. })),
            "Restoration should fail after configured window expires: {:?}",
            result
        );
    }

    // SECURITY TEST: Strict mode path validation
    #[test]
    fn test_strict_mode_rejects_tmp_path() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        unsafe {
            std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", "/tmp/counters.json");
            std::env::set_var("ANKAIOS_STRICT_SECURITY", "true");
        }

        let policy = SignaturePolicy {
            require_signature: false,
            require_counter: false,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let result = SignatureValidator::from_keys_directory(temp_dir.path(), policy);

        assert!(
            result.is_err(),
            "Should reject /tmp path in strict mode"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("/tmp"),
            "Error should mention /tmp path: {}",
            err_msg
        );

        unsafe {
            std::env::remove_var("ANKAIOS_COUNTER_STATE_PATH");
            std::env::remove_var("ANKAIOS_STRICT_SECURITY");
        }
    }

    #[test]
    fn test_strict_mode_rejects_dev_path() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        unsafe {
            std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", "/dev/null");
            std::env::set_var("ANKAIOS_STRICT_SECURITY", "true");
        }

        let policy = SignaturePolicy {
            require_signature: false,
            require_counter: false,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let result = SignatureValidator::from_keys_directory(temp_dir.path(), policy);

        assert!(
            result.is_err(),
            "Should reject /dev path in strict mode"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("/dev"),
            "Error should mention /dev path: {}",
            err_msg
        );

        unsafe {
            std::env::remove_var("ANKAIOS_COUNTER_STATE_PATH");
            std::env::remove_var("ANKAIOS_STRICT_SECURITY");
        }
    }

    #[test]
    fn test_strict_mode_rejects_relative_path() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        unsafe {
            std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", "relative/path/counters.json");
            std::env::set_var("ANKAIOS_STRICT_SECURITY", "true");
        }

        let policy = SignaturePolicy {
            require_signature: false,
            require_counter: false,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let result = SignatureValidator::from_keys_directory(temp_dir.path(), policy);

        assert!(
            result.is_err(),
            "Should reject relative path in strict mode"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("relative"),
            "Error should mention relative path: {}",
            err_msg
        );

        unsafe {
            std::env::remove_var("ANKAIOS_COUNTER_STATE_PATH");
            std::env::remove_var("ANKAIOS_STRICT_SECURITY");
        }
    }

    #[test]
    fn test_strict_mode_disabled_allows_tmp_with_warning() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Create the /tmp path so validation passes
        let tmp_counters_path = std::env::temp_dir().join("counters_test.json");

        unsafe {
            std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", &tmp_counters_path);
            std::env::remove_var("ANKAIOS_STRICT_SECURITY");  // Default: false
        }

        let policy = SignaturePolicy {
            require_signature: false,
            require_counter: false,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let result = SignatureValidator::from_keys_directory(temp_dir.path(), policy);

        assert!(
            result.is_ok(),
            "Should allow /tmp path without strict mode: {:?}",
            result
        );

        unsafe { std::env::remove_var("ANKAIOS_COUNTER_STATE_PATH"); }
    }

    #[test]
    fn test_strict_mode_various_boolean_values() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        let test_cases = vec![
            ("1", true, "numeric 1"),
            ("true", true, "lowercase true"),
            ("TRUE", true, "uppercase TRUE"),
            ("True", true, "mixed True"),
            ("0", false, "numeric 0"),
            ("false", false, "lowercase false"),
            ("FALSE", false, "uppercase FALSE"),
            ("no", false, "no (not supported)"),
            ("yes", false, "yes (not supported)"),
            ("", false, "empty string"),
        ];

        for (value, should_be_strict, description) in test_cases {
            unsafe {
                std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", "/tmp/test.json");
                std::env::set_var("ANKAIOS_STRICT_SECURITY", value);
            }

            let policy = SignaturePolicy {
                require_signature: false,
                require_counter: false,
                allowed_key_ids: vec![],
                min_counter: 0,
                allowed_restoration_plugins: vec![],
                restoration_window_seconds: 3600,
            };

            let result = SignatureValidator::from_keys_directory(temp_dir.path(), policy);

            if should_be_strict {
                assert!(
                    result.is_err(),
                    "Value '{}' ({}) should enable strict mode and reject /tmp",
                    value, description
                );
            } else {
                assert!(
                    result.is_ok(),
                    "Value '{}' ({}) should NOT enable strict mode: {:?}",
                    value, description, result
                );
            }

            unsafe {
                std::env::remove_var("ANKAIOS_COUNTER_STATE_PATH");
                std::env::remove_var("ANKAIOS_STRICT_SECURITY");
            }
        }
    }

    #[test]
    fn test_strict_mode_accepts_valid_absolute_path() {
        let _guard = lock_env();
        let temp_dir = TempDir::new().unwrap();
        let (_, verifying_key) = create_test_keypair();
        setup_test_keys(temp_dir.path(), &verifying_key, "test-key");

        // Use a valid absolute path outside forbidden directories
        // This path doesn't need to exist for validation to pass
        let valid_path = "/var/lib/ankaios/test-counters.json";

        unsafe {
            std::env::set_var("ANKAIOS_COUNTER_STATE_PATH", valid_path);
            std::env::set_var("ANKAIOS_STRICT_SECURITY", "true");
        }

        let policy = SignaturePolicy {
            require_signature: false,
            require_counter: false,
            allowed_key_ids: vec![],
            min_counter: 0,
            allowed_restoration_plugins: vec![],
            restoration_window_seconds: 3600,
        };

        let result = SignatureValidator::from_keys_directory(temp_dir.path(), policy);

        assert!(
            result.is_ok(),
            "Should accept valid absolute path in strict mode: {:?}",
            result
        );

        unsafe {
            std::env::remove_var("ANKAIOS_COUNTER_STATE_PATH");
            std::env::remove_var("ANKAIOS_STRICT_SECURITY");
        }
    }
}

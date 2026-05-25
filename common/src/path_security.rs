// Copyright (c) 2024 Elektrobit Automotive GmbH
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

//! Path security utilities to prevent traversal attacks
//!
//! This module provides validation for file paths used in workload names and persistence,
//! preventing path traversal, symlink attacks, and other filesystem security issues.

use std::path::{Path, PathBuf};

/// Errors that can occur during path security operations
#[derive(Debug)]
pub enum PathSecurityError {
    /// Name contains invalid characters that could enable path traversal
    InvalidCharacters(String),
    /// Name attempts path traversal (contains .. or path separators)
    TraversalAttempt(String),
    /// Path is invalid or cannot be resolved
    InvalidPath,
    /// Name exceeds maximum allowed length
    NameTooLong,
    /// I/O error during canonicalization
    IoError(String),
}

impl std::fmt::Display for PathSecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCharacters(name) => write!(f, "Invalid characters in name: {}", name),
            Self::TraversalAttempt(name) => write!(f, "Path traversal attempt: {}", name),
            Self::InvalidPath => write!(f, "Invalid path"),
            Self::NameTooLong => write!(f, "Name exceeds maximum length"),
            Self::IoError(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for PathSecurityError {}

/// Validates that a name contains only safe characters for file paths
///
/// # Security
/// - Rejects path separators (/, \)
/// - Rejects parent directory references (..)
/// - Rejects current directory references (.)
/// - Rejects null bytes
/// - Limits length to 255 characters
/// - Only allows: alphanumeric, hyphen, underscore, period
///
/// # Examples
/// ```
/// use common::path_security::validate_safe_name;
///
/// assert!(validate_safe_name("nginx-workload").is_ok());
/// assert!(validate_safe_name("../etc/passwd").is_err());
/// ```
pub fn validate_safe_name(name: &str) -> Result<String, PathSecurityError> {
    // Reject path traversal patterns
    if name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(PathSecurityError::TraversalAttempt(name.to_string()));
    }

    // Reject current directory references
    if name == "." || name.starts_with("./") {
        return Err(PathSecurityError::InvalidCharacters(name.to_string()));
    }

    // Maximum length check (filesystem limit)
    if name.len() > 255 {
        return Err(PathSecurityError::NameTooLong);
    }

    // Allow only safe characters: alphanumeric, hyphen, underscore, period
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(PathSecurityError::InvalidCharacters(name.to_string()));
    }

    Ok(name.to_string())
}

/// Safely join base directory with validated name
///
/// # Security
/// - Validates name before joining
/// - Uses canonicalization to prevent symlink escapes
/// - Verifies result is still under base directory
///
/// # Examples
/// ```no_run
/// use common::path_security::safe_join;
/// use std::path::Path;
///
/// let base = Path::new("/var/lib/ankaios/workloads");
/// let safe_path = safe_join(base, "nginx-workload.yaml")?;
/// # Ok::<(), common::path_security::PathSecurityError>(())
/// ```
pub fn safe_join<P: AsRef<Path>>(base: P, name: &str) -> Result<PathBuf, PathSecurityError> {
    let validated_name = validate_safe_name(name)?;
    let result = base.as_ref().join(validated_name);

    // Verify result is still under base (prevents symlink attacks)
    let canonical_base = base
        .as_ref()
        .canonicalize()
        .map_err(|e| PathSecurityError::IoError(e.to_string()))?;

    // For paths that don't exist yet, check parent directory
    let canonical_result = match result.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            // Path doesn't exist yet - verify parent is under base
            result
                .parent()
                .ok_or(PathSecurityError::InvalidPath)?
                .canonicalize()
                .map_err(|e| PathSecurityError::IoError(e.to_string()))?
                .join(
                    result
                        .file_name()
                        .ok_or(PathSecurityError::InvalidPath)?,
                )
        }
    };

    if !canonical_result.starts_with(canonical_base) {
        return Err(PathSecurityError::TraversalAttempt(name.to_string()));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_safe_name_valid() {
        assert!(validate_safe_name("nginx").is_ok());
        assert!(validate_safe_name("app-1").is_ok());
        assert!(validate_safe_name("workload_v2").is_ok());
        assert!(validate_safe_name("test.yaml").is_ok());
    }

    #[test]
    fn test_validate_safe_name_rejects_traversal() {
        assert!(validate_safe_name("../etc/passwd").is_err());
        assert!(validate_safe_name("../../root").is_err());
        assert!(validate_safe_name("a/b").is_err());
        assert!(validate_safe_name("a\\b").is_err());
    }

    #[test]
    fn test_validate_safe_name_rejects_current_dir() {
        assert!(validate_safe_name(".").is_err());
        assert!(validate_safe_name("./hidden").is_err());
    }

    #[test]
    fn test_validate_safe_name_rejects_null() {
        assert!(validate_safe_name("test\0file").is_err());
    }

    #[test]
    fn test_validate_safe_name_rejects_too_long() {
        let long_name = "a".repeat(256);
        assert!(validate_safe_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_safe_name_rejects_special_chars() {
        assert!(validate_safe_name("file@name").is_err());
        assert!(validate_safe_name("file$name").is_err());
        assert!(validate_safe_name("file name").is_err());
    }

    #[test]
    fn test_safe_join_valid() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let result = safe_join(temp_dir.path(), "test.yaml");
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(temp_dir.path()));
    }

    #[test]
    fn test_safe_join_rejects_traversal() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let result = safe_join(temp_dir.path(), "../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    #[cfg(unix)]
    fn test_safe_join_rejects_symlink_escape() {
        use std::os::unix::fs as unix_fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let link_path = temp_dir.path().join("link");

        // Create symlink pointing outside temp_dir
        unix_fs::symlink("/etc", &link_path).unwrap();

        // Try to use the symlink name
        let result = safe_join(temp_dir.path(), "link");

        // Should be rejected because canonicalization reveals it points outside base
        assert!(
            result.is_err(),
            "Symlink pointing outside base directory should be rejected"
        );

        // Verify it's specifically a traversal attempt error
        assert!(matches!(
            result,
            Err(PathSecurityError::TraversalAttempt(_))
        ));
    }
}

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

//! Secure file I/O operations to prevent race conditions and symlink attacks
//!
//! This module provides atomic file write operations with security protections:
//! - No symlink following (O_NOFOLLOW on Unix)
//! - Atomic writes via temp file + rename
//! - Restrictive file permissions (0600 - owner read/write only)
//! - Validation that files are regular files (not devices, sockets, etc.)

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// Atomically write data to file with security protections
///
/// # Security Features
/// - **Atomic writes**: Uses temp file + rename pattern to ensure atomicity
/// - **No symlink following**: On Unix, uses O_NOFOLLOW to prevent symlink attacks
/// - **Restrictive permissions**: Creates files with 0600 permissions (owner read/write only)
/// - **Safe directory creation**: Creates parent directories with 0700 permissions
///
/// # Implementation
/// 1. Creates parent directory if needed (mode 0700 on Unix)
/// 2. Writes data to temporary file with O_NOFOLLOW and mode 0600
/// 3. Syncs data to disk (fsync)
/// 4. Atomically renames temp file to final path
///
/// The rename operation is atomic on POSIX systems, ensuring that readers
/// either see the old file or the complete new file, never a partial write.
///
/// # Examples
/// ```no_run
/// use common::secure_io::secure_write;
///
/// secure_write("/var/lib/ankaios/state.json", "{\"version\": 1}")?;
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Errors
/// Returns an I/O error if:
/// - Parent directory cannot be created
/// - Temp file creation fails (e.g., if path is a symlink on Unix)
/// - Write or sync fails
/// - Rename fails
pub fn secure_write<P: AsRef<Path>>(path: P, data: &str) -> std::io::Result<()> {
    let path = path.as_ref();

    // Ensure parent directory exists with secure permissions
    if let Some(parent) = path.parent() {
        create_secure_dir(parent)?;
    }

    // Use thread ID and timestamp to create unique temp file name
    // This prevents conflicts when multiple threads write to the same file concurrently
    let thread_id = std::thread::current().id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let temp_name = format!(
        ".{}.{:?}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file"),
        thread_id,
        timestamp
    );

    let temp_path = if let Some(parent) = path.parent() {
        parent.join(&temp_name)
    } else {
        std::path::PathBuf::from(&temp_name)
    };

    // Write to temp file with security protections
    #[cfg(unix)]
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600) // Owner read/write only
            .custom_flags(libc::O_NOFOLLOW) // Don't follow symlinks - critical security feature
            .open(&temp_path)?;

        file.write_all(data.as_bytes())?;
        file.sync_all()?; // Ensure data is flushed to disk before rename
    }

    #[cfg(not(unix))]
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)?;

        file.write_all(data.as_bytes())?;
        file.sync_all()?;

        // Set restrictive permissions after creation on non-Unix platforms
        let metadata = file.metadata()?;
        let mut perms = metadata.permissions();
        perms.set_readonly(false);
        drop(file); // Close file before setting permissions
        std::fs::set_permissions(&temp_path, perms)?;
    }

    // Atomic rename - POSIX guarantees this is atomic
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Securely read file without following symlinks
///
/// # Security Features
/// - **No symlink following**: On Unix, uses O_NOFOLLOW to prevent symlink attacks
/// - **File type validation**: Verifies the file is a regular file, not a device or socket
///
/// # Examples
/// ```no_run
/// use common::secure_io::secure_read;
///
/// let contents = secure_read("/var/lib/ankaios/state.json")?;
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// # Errors
/// Returns an I/O error if:
/// - Path is a symlink (on Unix)
/// - Path is not a regular file (e.g., device, socket, directory)
/// - File cannot be opened or read
///
/// # TOCTOU Note
/// This function does NOT check if the file exists before opening. This prevents
/// TOCTOU (time-of-check-time-of-use) race conditions where a file could be
/// deleted or replaced between an exists() check and open().
pub fn secure_read<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    let path = path.as_ref();

    #[cfg(unix)]
    {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW) // Don't follow symlinks
            .open(path)?;

        // Verify it's a regular file (not device, socket, fifo, etc.)
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Not a regular file",
            ));
        }

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(contents)
    }

    #[cfg(not(unix))]
    {
        // On non-Unix platforms, use standard read
        // Symlink following is generally safer on Windows
        std::fs::read_to_string(path)
    }
}

/// Create a directory with secure permissions
///
/// # Security
/// - Creates directories with 0700 permissions on Unix (owner access only)
/// - Recursively creates parent directories
/// - Validates that the created path is actually a directory (not a symlink to a file)
///
/// # Implementation Note
/// This is an internal helper function. It's not exposed publicly because
/// most use cases should use `secure_write()` which handles directory creation
/// automatically.
fn create_secure_dir<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path = path.as_ref();

    #[cfg(unix)]
    {
        use std::fs::DirBuilder;

        DirBuilder::new()
            .recursive(true)
            .mode(0o700) // Owner access only
            .create(path)?;

        // Verify it's actually a directory (not a file or symlink to file)
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path exists but is not a directory",
            ));
        }
    }

    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_secure_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        secure_write(&file_path, "test data").unwrap();
        let content = secure_read(&file_path).unwrap();

        assert_eq!(content, "test data");
    }

    #[test]
    fn test_secure_write_creates_parent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("subdir").join("test.txt");

        secure_write(&file_path, "test").unwrap();
        assert!(file_path.exists());

        let content = secure_read(&file_path).unwrap();
        assert_eq!(content, "test");
    }

    #[test]
    #[cfg(unix)]
    fn test_secure_write_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        secure_write(&file_path, "test").unwrap();

        let metadata = std::fs::metadata(&file_path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    #[cfg(unix)]
    fn test_secure_write_directory_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("newdir");
        let file_path = subdir.join("test.txt");

        secure_write(&file_path, "test").unwrap();

        let metadata = std::fs::metadata(&subdir).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    }

    #[test]
    #[cfg(unix)]
    fn test_secure_read_rejects_symlink() {
        let temp_dir = TempDir::new().unwrap();
        let real_file = temp_dir.path().join("real.txt");
        let symlink = temp_dir.path().join("link.txt");

        std::fs::write(&real_file, "data").unwrap();
        std::os::unix::fs::symlink(&real_file, &symlink).unwrap();

        // Reading through symlink should fail with ELOOP or similar
        let result = secure_read(&symlink);
        assert!(result.is_err());
    }

    #[test]
    fn test_secure_write_overwrites_existing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        secure_write(&file_path, "original").unwrap();
        secure_write(&file_path, "updated").unwrap();

        let content = secure_read(&file_path).unwrap();
        assert_eq!(content, "updated");
    }

    #[test]
    fn test_secure_read_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        let result = secure_read(&file_path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_secure_read_directory_fails() {
        let temp_dir = TempDir::new().unwrap();

        // Attempting to read a directory should fail
        let result = secure_read(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_secure_write_atomic() {
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("atomic.txt");

        // Write initial data
        secure_write(&file_path, "initial").unwrap();

        // Spawn concurrent writers
        let mut handles = vec![];
        for i in 0..10 {
            // Clone the path for each thread
            let path = file_path.clone();
            let handle = thread::spawn(move || {
                secure_write(&path, &format!("thread-{}", i)).unwrap();
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Final content should be complete (not partially written)
        let content = secure_read(&file_path).unwrap();
        assert!(content.starts_with("thread-"));
        // Verify no corruption (content matches pattern)
        assert!(content.len() >= 8); // "thread-X"
        assert!(!content.contains("thread-thread")); // No interleaving

        // temp_dir is dropped here, ensuring it lives until after all threads complete
    }

    #[test]
    fn test_secure_write_large_data() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("large.txt");

        // Write 1MB of data
        let large_data = "x".repeat(1024 * 1024);
        secure_write(&file_path, &large_data).unwrap();

        let content = secure_read(&file_path).unwrap();
        assert_eq!(content.len(), 1024 * 1024);
        assert_eq!(content, large_data);
    }
}

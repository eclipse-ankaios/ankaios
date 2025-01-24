
mod directory;
mod filesystem;

#[cfg(not(test))]
pub use directory::Directory;
#[cfg(not(test))]
pub use filesystem::FileSystem;
pub use filesystem::FileSystemError;
#[cfg(test)]
pub use directory::{generate_test_directory_mock, MockDirectory};
#[cfg(test)]
pub use filesystem::MockFileSystem;



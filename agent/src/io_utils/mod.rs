// Copyright (c) 2025 Elektrobit Automotive GmbH
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

mod dir_utils;
mod directory;
mod fs;

pub const DEFAULT_RUN_FOLDER: &str = "/tmp/ankaios/";

#[cfg(not(test))]
pub use directory::Directory;
#[cfg(test)]
pub use directory::{generate_test_directory_mock, MockDirectory};
#[cfg(not(test))]
pub use fs::filesystem;
#[cfg(test)]
pub use fs::mock_filesystem;
pub use fs::FileSystemError;

pub use dir_utils::prepare_agent_run_directory;

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

mod workload_files_creator;
mod workload_files_path;
#[cfg(not(test))]
pub use workload_files_creator::WorkloadFilesCreator;
#[cfg(test)]
pub use workload_files_creator::{ConfigFileCreatorError, MockWorkloadFilesCreator};
#[cfg(test)]
pub use workload_files_path::generate_test_workload_files_path;
pub use workload_files_path::WorkloadFilesPath;

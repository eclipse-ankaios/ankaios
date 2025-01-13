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

use std::path::PathBuf;

use common::objects::WorkloadInstanceName;

#[derive(Debug, PartialEq)]
pub struct WorkloadConfigFilesPath(PathBuf);
const SUBFOLDER_CONFIG_FILES: &str = "config_files";

impl WorkloadConfigFilesPath {
    pub fn new(config_files_path: PathBuf) -> Self {
        Self(config_files_path)
    }

    pub fn as_path_buf(&self) -> &PathBuf {
        &self.0
    }
}

impl From<(&PathBuf, &WorkloadInstanceName)> for WorkloadConfigFilesPath {
    fn from((run_folder, workload_instance_name): (&PathBuf, &WorkloadInstanceName)) -> Self {
        let config_files_path = workload_instance_name
            .pipes_folder_name(run_folder.as_path())
            .join(SUBFOLDER_CONFIG_FILES);
        Self(config_files_path)
    }
}

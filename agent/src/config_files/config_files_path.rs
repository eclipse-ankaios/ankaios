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

use std::path::{Path, PathBuf};

use common::objects::WorkloadInstanceName;

#[derive(Debug, PartialEq)]
pub struct WorkloadConfigFilesPath(PathBuf);
const SUBFOLDER_CONFIG_FILES: &str = "config_files";

// [impl->swdd~location-of-workload-config-files-at-predefined-path~1]
impl WorkloadConfigFilesPath {
    pub fn new(config_files_path: PathBuf) -> Self {
        Self(config_files_path)
    }

    pub fn as_path_buf(&self) -> &PathBuf {
        &self.0
    }

    pub fn exists(&self) -> bool {
        self.0.exists()
    }
}

impl AsRef<Path> for WorkloadConfigFilesPath {
    fn as_ref(&self) -> &Path {
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {

    use super::{PathBuf, WorkloadConfigFilesPath, WorkloadInstanceName};

    // [utest->swdd~location-of-workload-config-files-at-predefined-path~1]
    #[test]
    fn utest_workload_config_files_path_from() {
        let run_folder = PathBuf::from("/tmp/ankaios/agent_A_io");
        let workload_instance_name = WorkloadInstanceName::builder()
            .agent_name("agent_A")
            .workload_name("workload_1")
            .id("id")
            .build();
        let expected = PathBuf::from("/tmp/ankaios/agent_A_io/workload_1.id/config_files");
        let workload_config_files_path =
            WorkloadConfigFilesPath::from((&run_folder, &workload_instance_name));
        assert_eq!(&expected, workload_config_files_path.as_path_buf());
    }
}

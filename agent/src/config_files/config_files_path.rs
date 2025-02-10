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

use std::{ops::Deref, path::PathBuf};

use common::objects::WorkloadInstanceName;

#[derive(Debug, PartialEq)]
pub struct WorkloadConfigFilesPath(PathBuf);
const SUBFOLDER_CONFIG_FILES: &str = "config_files";

impl Deref for WorkloadConfigFilesPath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// [impl->swdd~location-of-workload-config-files-at-predefined-path~1]
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
pub fn generate_test_config_files_path() -> WorkloadConfigFilesPath {
    let instance_name = WorkloadInstanceName::new("agent_A", "workload_1", "123xy");
    WorkloadConfigFilesPath::from((&"/tmp/ankaios/agent_A_io".into(), &instance_name))
}

#[cfg(test)]
mod tests {

    use super::{generate_test_config_files_path, PathBuf};

    // [utest->swdd~location-of-workload-config-files-at-predefined-path~1]
    #[test]
    fn utest_workload_config_files_path_from() {
        let workload_config_files_path = generate_test_config_files_path();
        let expected = PathBuf::from("/tmp/ankaios/agent_A_io/workload_1.123xy/config_files");
        assert_eq!(expected, workload_config_files_path.to_path_buf());
    }
}

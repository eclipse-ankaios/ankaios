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

use ankaios_api::ank_base::WorkloadInstanceNameSpec;
use std::{ops::Deref, path::PathBuf};

#[derive(Debug, PartialEq)]
pub struct WorkloadFilesBasePath(PathBuf);
const SUBFOLDER_WORKLOAD_FILES: &str = "files";

impl Deref for WorkloadFilesBasePath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// [impl->swdd~location-of-workload-files-at-predefined-path~1]
impl From<(&PathBuf, &WorkloadInstanceNameSpec)> for WorkloadFilesBasePath {
    fn from((run_folder, workload_instance_name): (&PathBuf, &WorkloadInstanceNameSpec)) -> Self {
        let workload_files_path = workload_instance_name
            .pipes_folder_name(run_folder.as_path())
            .join(SUBFOLDER_WORKLOAD_FILES);
        Self(workload_files_path)
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
pub fn generate_test_workload_files_path() -> WorkloadFilesBasePath {
    use crate::io_utils::default_run_folder_string;
    use ankaios_api::test_utils::fixtures;

    let agent_run_folder = PathBuf::from(default_run_folder_string()).join(format!("{}_io", fixtures::AGENT_NAMES[0]));

    let instance_name = WorkloadInstanceNameSpec::new(
        fixtures::AGENT_NAMES[0],
        fixtures::WORKLOAD_NAMES[0],
        fixtures::WORKLOAD_IDS[0],
    );

    WorkloadFilesBasePath::from((&agent_run_folder, &instance_name))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::WorkloadFilesBasePath;
    use crate::io_utils::default_run_folder_string;
    use ankaios_api::ank_base::WorkloadInstanceNameSpec;
    use ankaios_api::test_utils::fixtures;

    // [utest->swdd~location-of-workload-files-at-predefined-path~1]
    #[test]
    fn utest_workload_files_path_from() {
        let agent_run_folder = PathBuf::from(default_run_folder_string())
            .join(format!("{}_io", fixtures::AGENT_NAMES[0]));
        let instance_name = WorkloadInstanceNameSpec::new(
            fixtures::AGENT_NAMES[0],
            fixtures::WORKLOAD_NAMES[0],
            fixtures::WORKLOAD_IDS[0],
        );
        let workload_files_path = WorkloadFilesBasePath::from((&agent_run_folder, &instance_name));
        let expected = agent_run_folder
            .join(format!(
                "{}.{}",
                fixtures::WORKLOAD_NAMES[0],
                fixtures::WORKLOAD_IDS[0]
            ))
            .join("files");
        assert_eq!(expected, workload_files_path.to_path_buf());
    }
}

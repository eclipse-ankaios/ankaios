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

use std::path::{Path, PathBuf};

use common::{objects::WorkloadInstanceName, to_server_interface::ToServerSender};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, mockall_double::double)]
use super::PipesChannelContext;

pub struct PipesChannelContextInfo {
    run_folder: PathBuf,
    workload_instance_name: WorkloadInstanceName,
    control_interface_to_server_sender: ToServerSender,
}

#[cfg_attr(test, automock)]
impl PipesChannelContextInfo {
    pub fn new(
        run_folder: &Path,
        control_interface_to_server_sender: ToServerSender,
        workload_instance_name: &WorkloadInstanceName,
    ) -> Self {
        Self {
            run_folder: run_folder.to_path_buf(),
            workload_instance_name: workload_instance_name.clone(),
            control_interface_to_server_sender,
        }
    }

    pub fn get_run_folder(&self) -> &PathBuf {
        &self.run_folder
    }
    pub fn get_workload_instance_name(&self) -> &WorkloadInstanceName {
        &self.workload_instance_name
    }

    pub fn make_context(self) -> Option<PipesChannelContext> {
        match PipesChannelContext::new(
            &self.run_folder,
            &self.workload_instance_name,
            self.control_interface_to_server_sender.clone(),
        ) {
            Ok(res) => Some(res),
            _ => None,
        }
    }
}

// impl From<PipesChannelContextInfo> for Option<PipesChannelContext> {
//     fn from(val: PipesChannelContextInfo) -> Self {
//         PipesChannelContext::new(
//             &val.run_folder,
//             &val.workload_instance_name,
//             val.control_interface_to_server_sender.clone(),
//         )
//         .ok()
//     }
// }

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {}

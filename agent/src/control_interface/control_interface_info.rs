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

use std::path::{Path, PathBuf};

use common::{objects::WorkloadInstanceName, to_server_interface::ToServerSender};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, mockall_double::double)]
use super::authorizer::Authorizer;
#[cfg_attr(test, mockall_double::double)]
use super::ControlInterface;

#[derive(Debug)]
pub struct ControlInterfaceInfo {
    run_folder: PathBuf,
    workload_instance_name: WorkloadInstanceName,
    control_interface_to_server_sender: ToServerSender,
    authorizer: Authorizer,
}

#[cfg_attr(test, automock)]
impl ControlInterfaceInfo {
    pub fn new(
        run_folder: &Path,
        control_interface_to_server_sender: ToServerSender,
        workload_instance_name: &WorkloadInstanceName,
        authorizer: Authorizer,
    ) -> Self {
        Self {
            run_folder: run_folder.to_path_buf(),
            workload_instance_name: workload_instance_name.clone(),
            control_interface_to_server_sender,
            authorizer,
        }
    }

    pub fn get_run_folder(&self) -> &PathBuf {
        &self.run_folder
    }

    pub fn get_workload_instance_name(&self) -> &WorkloadInstanceName {
        &self.workload_instance_name
    }

    pub fn get_control_interface_to_server_sender(&self) -> ToServerSender {
        self.control_interface_to_server_sender.clone()
    }

    pub fn get_authorizer(&self) -> Authorizer {
        self.authorizer.clone()
    }

    pub fn has_same_configuration(&self, other: &ControlInterface) -> bool {
        let self_location = self
            .workload_instance_name
            .pipes_folder_name(&self.run_folder);

        if self_location != other.get_api_location() {
            return false;
        };

        let self_authorizer = &self.authorizer;
        let other_authorizer = other.get_authorizer();

        self_authorizer == other_authorizer
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
    use super::{
        Authorizer, ControlInterface, ControlInterfaceInfo, Path, PathBuf, WorkloadInstanceName,
    };

    use common::to_server_interface::ToServer;

    const WORKLOAD_1_NAME: &str = "workload1";
    const PIPES_LOCATION: &str = "/some/path";

    #[test]
    fn utest_new() {
        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        let new_context_info = ControlInterfaceInfo::new(
            Path::new(PIPES_LOCATION),
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &workload_instance_name,
            Authorizer::default(),
        );

        assert_eq!(
            Path::new(PIPES_LOCATION).to_path_buf(),
            new_context_info.run_folder
        );
        assert_eq!(
            workload_instance_name,
            new_context_info.workload_instance_name
        );
    }

    #[test]
    fn utest_get_run_folder() {
        let path = &Path::new(PIPES_LOCATION);
        let new_context_info = ControlInterfaceInfo::new(
            path,
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &WorkloadInstanceName::builder()
                .workload_name(WORKLOAD_1_NAME)
                .build(),
            Authorizer::default(),
        );

        assert_eq!(&path.to_path_buf(), new_context_info.get_run_folder());
    }

    #[test]
    fn utest_has_same_configuration_true() {
        let run_folder = Path::new(PIPES_LOCATION);
        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();
        let pipes_folder = workload_instance_name.pipes_folder_name(run_folder);
        let mut context_info_authorizer = Authorizer::default();
        let other_context_authorizer = Authorizer::default();
        context_info_authorizer.expect_eq().return_const(true);

        let context_info = ControlInterfaceInfo::new(
            run_folder,
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &workload_instance_name,
            context_info_authorizer,
        );

        let mut other_context = ControlInterface::default();
        other_context
            .expect_get_api_location()
            .once()
            .return_const(pipes_folder);
        other_context
            .expect_get_authorizer()
            .once()
            .return_const(other_context_authorizer);

        assert!(context_info.has_same_configuration(&other_context));
    }

    #[test]
    fn utest_has_same_configuration_with_different_location_returns_false() {
        let run_folder = Path::new(PIPES_LOCATION);
        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        let context_info = ControlInterfaceInfo::new(
            run_folder,
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &workload_instance_name,
            Authorizer::default(),
        );

        let mut other_context = ControlInterface::default();
        other_context
            .expect_get_api_location()
            .once()
            .return_const(PathBuf::from("other_path"));

        assert!(!context_info.has_same_configuration(&other_context));
    }

    #[test]
    fn utest_has_same_configuration_with_different_authorizer_returns_false() {
        let run_folder = Path::new(PIPES_LOCATION);
        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();
        let pipes_folder = workload_instance_name.pipes_folder_name(run_folder);
        let mut context_info_authorizer = Authorizer::default();
        let other_context_authorizer = Authorizer::default();
        context_info_authorizer.expect_eq().return_const(false);

        let context_info = ControlInterfaceInfo::new(
            run_folder,
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &workload_instance_name,
            context_info_authorizer,
        );

        let mut other_context = ControlInterface::default();
        other_context
            .expect_get_api_location()
            .once()
            .return_const(pipes_folder);
        other_context
            .expect_get_authorizer()
            .once()
            .return_const(other_context_authorizer);

        assert!(!context_info.has_same_configuration(&other_context));
    }
}

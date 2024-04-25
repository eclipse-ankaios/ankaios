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
use super::PipesChannelContext;

#[derive(Debug)]
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

    pub fn create_control_interface(self) -> Option<PipesChannelContext> {
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_interface::MockPipesChannelContext;
    use crate::control_interface::PipesChannelContextError;
    use common::to_server_interface::ToServer;

    const WORKLOAD_1_NAME: &str = "workload1";
    const PIPES_LOCATION: &str = "/some/path";

    #[test]
    fn utest_new() {
        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        let new_context_info = PipesChannelContextInfo::new(
            Path::new(PIPES_LOCATION),
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &workload_instance_name,
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
        let new_context_info = PipesChannelContextInfo::new(
            path,
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &WorkloadInstanceName::builder()
                .workload_name(WORKLOAD_1_NAME)
                .build(),
        );

        assert_eq!(&path.to_path_buf(), new_context_info.get_run_folder());
    }

    #[test]
    fn utest_get_workload_instance_name() {
        let workload_instance_name = WorkloadInstanceName::builder()
            .workload_name(WORKLOAD_1_NAME)
            .build();

        let new_context_info = PipesChannelContextInfo::new(
            Path::new(PIPES_LOCATION),
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &workload_instance_name,
        );

        assert_eq!(
            &workload_instance_name,
            new_context_info.get_workload_instance_name()
        );
    }

    #[tokio::test]
    async fn utest_create_control_interface_ok() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_context_info = PipesChannelContextInfo::new(
            Path::new(PIPES_LOCATION),
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &WorkloadInstanceName::builder()
                .workload_name(WORKLOAD_1_NAME)
                .build(),
        );

        let pipes_channel_context_mock = MockPipesChannelContext::new_context();
        pipes_channel_context_mock
            .expect()
            .once()
            .return_once(|_, _, _| Ok(MockPipesChannelContext::default()));

        assert!(new_context_info.create_control_interface().is_some());
    }

    #[tokio::test]
    async fn utest_create_control_interface_failed() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let new_context_info = PipesChannelContextInfo::new(
            Path::new(PIPES_LOCATION),
            tokio::sync::mpsc::channel::<ToServer>(1).0,
            &WorkloadInstanceName::builder()
                .workload_name(WORKLOAD_1_NAME)
                .build(),
        );

        let pipes_channel_context_mock = MockPipesChannelContext::new_context();
        pipes_channel_context_mock
            .expect()
            .once()
            .return_once(|_, _, _| {
                Err(PipesChannelContextError::CouldNotCreateFifo(String::from(
                    "error",
                )))
            });

        assert!(new_context_info.create_control_interface().is_none());
    }
}

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

use async_trait::async_trait;
use common::objects::{ExecutionState, WorkloadSpec};
use common::state_change_interface::StateChangeInterface;
use common::state_change_interface::StateChangeSender;

use std::path::{Path, PathBuf};
use tokio::task::JoinHandle;

use crate::podman::podman_runtime_config::PodmanRuntimeConfig;

use crate::workload_trait::{Workload, WorkloadError};
use common::objects::{WorkloadExecutionInstanceName, WorkloadInstanceName};

use crate::podman::podman_utils::PodmanUtils;

#[cfg(test)]
use mockall::automock;

#[derive(Debug)]
pub struct PodmanWorkload {
    manager_interface: StateChangeSender,
    workload_spec: WorkloadSpec,
    api_pipes_location: PathBuf,
    podman_utils: PodmanUtils,
    socket_path: String,
}

#[derive(Debug)]
pub struct PodmanWorkloadState {
    container_id: String,
    handle: JoinHandle<()>,
}

#[cfg(test)]
lazy_static::lazy_static! {
    pub static ref MOCK_PODMAN_WORKLOAD_MTX: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
}

#[cfg_attr(test, automock)]
impl PodmanWorkload {
    pub fn new(
        manager_interface: StateChangeSender,
        socket_path: String,
        workload_spec: WorkloadSpec,
        run_folder: &Path,
    ) -> Self {
        Self {
            manager_interface,
            api_pipes_location: workload_spec.instance_name().pipes_folder_name(run_folder),
            workload_spec,
            podman_utils: PodmanUtils::new(socket_path.clone()),
            socket_path,
        }
    }

    async fn start_workload(&self) -> Result<PodmanWorkloadState, WorkloadError> {
        let result = async {
            let container_id = self.create_container().await?;
            let handle = self.start_container(&container_id).await?;

            Ok(PodmanWorkloadState {
                container_id,
                handle,
            })
        }
        .await;

        if result.is_err() {
            // [impl->swdd~podman-workload-update-workload-state-on-start-failure~1]
            self.manager_interface
                .update_workload_state(vec![common::objects::WorkloadState {
                    agent_name: self.workload_spec.agent.clone(),
                    workload_name: self.workload_spec.workload.name.to_string(),
                    execution_state: ExecutionState::ExecFailed,
                }])
                .await;
        }

        result
    }

    async fn create_container(&self) -> Result<String, WorkloadError> {
        let workload_cfg = PodmanRuntimeConfig::try_from(&self.workload_spec)?;

        // [impl->swdd~podman-workload-pulls-container~2]]
        let has_image = self.has_image(&workload_cfg.image).await.map_err(|e| {
            WorkloadError::StartError(format!("Could not check for image in local storage: {}", e))
        })?;

        if !has_image {
            // [impl->swdd~podman-workload-pulls-container~2]]
            self.podman_utils
                .pull_image(&workload_cfg.image)
                .await
                .map_err(|e| {
                    WorkloadError::StartError(format!(
                        "Could not pull the podman image '{}': {}",
                        workload_cfg.image, e
                    ))
                })?;
        }

        // [impl->swdd~podman-workload-creates-container~1]
        // [impl->swdd~podman-adapt-mount-interface-pipes-into-workload~2]
        match self

            .podman_utils
            .create_container(
                workload_cfg,
                self.workload_spec.instance_name().to_string(),
                self.api_pipes_location.to_string_lossy().to_string(),
            )
            .await
        {
            Ok(id) => {
                // [impl->swdd~podman-workload-stores-container-id~1]
                Ok(id)
            }
            Err(e) => Err(WorkloadError::StartError(format!(
                "Could not create the podman container. Error: '{}'",
                e
            ))),
        }
    }

    async fn has_image(&self, image: &str) -> Result<bool, String> {
        self.podman_utils.has_image(image).await
    }

    // [impl->swdd~podman-workload-monitors-workload-state~1]
    // [impl->swdd~podman-workload-starts-container~1]
    async fn start_container(&self, container_id: &str) -> Result<JoinHandle<()>, WorkloadError> {
        if let Err(e) = self.podman_utils.start_container(container_id).await {
            Err(WorkloadError::StartError(format!(
                "Error starting the container '{}': '{}'",
                self.workload_spec.instance_name(),
                e
            )))
        } else {
            Ok(self.watch_container(container_id.to_string()))
        }
    }

    fn watch_container(&self, container_id: String) -> JoinHandle<()> {
        let manager_interface_clone = self.manager_interface.clone();
        let agent_name_clone = self.workload_spec.instance_name().agent_name().to_string();
        let workload_name_clone = self
            .workload_spec
            .instance_name()
            .workload_name()
            .to_string();

        let podman_utils = PodmanUtils::new(self.socket_path.to_string());
        tokio::spawn(async move {
            podman_utils
                .check_status(
                    manager_interface_clone,
                    agent_name_clone,
                    workload_name_clone,
                    container_id,
                )
                .await
        })
    }

    // [impl->swdd~podman-workload-stops-container~1]
    async fn stop_container(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
        container_id: &str,
    ) -> Result<(), WorkloadError> {
        match self.podman_utils.stop_container(container_id).await {
            Ok(()) => {
                log::debug!("Successfully stopped container '{}'.", instance_name);
                Ok(())
            }
            Err(error) => Err(WorkloadError::DeleteError(format!(
                "Error stopping container '{}': '{}'.",
                instance_name, error
            ))),
        }
    }

    // [impl->swdd~podman-workload-deletes-container~1]
    async fn delete_container(
        &self,
        instance_name: &WorkloadExecutionInstanceName,
        container_id: &str,
        manager_interface: &StateChangeSender,
    ) -> Result<(), WorkloadError> {
        match self.podman_utils.delete_container(container_id).await {
            Ok(()) => {
                log::debug!("Successfully deleted container '{}'.", instance_name);
                manager_interface
                    .update_workload_state(vec![common::objects::WorkloadState {
                        agent_name: instance_name.agent_name().to_string(),
                        workload_name: instance_name.workload_name().to_string(),
                        execution_state: ExecutionState::ExecRemoved,
                    }])
                    .await;
                Ok(())
            }
            Err(e) => Err(WorkloadError::DeleteError(format!(
                "Error deleting container '{}': '{}'.",
                instance_name, e
            ))),
        }
    }
}

#[async_trait]
impl Workload for PodmanWorkload {
    type State = PodmanWorkloadState;
    type Id = String;
    async fn start(&self) -> Result<PodmanWorkloadState, WorkloadError> {
        self.start_workload().await
    }

    // [impl->swdd~agent-podman-workload-resumes-existing-workload~1]
    fn resume(&self, id: String) -> Result<PodmanWorkloadState, WorkloadError> {
        Ok(PodmanWorkloadState {
            container_id: id.clone(),
            handle: self.watch_container(id),
        })
    }

    // [impl->swdd~agent-podman-workload-replace-existing-workload~1]
    async fn replace(
        &self,
        existing_instance_name: WorkloadExecutionInstanceName,
        existing_id: Self::Id,
    ) -> Result<Self::State, WorkloadError> {
        self.stop_container(&existing_instance_name, &existing_id)
            .await?;
        self.delete_container(
            &existing_instance_name,
            &existing_id,
            &self.manager_interface,
        )
        .await?;

        self.start_workload().await
    }

    async fn delete(&mut self, running_state: PodmanWorkloadState) -> Result<(), WorkloadError> {
        self.stop_container(
            &self.workload_spec.instance_name(),
            &running_state.container_id,
        )
        .await?;
        self.delete_container(
            &self.workload_spec.instance_name(),
            &running_state.container_id,
            &self.manager_interface,
        )
        .await?;
        Ok(())
    }

    fn name(&self) -> String {
        self.workload_spec
            .instance_name()
            .workload_name()
            .to_string()
    }
}

impl Drop for PodmanWorkloadState {
    fn drop(&mut self) {
        log::debug!(
            "Stopping the status checker task for container '{}'",
            self.container_id
        );

        self.handle.abort();
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
impl PodmanWorkload {
    pub fn generate_test_podman_workload(
        agent_name: &str,
        workload_name: &str,
        runtime_name: &str,
        socket_path: &str,
        run_folder: &str,
        to_server: common::state_change_interface::StateChangeSender,
    ) -> PodmanWorkload {
        let workload_spec = common::test_utils::generate_test_workload_spec_with_param(
            agent_name.into(),
            workload_name.into(),
            runtime_name.into(),
        );

        PodmanWorkload::new(
            to_server,
            socket_path.to_string(),
            workload_spec,
            Path::new(run_folder),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, thread, time::Duration};

    use crate::{
        podman::{
            test_utils::{
                request_handlers::{
                    handler_helpers::*, BasicRequestHandler, ContainerCreateRequestHandler,
                    ErrorResponseRequestHandler, ListContainerRequestHandler,
                    PullImagesRequestHandler, RequestHandler, WithRequestHandlerParameter,
                },
                server_models::ServerListContainer,
                test_daemon::PodmanTestDaemon,
            },
            workload::{PodmanWorkload, PodmanWorkloadState},
        },
        workload_trait::Workload,
    };
    use common::{
        objects::WorkloadExecutionInstanceName,
        state_change_interface::generate_test_failed_update_workload_state,
    };
    use common::{
        state_change_interface::StateChangeCommand,
        test_utils::generate_test_workload_spec_with_param,
    };
    use futures_util::Future;
    use hyper::StatusCode;

    const BUFFER_SIZE: usize = 20;
    const AGENT_NAME: &str = "agent X";
    const WORKLOAD_NAME: &str = "workload_name";
    const RUNTIME_NAME: &str = "my_runtime";
    const RUN_FOLDER: &str = "/api/pipes/location";
    const API_PIPES_LOCATION: &str = "/api/pipes/location/workload_name.29b6a7ccff50ffc4dac58ad64c837888584fb121d7ef72c059205e5d29b9c901";
    const CONTAINER_ID: &str = "testid";
    const TEST_TIMEOUT: u64 = 100;
    const API_PIPES_MOUNT_POINT: &str = "/run/ankaios/control_interface";
    const BIND_MOUNT: &str = "bind";

    #[tokio::test()]
    async fn utest_workload_name() {
        let (to_server, _from_agent) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let workload = PodmanWorkload::generate_test_podman_workload(
            AGENT_NAME,
            WORKLOAD_NAME,
            RUNTIME_NAME,
            "socket_path",
            "/run/folder",
            to_server,
        );

        assert_eq!(
            WORKLOAD_NAME.to_string(),
            workload.name(),
            "Expected: '{}', got: '{}'",
            WORKLOAD_NAME,
            workload.name()
        );
    }

    // [utest->swdd~podman-workload-pulls-container~2]
    // [utest->swdd~podman-workload-creates-container~1]
    // [utest->swdd~podman-adapt-mount-interface-pipes-into-workload~2]
    // [utest->swdd~podman-workload-sends-workload-state~1]
    // [utest->swdd~podman-workload-monitors-workload-state~1]
    // [utest->swdd~podman-workload-starts-container~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_start_with_pull_success() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            list_images_empty_handler(),
            pull_successful_handler(),
            create_successful_handler(),
            start_successful_handler(),
            list_containers_handler(),
        ])
        .await;

        let res = workload.start().await;

        // [utest->swdd~podman-workload-stores-container-id~1]
        assert!(matches!(&res, Ok(running_state) if running_state.container_id == CONTAINER_ID));

        timeout(test_daemon.wait_expected_requests_done()).await;
        timeout(from_agent.recv())
            .await
            .expect("Not message from agent");

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-pulls-container~2]
    // [utest->swdd~podman-workload-creates-container~1]
    // [utest->swdd~podman-adapt-mount-interface-pipes-into-workload~2]
    // [utest->swdd~podman-workload-sends-workload-state~1]
    // [utest->swdd~podman-workload-monitors-workload-state~1]
    // [utest->swdd~podman-workload-starts-container~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_start_image_in_local_storage_success() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            list_images_result_handler(),
            pull_not_called_handler(),
            create_successful_handler(),
            start_successful_handler(),
            list_containers_handler(),
        ])
        .await;

        let res = workload.start().await;

        // [utest->swdd~podman-workload-stores-container-id~1]
        assert!(matches!(&res, Ok(running_state) if running_state.container_id == CONTAINER_ID));

        timeout(test_daemon.wait_expected_requests_done()).await;
        timeout(from_agent.recv())
            .await
            .expect("Not message from agent");

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-creates-container~1]
    // [utest->swdd~podman-adapt-mount-interface-pipes-into-workload~2]
    // [utest->swdd~podman-workload-sends-workload-state~1]
    // [utest->swdd~podman-workload-monitors-workload-state~1]
    // [utest->swdd~podman-workload-starts-container~1]
    // [utest->swdd~podman-workload-update-workload-state-on-start-failure~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_start_list_image_fail() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            list_images_error_handler(),
            pull_not_called_handler(),
            create_not_called_handler(),
            start_not_called_handler(),
        ])
        .await;

        let res = workload.start().await;

        // [utest->swdd~podman-workload-stores-container-id~1]
        assert!(res.is_err());

        // Check that receiver is empty - no notification has been sent
        timeout(test_daemon.wait_expected_requests_done()).await;
        assert_eq!(
            from_agent.try_recv().unwrap(),
            generate_test_failed_update_workload_state(AGENT_NAME, WORKLOAD_NAME)
        );

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-creates-container~1]
    // [utest->swdd~podman-adapt-mount-interface-pipes-into-workload~2]
    // [utest->swdd~podman-workload-sends-workload-state~1]
    // [utest->swdd~podman-workload-monitors-workload-state~1]
    // [utest->swdd~podman-workload-starts-container~1]
    // [utest->swdd~podman-workload-update-workload-state-on-start-failure~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_start_pull_fail() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            list_images_empty_handler(),
            pull_error_handler(),
            create_not_called_handler(),
            start_not_called_handler(),
        ])
        .await;

        let res = workload.start().await;

        // [utest->swdd~podman-workload-stores-container-id~1]
        assert!(res.is_err());

        // Check that receiver is empty - no notification has been sent
        timeout(test_daemon.wait_expected_requests_done()).await;
        assert_eq!(
            from_agent.try_recv().unwrap(),
            generate_test_failed_update_workload_state(AGENT_NAME, WORKLOAD_NAME)
        );

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-update-workload-state-on-start-failure~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_create_failed() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            list_images_empty_handler(),
            create_error_handler(),
            start_not_called_handler(),
        ])
        .await;

        let res = workload.start().await;

        assert!(res.is_err());
        timeout(test_daemon.wait_expected_requests_done()).await;
        assert_eq!(
            from_agent.try_recv().unwrap(),
            generate_test_failed_update_workload_state(AGENT_NAME, WORKLOAD_NAME)
        );

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-update-workload-state-on-start-failure~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_create_no_connection_to_socket() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut from_agent) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.into(),
            WORKLOAD_NAME.into(),
            RUNTIME_NAME.into(),
        );

        let workload = PodmanWorkload::new(
            to_server,
            String::from("/not/running/socket"),
            workload_spec,
            Path::new(RUN_FOLDER),
        );

        let res = workload.start().await;

        assert!(res.is_err());
        thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        assert_eq!(
            from_agent.try_recv().unwrap(),
            generate_test_failed_update_workload_state(AGENT_NAME, WORKLOAD_NAME)
        );
    }

    // [utest->swdd~podman-workload-update-workload-state-on-start-failure~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_start_failure() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            list_images_result_handler(),
            create_successful_handler(),
            start_error_handler(),
        ])
        .await;

        let res = workload.start().await;

        assert!(res.is_err());
        timeout(test_daemon.wait_expected_requests_done()).await;
        assert_eq!(
            from_agent.try_recv().unwrap(),
            generate_test_failed_update_workload_state(AGENT_NAME, WORKLOAD_NAME)
        );

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-update-workload-state-on-start-failure~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_start_failure_no_connection() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut from_agent) =
            tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let workload_spec = generate_test_workload_spec_with_param(
            AGENT_NAME.into(),
            WORKLOAD_NAME.into(),
            RUNTIME_NAME.into(),
        );

        let workload = PodmanWorkload::new(
            to_server,
            String::from("/not/running/socket"),
            workload_spec,
            Path::new(RUN_FOLDER),
        );

        let res = workload.start().await;

        assert!(res.is_err());
        thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        assert_eq!(
            from_agent.try_recv().unwrap(),
            generate_test_failed_update_workload_state(AGENT_NAME, WORKLOAD_NAME)
        );
    }

    // [utest->swdd~podman-workload-stops-container~1]
    // [utest->swdd~podman-workload-deletes-container~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_delete_success() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (mut workload, mut test_daemon, mut from_agent) =
            setup_test_workload_with_daemon(vec![
                stop_success_handler(CONTAINER_ID),
                delete_success_handler(CONTAINER_ID),
            ])
            .await;

        let handle = tokio::spawn(async move {});

        let res = workload
            .delete(PodmanWorkloadState {
                container_id: CONTAINER_ID.into(),
                handle,
            })
            .await;

        assert!(res.is_ok());

        // Wait for a while and check if the workload sent the notification about removed workload
        timeout(test_daemon.wait_expected_requests_done()).await;
        timeout(from_agent.recv())
            .await
            .expect("Not message from agent");
        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-stops-container~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_stop_failure() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (mut workload, test_daemon, mut _from_agent) = setup_test_workload_with_daemon(vec![
            stop_error_handler(CONTAINER_ID),
            delete_not_called_handler(CONTAINER_ID),
        ])
        .await;

        let handle = tokio::spawn(async move {});

        let res = workload
            .delete(PodmanWorkloadState {
                container_id: CONTAINER_ID.into(),
                handle,
            })
            .await;

        assert!(res.is_err());
        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-deletes-container~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_container_delete_failure() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (mut workload, test_daemon, mut _from_agent) = setup_test_workload_with_daemon(vec![
            stop_success_handler(CONTAINER_ID),
            delete_error_handler(CONTAINER_ID),
        ])
        .await;

        let handle = tokio::spawn(async move {});

        let res = workload
            .delete(PodmanWorkloadState {
                container_id: CONTAINER_ID.into(),
                handle,
            })
            .await;

        assert!(res.is_err());
        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~agent-podman-workload-replace-existing-workload~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_workload_replace_success() {
        let _ = env_logger::builder().is_test(true).try_init();
        let old_container_id = "old_id";
        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            list_images_result_handler(),
            pull_not_called_handler(),
            stop_success_handler(old_container_id),
            delete_success_handler(old_container_id),
            create_successful_handler(),
            start_successful_handler(),
            list_containers_handler(),
        ])
        .await;

        let instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(AGENT_NAME)
            .workload_name(WORKLOAD_NAME)
            .build();

        let res = workload
            .replace(instance_name, old_container_id.to_string())
            .await;

        // [utest->swdd~podman-workload-stores-container-id~1]
        assert!(matches!(&res, Ok(running_state) if running_state.container_id == CONTAINER_ID));

        timeout(test_daemon.wait_expected_requests_done()).await;
        timeout(from_agent.recv()).await.expect("");

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~agent-podman-workload-replace-existing-workload~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_workload_replace_stop_failed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let old_container_id = "old_id";
        let (workload, test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            stop_error_handler(old_container_id),
            delete_not_called_handler(old_container_id),
            create_not_called_handler(),
            start_not_called_handler(),
        ])
        .await;

        let instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(AGENT_NAME)
            .workload_name(WORKLOAD_NAME)
            .build();

        let res = workload
            .replace(instance_name, old_container_id.to_string())
            .await;

        assert!(res.is_err());
        thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        assert!(from_agent.try_recv().is_err());

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~agent-podman-workload-replace-existing-workload~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_workload_replace_delete_failed() {
        let _ = env_logger::builder().is_test(true).try_init();
        let old_container_id = "old_id";
        let (workload, mut test_daemon, mut from_agent) = setup_test_workload_with_daemon(vec![
            stop_success_handler(old_container_id),
            delete_error_handler(old_container_id),
            create_not_called_handler(),
            start_not_called_handler(),
        ])
        .await;

        let instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name(AGENT_NAME)
            .workload_name(WORKLOAD_NAME)
            .build();

        let res = workload
            .replace(instance_name, old_container_id.to_string())
            .await;

        assert!(res.is_err());
        timeout(test_daemon.wait_expected_requests_done()).await;
        assert!(from_agent.try_recv().is_err());

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~agent-podman-workload-resumes-existing-workload~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_workload_resume_success() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (workload, test_daemon, mut from_agent) =
            setup_test_workload_with_daemon(vec![list_containers_handler()]).await;

        let res = workload.resume(CONTAINER_ID.to_string());

        // [utest->swdd~podman-workload-stores-container-id~1]
        assert!(matches!(&res, Ok(running_state) if running_state.container_id == CONTAINER_ID));

        // Check that receiver is empty - no notification has been sent
        thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        assert!(from_agent.try_recv().is_ok());

        test_daemon.check_calls_and_stop();
    }

    async fn setup_test_workload_with_daemon(
        request_handlers: Vec<Box<dyn RequestHandler + Send + Sync>>,
    ) -> (
        PodmanWorkload,
        PodmanTestDaemon,
        tokio::sync::mpsc::Receiver<StateChangeCommand>,
    ) {
        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let (to_server, from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(BUFFER_SIZE);

        let workload = PodmanWorkload::generate_test_podman_workload(
            AGENT_NAME,
            WORKLOAD_NAME,
            RUNTIME_NAME,
            &test_daemon.socket_path,
            RUN_FOLDER,
            to_server,
        );

        (workload, test_daemon, from_agent)
    }

    fn list_images_result_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .request_path("/libpod/images/json?filters")
                .resp_body(r#"[{"Id":"9ed4aefc74f6792b5a804d1d146fe4b4a2299147b0f50eaf2b08435d7b38c27e","ParentId":"","RepoTags":["docker.io/library/alpine:latest"],"RepoDigests":["docker.io/library/alpine@sha256:124c7d2707904eea7431fffe91522a01e5a861a624ee31d03372cc1d138a3126","docker.io/library/alpine@sha256:b6ca290b6b4cdcca5b3db3ffa338ee0285c11744b4a6abaa9627746ee3291d8d"],"Created":1680113964,"Size":7342148,"SharedSize":0,"VirtualSize":7342148,"Labels":null,"Containers":0,"Names":["docker.io/library/alpine:latest"],"Digest":"sha256:124c7d2707904eea7431fffe91522a01e5a861a624ee31d03372cc1d138a3126","History":["docker.io/library/alpine:latest"]}]"#)
                .times(1),
        )
    }

    fn list_images_empty_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .request_path("/libpod/images/json?filters")
                .resp_body(r"[]")
                .times(1),
        )
    }

    fn list_images_error_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            ErrorResponseRequestHandler::default()
                .request_path("/libpod/images/json?filters")
                .status_code(StatusCode::BAD_REQUEST)
                .error_message("Simulated error")
                .times(1),
        )
    }

    fn pull_successful_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            PullImagesRequestHandler::default()
                .resp_body("testImageId")
                .times(1),
        )
    }

    fn pull_not_called_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .methods(vec!["PULL".into()])
                .request_path("/libpod/images/pull")
                .status_code(StatusCode::NO_CONTENT)
                .times(0),
        )
    }

    fn pull_error_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            ErrorResponseRequestHandler::default()
                .methods(vec!["PULL".into()])
                .request_path("/libpod/images/pull")
                .status_code(StatusCode::NO_CONTENT)
                .times(0),
        )
    }

    fn create_successful_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            ContainerCreateRequestHandler::default()
                .status_code(StatusCode::CREATED)
                .resp_body(CONTAINER_ID)
                .times(1)
                .call_checker(|_, call_body| {
                    let assert_successful = (move || {
                        let mounts = call_body.as_object()?.get("mounts")?.as_array()?;
                        assert_eq!(
                            mounts.len(),
                            1,
                            "Expected 1 mount parameter got {}",
                            mounts.len()
                        );
                        let mount = mounts[0].as_object()?;
                        let destination = mount.get("destination")?.as_str()?;
                        let source = mount.get("source")?.as_str()?;
                        let mount_type = mount.get("type")?.as_str()?;

                        assert_eq!(
                            destination, API_PIPES_MOUNT_POINT,
                            "Expected destination to be \"{}\", got \"{}\"",
                            API_PIPES_MOUNT_POINT, destination
                        );
                        assert_eq!(
                            source, API_PIPES_LOCATION,
                            "Expected source to be \"{}\", got \"{}\"",
                            API_PIPES_LOCATION, source
                        );
                        assert_eq!(
                            mount_type, BIND_MOUNT,
                            "Expected type to be \"{}\", got \"{}\"",
                            BIND_MOUNT, mount_type
                        );

                        Some(())
                    })();
                    assert!(
                    assert_successful.is_some(),
                    "Expected call body ({}) to be valid parameter for /libpod/containers/create",
                    call_body
                );
                    true
                }),
        )
    }

    fn create_not_called_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .request_path("/libpod/containers/create")
                .times(0),
        )
    }

    fn create_error_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            ErrorResponseRequestHandler::default()
                .request_path("/libpod/containers/create")
                .status_code(StatusCode::BAD_REQUEST)
                .error_message("Simulated error"),
        )
    }

    fn start_successful_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .request_path("/libpod/containers/testid/start")
                .status_code(StatusCode::NO_CONTENT),
        )
    }

    fn start_not_called_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            BasicRequestHandler::default()
                .request_path("start")
                .times(0),
        )
    }

    fn start_error_handler() -> Box<dyn RequestHandler + Send + Sync> {
        Box::new(
            ErrorResponseRequestHandler::default()
                .request_path("/libpod/containers/testid/start")
                .status_code(StatusCode::NOT_FOUND)
                .error_message("Simulated rejection"),
        )
    }

    fn list_containers_handler() -> Box<dyn RequestHandler + Send + Sync> {
        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("Pending");
        Box::new(
            ListContainerRequestHandler::default()
                .resp_body(&vec![container_list_item])
                .times(1),
        )
    }

    async fn timeout<F: Future>(future: F) -> F::Output {
        tokio::time::timeout(Duration::from_millis(TEST_TIMEOUT), future)
            .await
            .expect("Test timed out")
    }
}

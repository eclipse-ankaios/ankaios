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

use common::state_change_interface::StateChangeInterface;
use common::state_change_interface::StateChangeSender;
use hyper::http;
use podman_api::models::ContainerMount;
use podman_api::models::ListContainer;
use podman_api::opts::ContainerCreateOpts;
use podman_api::opts::ContainerDeleteOpts;
use podman_api::opts::ContainerStopOpts;
use podman_api::opts::ImageListFilter;
use podman_api::opts::ImageListOpts;
use podman_api::opts::PullOpts;

use std::collections::HashMap;
use std::time::Duration;
use tokio::time;

use common::objects::ExecutionState;
use podman_api::{
    models::PortMapping,
    opts::{ContainerListFilter, ContainerListOpts},
    Podman,
};

use crate::podman::podman_runtime_config::{PodmanRuntimeConfig, Mount};


use common::objects::WorkloadExecutionInstanceName;
#[cfg(test)]
lazy_static::lazy_static! {
    pub static ref MOCK_PODMAN_UTILS_MTX: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
}
#[cfg(test)]
use mockall::automock;

use super::podman_runtime_config::Mapping;

// [impl->swdd~podman-workload-monitor-interval~1]
const STATUS_CHECK_INTERVAL_MS: u64 = 1000;

const API_PIPES_MOUNT_POINT: &str = "/run/ankaios/control_interface";
const BIND_MOUNT: &str = "bind";

pub fn convert_to_port_mapping(item: &[Mapping]) -> Vec<PortMapping> {
    item.iter()
        .map(|value| PortMapping {
            container_port: value.container_port.parse::<u16>().ok(),
            host_port: value.host_port.parse::<u16>().ok(),
            host_ip: None,
            protocol: None,
            range: None,
        })
        .collect()
}

#[derive(Debug)]
pub struct PodmanUtils {
    podman: Podman,
}

#[cfg_attr(test, automock)]
impl PodmanUtils {
    pub fn new(socket_path: String) -> Self {
        Self {
            podman: Podman::unix(socket_path.as_str()),
        }
    }

    pub async fn check_status(
        &self,
        manager_interface: StateChangeSender,
        agent_name: String,
        workload_name: String,
        container_id: String,
    ) {
        let mut last_state = ExecutionState::ExecUnknown;

        let mut interval = time::interval(Duration::from_millis(STATUS_CHECK_INTERVAL_MS));
        loop {
            interval.tick().await;
            let current_state = self.get_status(&container_id).await;

            if current_state != last_state {
                log::info!(
                    "The workload {} has changed its state to {:?}",
                    &workload_name,
                    &current_state
                );
                last_state = current_state.clone();

                // [impl->swdd~podman-workload-sends-workload-state~1]
                manager_interface
                    .update_workload_state(vec![common::objects::WorkloadState {
                        agent_name: agent_name.clone(),
                        workload_name: workload_name.to_string(),
                        execution_state: current_state,
                    }])
                    .await;

                if last_state == ExecutionState::ExecRemoved {
                    break;
                }
            }
        }
    }

    // [impl->swdd~podman-workload-state~1]
    // [impl->swdd~podman-workload-maps-state~1]
    async fn get_status(&self, container_id: &str) -> ExecutionState {
        let mut ret_state = ExecutionState::ExecUnknown;

        match self
            .podman
            .containers()
            .list(
                &ContainerListOpts::builder()
                    .all(true)
                    .filter([ContainerListFilter::Id(container_id.into())])
                    .build(),
            )
            .await
        {
            Ok(containers_state) => match containers_state.len() {
                1 => {
                    ret_state = Self::convert_podman_container_state_to_execution_state(
                        containers_state[0].to_owned(),
                    )
                }
                0 => ret_state = ExecutionState::ExecRemoved, // we know that container was removed
                _ => log::error!("Too many matches for the container Id '{}'", &container_id),
            },
            Err(e) => {
                log::warn!("Unable to get containers: {:?}", e);
            }
        };
        ret_state
    }

    // This conversion function is here to not pollute the
    // WorkloadExecutionInstanceName with podman specific handlings
    pub fn convert_to_ankaios_instance_name(
        agent_name: &str,
        list_container: &ListContainer,
    ) -> Option<WorkloadExecutionInstanceName> {
        let names_vec = list_container.names.clone()?;
        for name in names_vec {
            if let Some(instance_name) = WorkloadExecutionInstanceName::new(&name) {
                if agent_name == instance_name.agent_name() {
                    return Some(instance_name);
                }
            }
        }
        None
    }

    pub fn convert_podman_container_state_to_execution_state(
        value: podman_api::models::ListContainer,
    ) -> ExecutionState {
        if let Some(status) = &value.state {
            let is_status_exited = status.to_lowercase() == "exited"
                && value.exited.is_some()
                && value.exited.unwrap()
                && value.exit_code.is_some();
            match status.parse::<ExecutionState>() {
                Ok(_) if is_status_exited && value.exit_code.unwrap() == 0 => {
                    ExecutionState::ExecSucceeded
                }
                Ok(_) if is_status_exited && value.exit_code.unwrap() != 0 => {
                    ExecutionState::ExecFailed
                }
                Ok(st) => st,
                Err(_) => ExecutionState::ExecUnknown,
            }
        } else {
            ExecutionState::ExecUnknown
        }
    }

    pub async fn has_image(&self, image_name: &str) -> Result<bool, String> {
        match self
            .podman
            .images()
            .list(
                &ImageListOpts::builder()
                    .filter(vec![ImageListFilter::Reference(image_name.into(), None)])
                    .build(),
            )
            .await
        {
            Ok(list) => Ok(!list.is_empty()),
            Err(err) => Err(err.to_string()),
        }
    }

    async fn list_containers(&self, name_filter: &str) -> Result<Vec<ListContainer>, String> {
        self.podman
            .containers()
            .list(
                &ContainerListOpts::builder()
                    .all(true)
                    .filter([ContainerListFilter::Name(name_filter.to_string())])
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn pull_image(&self, image: &String) -> Result<(), String> {
        use futures_util::{StreamExt, TryStreamExt};

        self.podman
            .images()
            .pull(&PullOpts::builder().reference(image).build())
            .map(|report| {
                report.and_then(|report| match report.error {
                    Some(error) => Err(podman_api::Error::InvalidResponse(error)),
                    None => Ok(()),
                })
            })
            .try_collect()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn create_container(
        &self,
        workload_cfg: PodmanRuntimeConfig,
        container_name: String,
        api_pipes_location: String,
    ) -> Result<String, String> {
        match self
            .podman
            .containers()
            .create(
                &ContainerCreateOpts::builder()
                    .image(&workload_cfg.image)
                    .command(&workload_cfg.get_command_with_args())
                    .name(container_name)
                    .env(&workload_cfg.env)
                    .portmappings(convert_to_port_mapping(&workload_cfg.ports))
                    .mounts(Self::create_mounts(api_pipes_location, workload_cfg.mounts))
                    .remove(workload_cfg.remove)
                    .build(),
            )
            .await
        {
            Ok(response) => Ok(response.id),
            Err(err) => Err(err.to_string()),
        }
    }

    fn create_mounts(api_pipes_location: String, mounts: Vec<Mount>) -> Vec<ContainerMount> {
        let mut res = Vec::with_capacity(mounts.len() + 1);
        res.push(ContainerMount {
            destination: Some(String::from(API_PIPES_MOUNT_POINT)),
            options: None,
            source: Some(api_pipes_location),
            _type: Some(String::from(BIND_MOUNT)),
            uid_mappings: None,
            gid_mappings: None,
        });

        for m in mounts {
            res.push(m.into());
        }

        res
    }

    pub async fn start_container(&self, container_id: &str) -> Result<(), String> {
        self.podman
            .containers()
            .get(container_id)
            .start(None)
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn stop_container(&self, container_id: &str) -> Result<(), String> {
        match self
            .podman
            .containers()
            .get(container_id)
            .stop(&ContainerStopOpts::default())
            .await
        {
            Ok(()) => Ok(()),
            Err(podman_api::Error::Fault {
                code: http::StatusCode::NOT_MODIFIED,
                message,
            }) => {
                log::debug!(
                    "Cannot stop container '{}'. Already stopped. Message: '{}'",
                    container_id,
                    message
                );
                Ok(())
            }
            Err(podman_api::Error::Fault {
                code: http::StatusCode::NOT_FOUND,
                message,
            }) => {
                log::debug!(
                    "Cannot stop container '{}'. Not found. Message: '{}'",
                    container_id,
                    message
                );
                Ok(())
            }
            Err(err) => Err(err.to_string()),
        }
    }

    pub async fn delete_container(&self, container_id: &str) -> Result<(), String> {
        match self
            .podman
            .containers()
            .get(container_id)
            .delete(&ContainerDeleteOpts::builder().volumes(true).build())
            .await
        {
            Ok(()) => Ok(()),
            Err(podman_api::Error::Fault {
                code: http::StatusCode::NOT_FOUND,
                message,
            }) => {
                log::debug!(
                    "Cannot delete container '{}'. Not found. Message: '{}'",
                    container_id,
                    message
                );
                Ok(())
            }
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn remove_containers(
        socket_path: &str,
        instance_map: HashMap<String, (WorkloadExecutionInstanceName, String)>,
    ) {
        for (_, (_, container_id)) in instance_map {
            let podman_utils = PodmanUtils::new(socket_path.to_string());
            tokio::spawn(async move {
                if let Err(err) = (|| async {
                    podman_utils.stop_container(&container_id).await?;
                    podman_utils.delete_container(&container_id).await?;
                    Result::<(), String>::Ok(())
                })()
                .await
                {
                    log::error!(
                        "Could not stop and delete container '{container_id}'. Error: {err}"
                    );
                }
            });
        }
    }

    // [impl->swdd~agent-adapter-start-finds-existing-workloads~1]
    pub async fn list_running_workloads(
        socket_path: &str,
        agent_name: &str,
    ) -> HashMap<String, (WorkloadExecutionInstanceName, String)> {
        let mut running_workloads = HashMap::new();

        let name_filter = WorkloadExecutionInstanceName::get_agent_filter_regex(agent_name);

        let podman_utils = PodmanUtils::new(socket_path.to_string());

        match podman_utils.list_containers(&name_filter).await {
            Ok(container_list) => {
                for container in container_list {
                    if let Some(instance_name) =
                        PodmanUtils::convert_to_ankaios_instance_name(agent_name, &container)
                    {
                        if let Some(container_id) = container.id {
                            running_workloads.insert(
                                instance_name.workload_name().to_string(),
                                (instance_name, container_id),
                            );
                        } else {
                            log::error!(
                                "Could not add running workload '{}', container id is None.",
                                instance_name.workload_name()
                            );
                        }
                    }
                }
            }
            Err(err) => log::warn!("Could not list podman containers. Error: '{err}'"),
        }

        running_workloads
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
pub mod test_utils {
    use podman_api::models::ListContainer;

    pub fn generate_test_empty_list_container() -> ListContainer {
        ListContainer {
            auto_remove: None,
            command: None,
            created: None,
            created_at: None,
            exit_code: None,
            exited: None,
            exited_at: None,
            id: None,
            image: None,
            image_id: None,
            is_infra: None,
            labels: None,
            mounts: None,
            names: None,
            namespaces: None,
            networks: None,
            pid: None,
            pod: None,
            pod_name: None,
            ports: None,
            size: None,
            started_at: None,
            state: None,
            status: None,
        }
    }

    pub fn generate_test_list_container_with_state(
        exit_code: Option<i32>,
        exited: Option<bool>,
        state: Option<String>,
    ) -> ListContainer {
        let mut list_item = generate_test_empty_list_container();
        list_item.exit_code = exit_code;
        list_item.exited = exited;
        list_item.state = state;

        list_item
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use common::objects::ExecutionState;
    use hyper::StatusCode;

    use crate::podman::{
        podman_utils::{
            test_utils::{
                generate_test_empty_list_container, generate_test_list_container_with_state,
            },
            PodmanUtils,
        },
        test_utils::{
            request_handlers::{
                handler_helpers::*, ErrorResponseRequestHandler, ListContainerRequestHandler,
                RequestHandler, WithRequestHandlerParameter,
            },
            server_models::ServerListContainer,
            test_daemon::PodmanTestDaemon,
        },
    };
    use common::objects::WorkloadExecutionInstanceName;

    const CONTAINER_ID: &str = "testid";
    const WORKLOAD_NAME: &str = "workload_name";
    const WORKLOAD_NAME_2: &str = "workload_name_2";
    const CONTAINER_ID_2: &str = "some_other_id";

    // [utest->swdd~agent-adapter-start-finds-existing-workloads~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_list_containers() {
        let _ = env_logger::builder().is_test(true).try_init();
        let container_names = vec![
            "scsdcd213ewed".to_string(),
            "workload.bhvgvghv4rg4.agent_1".to_string(),
        ];
        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("Dead");
        container_list_item.names = container_names.clone();

        let handler = ListContainerRequestHandler::default().resp_body(&vec![container_list_item]);

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;
        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());

        let result = podman_utils.list_containers(".agent_1").await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].names, Some(container_names));

        test_daemon.check_calls_and_stop();
    }

    // [utest->swdd~podman-workload-state~1]
    // [utest->swdd~podman-workload-maps-state~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn utest_status_get_success() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("Dead");

        let handler = ListContainerRequestHandler::default().resp_body(&vec![container_list_item]);

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        let status = podman_utils
            .get_status(&String::from("test_workload"))
            .await;
        assert_eq!(status, ExecutionState::ExecUnknown);

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_status_get_bad_request() {
        let _ = env_logger::builder().is_test(true).try_init();

        let handler = ErrorResponseRequestHandler::default()
            .request_path("/libpod/containers/json")
            .status_code(StatusCode::BAD_REQUEST)
            .error_message("Simulated rejection");

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert_eq!(
            podman_utils
                .get_status(&String::from("test_workload"))
                .await,
            ExecutionState::ExecUnknown
        );

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_status_get_empty_list_returned() {
        let _ = env_logger::builder().is_test(true).try_init();

        let handler = ListContainerRequestHandler::default().resp_body(&vec![]);

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert_eq!(
            podman_utils
                .get_status(&String::from("test_workload"))
                .await,
            ExecutionState::ExecRemoved
        );

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_status_get_undefined_status() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("Undefined");

        let handler = ListContainerRequestHandler::default().resp_body(&vec![container_list_item]);

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert_eq!(
            podman_utils
                .get_status(&String::from("test_workload"))
                .await,
            ExecutionState::ExecUnknown
        );

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_status_get_status_key_missing() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("BrokenValue");
        let container_list_str = serde_json::to_string(&vec![container_list_item]).unwrap();

        let handler = ListContainerRequestHandler::default()
            .resp_body_as_str(container_list_str.replace("State", "BrokenKey"));

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert_eq!(
            podman_utils
                .get_status(&String::from("test_workload"))
                .await,
            ExecutionState::ExecUnknown
        );

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_status_get_no_connetion_to_socket() {
        let _ = env_logger::builder().is_test(true).try_init();

        let podman_utils = PodmanUtils::new(String::from("/not/running/socket"));
        assert_eq!(
            podman_utils
                .get_status(&String::from("test_workload"))
                .await,
            ExecutionState::ExecUnknown
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_stop_container_success() {
        let _ = env_logger::builder().is_test(true).try_init();

        let test_daemon = PodmanTestDaemon::create(vec![stop_success_handler(CONTAINER_ID)]).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert!(podman_utils.stop_container(CONTAINER_ID).await.is_ok());

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_stop_container_failed() {
        let _ = env_logger::builder().is_test(true).try_init();

        let test_daemon = PodmanTestDaemon::create(vec![stop_error_handler(CONTAINER_ID)]).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert!(podman_utils.stop_container(CONTAINER_ID).await.is_err());

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_delete_container_success() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut test_daemon =
            PodmanTestDaemon::create(vec![delete_success_handler(CONTAINER_ID)]).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert!(podman_utils.delete_container(CONTAINER_ID).await.is_ok());

        test_daemon.wait_expected_requests_done().await;

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_delete_container_failed() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut test_daemon =
            PodmanTestDaemon::create(vec![delete_error_handler(CONTAINER_ID)]).await;

        let podman_utils = PodmanUtils::new(test_daemon.socket_path.clone());
        assert!(podman_utils.delete_container(CONTAINER_ID).await.is_err());

        test_daemon.wait_expected_requests_done().await;

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_remove_containers_success() {
        let mut unneeded_workloads = HashMap::new();
        unneeded_workloads.insert(
            WORKLOAD_NAME.to_string(),
            (
                WorkloadExecutionInstanceName::builder().build(),
                CONTAINER_ID.to_string(),
            ),
        );
        unneeded_workloads.insert(
            WORKLOAD_NAME_2.to_string(),
            (
                WorkloadExecutionInstanceName::builder().build(),
                CONTAINER_ID_2.to_string(),
            ),
        );

        let mut test_daemon = PodmanTestDaemon::create(vec![
            stop_success_handler(CONTAINER_ID),
            delete_success_handler(CONTAINER_ID),
            stop_success_handler(CONTAINER_ID_2),
            delete_success_handler(CONTAINER_ID_2),
        ])
        .await;

        PodmanUtils::remove_containers(test_daemon.socket_path.as_str(), unneeded_workloads);

        test_daemon.wait_expected_requests_done().await;

        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_remove_containers_one_failed() {
        let mut unneeded_workloads = HashMap::new();
        unneeded_workloads.insert(
            WORKLOAD_NAME.to_string(),
            (
                WorkloadExecutionInstanceName::builder().build(),
                CONTAINER_ID.to_string(),
            ),
        );
        unneeded_workloads.insert(
            WORKLOAD_NAME_2.to_string(),
            (
                WorkloadExecutionInstanceName::builder().build(),
                CONTAINER_ID_2.to_string(),
            ),
        );

        let mut test_daemon = PodmanTestDaemon::create(vec![
            stop_error_handler(CONTAINER_ID),
            delete_not_called_handler(CONTAINER_ID),
            stop_success_handler(CONTAINER_ID_2),
            delete_success_handler(CONTAINER_ID_2),
        ])
        .await;

        PodmanUtils::remove_containers(test_daemon.socket_path.as_str(), unneeded_workloads);

        test_daemon.wait_expected_requests_done().await;

        test_daemon.check_calls_and_stop();
    }

    #[test]
    fn utest_execution_state_from_podman_states() {
        let ec_success = Some(0);
        let ec_failed = Some(1);
        let ec_none: Option<i32> = None;
        let finished = Some(true);
        let not_finished = Some(false);

        let item_running_state = generate_test_list_container_with_state(
            ec_success,
            not_finished,
            Some("running".to_string()),
        );
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_running_state),
            ExecutionState::ExecRunning
        );

        let item_running_state = generate_test_list_container_with_state(
            ec_success,
            not_finished,
            Some("pending".to_string()),
        );
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_running_state),
            ExecutionState::ExecPending
        );

        let item_created_state = generate_test_list_container_with_state(
            ec_none,
            not_finished,
            Some("Created".to_string()),
        );
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_created_state),
            ExecutionState::ExecPending
        );

        let item_paused_state = generate_test_list_container_with_state(
            ec_none,
            not_finished,
            Some("Paused".to_string()),
        );
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_paused_state),
            ExecutionState::ExecUnknown
        );

        let item_unknown_state = generate_test_list_container_with_state(
            ec_success,
            not_finished,
            Some("Unknown".to_string()),
        );
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_unknown_state),
            ExecutionState::ExecUnknown
        );

        let item_exited_succeeded = generate_test_list_container_with_state(
            ec_success,
            finished,
            Some("Exited".to_string()),
        );
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_exited_succeeded),
            ExecutionState::ExecSucceeded
        );

        let item_exited_failed = generate_test_list_container_with_state(
            ec_failed,
            finished,
            Some("Exited".to_string()),
        );
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_exited_failed),
            ExecutionState::ExecFailed
        );

        // Following combinations are rather unrealistic, but we should test that it behaves correctly.
        let item_no_exit_code =
            generate_test_list_container_with_state(None, finished, Some("exited".to_string()));
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_no_exit_code),
            ExecutionState::ExecUnknown
        );

        let item_success_no_exited =
            generate_test_list_container_with_state(ec_success, None, Some("exited".to_string()));
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_success_no_exited),
            ExecutionState::ExecUnknown
        );

        let item_success_no_state =
            generate_test_list_container_with_state(ec_success, finished, None);
        assert_eq!(
            PodmanUtils::convert_podman_container_state_to_execution_state(item_success_no_state),
            ExecutionState::ExecUnknown
        );
    }

    #[test]
    fn utest_convert_to_ankaios_instance_name() {
        let workload_instance_name = WorkloadExecutionInstanceName::builder()
            .agent_name("agent_x")
            .workload_name("fancy_name-1")
            .config(&String::from("fancy config"))
            .build();

        let container_names = vec![
            "bjhc76ec87c78yds78cyds8".to_string(),
            workload_instance_name.to_string(),
        ];

        let mut list_container = generate_test_empty_list_container();

        assert_eq!(
            PodmanUtils::convert_to_ankaios_instance_name(
                workload_instance_name.agent_name(),
                &list_container
            ),
            None
        );

        list_container.names = Some(container_names);

        assert_eq!(
            PodmanUtils::convert_to_ankaios_instance_name(
                workload_instance_name.agent_name(),
                &list_container
            ),
            Some(workload_instance_name)
        );

        assert_eq!(
            PodmanUtils::convert_to_ankaios_instance_name("some other agent name", &list_container),
            None
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_list_running_workloads() {
        let _ = env_logger::builder().is_test(true).try_init();

        let expected_container_id = "1234567".to_string();
        let expected_agent = "agent_1".to_string();

        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("Running");
        container_list_item.names = vec![
            "workload1.2a301c7d8b7a94f51214ed5a6bd9b6347b460179ec8b482b3dfe19512cd1a307.agent_1"
                .to_string(),
        ];
        container_list_item.id = Some(expected_container_id.clone());

        let handler = ListContainerRequestHandler::default().resp_body(&vec![container_list_item]);

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let running_workloads = PodmanUtils::list_running_workloads(
            test_daemon.socket_path.as_str(),
            expected_agent.as_str(),
        )
        .await;

        assert_eq!(running_workloads.len(), 1);
        let (workload_instance_name, container_id) =
            running_workloads.get(&"workload1".to_string()).unwrap();

        assert_eq!(container_id, &expected_container_id);
        assert_eq!(workload_instance_name.agent_name(), expected_agent);
        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_list_running_workloads_wrong_instance_name() {
        let _ = env_logger::builder().is_test(true).try_init();

        let expected_container_id = "1234567".to_string();
        let expected_agent = "agent_1".to_string();

        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("Running");
        container_list_item.names = vec!["wrong_name".to_string()];
        container_list_item.id = Some(expected_container_id.clone());

        let handler = ListContainerRequestHandler::default().resp_body(&vec![container_list_item]);

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let running_workloads = PodmanUtils::list_running_workloads(
            test_daemon.socket_path.as_str(),
            expected_agent.as_str(),
        )
        .await;
        assert!(running_workloads.is_empty());
        test_daemon.check_calls_and_stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn utest_list_running_workloads_no_container_id() {
        let _ = env_logger::builder().is_test(true).try_init();

        let mut container_list_item = ServerListContainer::new();
        container_list_item.state = String::from("Running");
        container_list_item.names = vec![
            "workload1.2a301c7d8b7a94f51214ed5a6bd9b6347b460179ec8b482b3dfe19512cd1a307.agent_1"
                .to_string(),
        ];
        container_list_item.id = None;

        let handler = ListContainerRequestHandler::default().resp_body(&vec![container_list_item]);

        let request_handlers = vec![Box::new(handler) as Box<dyn RequestHandler + Sync + Send>];

        let test_daemon = PodmanTestDaemon::create(request_handlers).await;

        let expected_agent = "agent_1".to_string();
        let running_workloads = PodmanUtils::list_running_workloads(
            test_daemon.socket_path.as_str(),
            expected_agent.as_str(),
        )
        .await;
        assert!(running_workloads.is_empty());
        test_daemon.check_calls_and_stop();
    }
}

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

use std::collections::HashMap;
use std::path::PathBuf;

use common::objects::WorkloadSpec;
use common::state_change_interface::StateChangeSender;

use async_trait::async_trait;

#[cfg(test)]
use mockall_double::double;

use crate::runtime_adapter::RuntimeAdapter;
use common::objects::WorkloadInstanceName;

#[cfg_attr(test, double)]
use crate::workload_facade::WorkloadFacade;

const PODMAN_ADAPTER_NAME: &str = "podman";

#[cfg_attr(test, double)]
use super::podman_utils::PodmanUtils;

use super::workload::PodmanWorkload;

type Workload = WorkloadFacade<PodmanWorkload>;

pub struct PodmanAdapter {
    run_folder: PathBuf,
    manager_interface: StateChangeSender,
    socket_path: String,
    running_workloads: HashMap<String, Workload>,
}

impl PodmanAdapter {
    pub fn new(
        run_folder: PathBuf,
        manager_interface: StateChangeSender,
        socket_path: String,
    ) -> Self {
        Self {
            run_folder,
            manager_interface,
            socket_path,
            running_workloads: HashMap::new(),
        }
    }
}

// Note: the trait implementation here shall be tested, but it is not possible to cover the code with unit tests.
// Therefore the code is left as uncovered.
#[async_trait]
impl RuntimeAdapter for PodmanAdapter {
    async fn start(&mut self, agent_name: &str, initial_workload_list: Vec<WorkloadSpec>) {
        // [impl->swdd~agent-adapter-start-finds-existing-workloads~1]
        let mut found_running_workloads =
            PodmanUtils::list_running_workloads(&self.socket_path, agent_name).await;

        log::debug!(
            "Starting podman adapter. Found the following still running workloads: {:?}",
            found_running_workloads
        );

        // First go through everything that should run
        for workload_spec in initial_workload_list {
            if let Some((found_instance_name, found_container_id)) =
                found_running_workloads.remove(&workload_spec.workload.name)
            {
                // the workload name is the same, let's check the instance name which includes a config hash
                let replace_required = found_instance_name != workload_spec.instance_name();

                log::info!("Found existing workload execution instance '{found_instance_name}'. Replace required: '{replace_required}'");

                let workload_name = workload_spec.workload.name.clone();
                let podman_workload = PodmanWorkload::new(
                    self.manager_interface.clone(),
                    self.socket_path.clone(),
                    workload_spec,
                    &self.run_folder,
                );

                let workload = if replace_required {
                    // [impl->swdd~agent-adapter-start-replace-updated~1]
                    Workload::replace(found_instance_name, found_container_id, podman_workload)
                } else {
                    // [impl->swdd~agent-adapter-start-resume-existing~1]
                    Workload::resume(podman_workload, found_container_id)
                };

                self.running_workloads.insert(workload_name, workload);
            } else {
                // [impl->swdd~agent-adapter-start-new-workloads-if-non-found~1]
                self.add_workload(workload_spec);
            }
        }

        // Now stop the remaining things that should not run anymore
        // [impl->swdd~agent-adapter-start-unneeded-stopped~1]
        if !found_running_workloads.is_empty() {
            PodmanUtils::remove_containers(&self.socket_path, found_running_workloads);
        }
    }

    fn get_name(&self) -> &'static str {
        PODMAN_ADAPTER_NAME
    }

    // [impl->swdd~podman-adapter-creates-starts-podman-workload~2]
    fn add_workload(&mut self, workload_spec: WorkloadSpec) {
        log::info!("Starting workload '{}' ...", workload_spec.workload.name);

        let workload_name = workload_spec.workload.name.clone();

        let podman_workload = PodmanWorkload::new(
            self.manager_interface.clone(),
            self.socket_path.clone(),
            workload_spec,
            &self.run_folder,
        );

        let workload = Workload::start(podman_workload);
        // [impl->swdd~podman-adapter-stores-podman-workload~2]
        self.running_workloads.insert(workload_name, workload);
    }

    async fn update_workload(&mut self, workload_spec: WorkloadSpec) {
        log::info!("Updating workload '{}' ...", workload_spec.workload.name);

        let workload_name = &workload_spec.workload.name;

        if let Some(workload_facade) = self.running_workloads.get(workload_name) {
            let podman_workload = PodmanWorkload::new(
                self.manager_interface.clone(),
                self.socket_path.clone(),
                workload_spec,
                &self.run_folder,
            );

            workload_facade.update(podman_workload).await;
        } else {
            log::warn!("Could not find existing workload '{workload_name}', starting a new one.");
            return self.add_workload(workload_spec);
        }
    }

    async fn delete_workload(&mut self, workload_name: &str) {
        log::info!("Deleting workload '{}'.", workload_name);

        // [impl->swdd~podman-adapter-removes-podman-workload~2]
        if let Some(podman_workload) = self.running_workloads.remove(workload_name) {
            // [impl->swdd~podman-adapter-request-stopping-container~2]
            podman_workload.stop().await;
        }
    }

    async fn stop(&self) {}
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
    use std::collections::HashMap;
    use std::path::Path;

    use super::PodmanAdapter;
    use crate::podman::podman_utils::MockPodmanUtils;
    use crate::podman::workload::PodmanWorkload;
    use crate::runtime_adapter::RuntimeAdapter;
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;
    use crate::workload_facade::MockWorkloadFacade;
    use common::objects::WorkloadInstanceName;
    use common::state_change_interface::StateChangeCommand;
    use common::test_utils;
    use mockall::predicate;
    use tokio::task::yield_now;

    type MockWorkload = MockWorkloadFacade<PodmanWorkload>;

    const RUN_DIR: &str = "/tmp/base/path";
    const AGENT_NAME: &str = "test_agent";
    const WORKLOAD_NAME: &str = "workload X";
    const RUNTIME_NAME: &str = "my favorite runtime";
    const CONTAINER_ID: &str = "9jmdc923dxcsdlmc2pedjd9jcwdkl";

    #[test]
    fn utest_podman_adapter_get_name() {
        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let podman_adapter = PodmanAdapter::new(
            Path::new("/tmp/base/path").to_path_buf(),
            to_server,
            "/not/really/the/podman/socket".to_string(),
        );

        assert_eq!(podman_adapter.get_name(), "podman");
    }

    // [utest->swdd~agent-adapter-start-resume-existing~1]
    // [utest->swdd~podman-adapter-stores-podman-workload~2]
    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_start_resume() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let workload_spec = test_utils::generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mock_podman_start_context = MockWorkload::start_context();
        mock_podman_start_context.expect().never();

        let workload_spec_clone = workload_spec.clone();
        let mock_podman_workload_context_resume = MockWorkload::resume_context();
        mock_podman_workload_context_resume
            .expect()
            .once()
            .with(
                predicate::function(move |workload: &PodmanWorkload| {
                    format!("{workload:?}").contains(&format!("{workload_spec_clone:?}"))
                }),
                predicate::eq(CONTAINER_ID.to_string()),
            )
            .return_once(|_, _| MockWorkload::default());

        let mut running_workloads_list = HashMap::new();
        running_workloads_list.insert(
            WORKLOAD_NAME.to_string(),
            (workload_spec.instance_name(), CONTAINER_ID.to_string()),
        );

        let list_container_context = MockPodmanUtils::list_running_workloads_context();
        list_container_context
            .expect()
            .once()
            .with(
                predicate::always(), // Ignore the first argument as Podman does not implement PartialEq
                predicate::eq(AGENT_NAME),
            )
            .return_const(running_workloads_list);

        let mut podman_adapter = PodmanAdapter::new(
            Path::new(RUN_DIR).to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter.start(AGENT_NAME, vec![workload_spec]).await;

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_some());
    }

    // [utest->swdd~agent-adapter-start-new-workloads-if-non-found~1]
    // [utest->swdd~podman-adapter-stores-podman-workload~2]
    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_start_add_new() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let workload_spec = test_utils::generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let workload_spec_clone = workload_spec.clone();
        let start_context = MockWorkload::start_context();
        start_context
            .expect()
            .once()
            .with(predicate::function(move |workload: &PodmanWorkload| {
                format!("{workload:?}").contains(&format!("{workload_spec_clone:?}"))
            }))
            .return_once(|_| MockWorkload::default());

        let list_container_context = MockPodmanUtils::list_running_workloads_context();
        list_container_context
            .expect()
            .once()
            .with(
                predicate::always(), // Ignore the first argument as Podman does not implement PartialEq
                predicate::eq(AGENT_NAME),
            )
            .return_const(HashMap::new());

        let mut podman_adapter = PodmanAdapter::new(
            Path::new(RUN_DIR).to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter.start(AGENT_NAME, vec![workload_spec]).await;

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_some());
    }

    // [utest->swdd~agent-adapter-start-replace-updated~1]
    // [utest->swdd~podman-adapter-stores-podman-workload~2]
    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_start_replace_existing() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let old_container_id = "old_container_id";
        let mut workload_spec_old = test_utils::generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );
        let workload_spec_new = workload_spec_old.clone();

        workload_spec_old.workload.runtime_config = "a fancy updated runtime config".to_string();

        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut mock_workload_facade = MockWorkload::default();
        mock_workload_facade.expect_stop().never();

        let workload_spec_clone = workload_spec_new.clone();
        let replace_context = MockWorkload::replace_context();
        replace_context
            .expect()
            .once()
            .with(
                predicate::eq(workload_spec_old.instance_name()),
                predicate::eq(old_container_id.to_string()),
                predicate::function(move |workload: &PodmanWorkload| {
                    format!("{workload:?}").contains(&format!("{workload_spec_clone:?}"))
                }),
            )
            .return_once(|_, _, _| mock_workload_facade);

        let mut running_workloads_list = HashMap::new();
        running_workloads_list.insert(
            WORKLOAD_NAME.to_string(),
            (
                workload_spec_old.instance_name(),
                old_container_id.to_string(),
            ),
        );

        let list_container_context = MockPodmanUtils::list_running_workloads_context();
        list_container_context
            .expect()
            .once()
            .with(
                predicate::always(), // Ignore the first argument as Podman does not implement PartialEq
                predicate::eq(AGENT_NAME),
            )
            .return_const(running_workloads_list);

        let mut podman_adapter = PodmanAdapter::new(
            Path::new(RUN_DIR).to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter
            .start(AGENT_NAME, vec![workload_spec_new])
            .await;

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_some());
    }

    // [utest->swdd~agent-adapter-start-unneeded-stopped~1]
    // [utest->swdd~podman-adapter-stores-podman-workload~2]
    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_start_delete_unneeded() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let workload_spec = test_utils::generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let mut running_workloads_list = HashMap::new();
        running_workloads_list.insert(
            WORKLOAD_NAME.to_string(),
            (workload_spec.instance_name(), CONTAINER_ID.to_string()),
        );

        let list_container_context = MockPodmanUtils::list_running_workloads_context();
        list_container_context
            .expect()
            .once()
            .with(
                predicate::always(), // Ignore the first argument as Podman does not implement PartialEq
                predicate::eq(AGENT_NAME),
            )
            .return_const(running_workloads_list.clone());

        let list_container_context = MockPodmanUtils::remove_containers_context();
        list_container_context
            .expect()
            .once()
            .with(
                predicate::always(), // Ignore the first argument as Podman does not implement PartialEq
                predicate::eq(running_workloads_list),
            )
            .return_const(());

        let mut podman_adapter = PodmanAdapter::new(
            Path::new(RUN_DIR).to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter.start(AGENT_NAME, vec![]).await;

        // We have to wait until the task spawned in add_workload is executed.
        // The test is explicitly single thread, s.t. a simple yield is enough here.
        yield_now().await;

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_none());
    }

    // [utest->swdd~podman-adapter-stores-podman-workload~2]
    // [utest->swdd~podman-adapter-creates-starts-podman-workload~2]
    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_add_workload() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let workload_spec = test_utils::generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let workload_spec_clone = workload_spec.clone();
        let start_context = MockWorkload::start_context();
        start_context
            .expect()
            .once()
            .with(predicate::function(move |workload: &PodmanWorkload| {
                format!("{workload:?}").contains(&format!("{workload_spec_clone:?}"))
            }))
            .return_once(|_| MockWorkload::default());

        let mut podman_adapter = PodmanAdapter::new(
            Path::new(RUN_DIR).to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter.add_workload(workload_spec);

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_some());
    }

    // [utest->swdd~podman-adapter-removes-podman-workload~2]
    // [utest->swdd~podman-adapter-request-stopping-container~2]
    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_delete_workload() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let mut mock_podman_workload = MockWorkload::default();
        mock_podman_workload.expect_stop().once().return_const(());

        let mut podman_adapter = PodmanAdapter::new(
            Path::new("/tmp/base/path").to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter
            .running_workloads
            .insert(WORKLOAD_NAME.to_string(), mock_podman_workload);

        podman_adapter.delete_workload(WORKLOAD_NAME).await;

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_update_workload_update_existing() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let workload_spec = test_utils::generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let workload_spec_clone = workload_spec.clone();
        let mut mock_podman_workload = MockWorkload::default();
        mock_podman_workload
            .expect_update()
            .once()
            .with(predicate::function(move |workload: &PodmanWorkload| {
                format!("{workload:?}").contains(&format!("{workload_spec_clone:?}"))
            }))
            .return_const(());

        let mut podman_adapter = PodmanAdapter::new(
            Path::new("/tmp/base/path").to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter
            .running_workloads
            .insert(WORKLOAD_NAME.to_string(), mock_podman_workload);

        podman_adapter.update_workload(workload_spec).await;

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn utest_podman_adapter_update_workload_add_new() {
        let _ = env_logger::builder().is_test(true).try_init();

        let (to_server, mut _from_agent) = tokio::sync::mpsc::channel::<StateChangeCommand>(1);

        let workload_spec = test_utils::generate_test_workload_spec_with_param(
            AGENT_NAME.to_string(),
            WORKLOAD_NAME.to_string(),
            RUNTIME_NAME.to_string(),
        );

        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let workload_spec_clone = workload_spec.clone();
        let start_context = MockWorkload::start_context();
        start_context
            .expect()
            .once()
            .with(predicate::function(move |workload: &PodmanWorkload| {
                format!("{workload:?}").contains(&format!("{workload_spec_clone:?}"))
            }))
            .return_once(|_| MockWorkload::default());

        let mut podman_adapter = PodmanAdapter::new(
            Path::new("/tmp/base/path").to_path_buf(),
            to_server,
            "/no/such/socket".to_string(),
        );

        podman_adapter.update_workload(workload_spec).await;

        assert!(podman_adapter
            .running_workloads
            .get(WORKLOAD_NAME)
            .is_some());
    }
}

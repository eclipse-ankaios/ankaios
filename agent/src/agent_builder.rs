// Copyright (c) 2026 Elektrobit Automotive GmbH
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

use crate::runtime_connectors::OwnableRuntime;
#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;
use crate::runtime_registry::RuntimeRegistry;

use crate::agent_config::AgentConfig;
use crate::agent_manager::AgentManager;
use crate::io_utils;
#[cfg_attr(test, mockall_double::double)]
use crate::io_utils::Directory;
use ankaios_api::ank_base::WorkloadStateSpec;

use common::{
    communications_client::CommunicationsClient,
    from_server_interface::{FromServer, FromServerSender},
    objects::AgentName,
    std_extensions::GracefulExitResult,
    to_server_interface::{ToServer, ToServerReceiver},
};

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_connectors::RuntimeFacade;

#[cfg_attr(test, mockall_double::double)]
use grpc::client::GRPCCommunicationsClient;
use std::collections::HashMap;

pub(crate) const BUFFER_SIZE: usize = 20;

/// Builder for assembling and running an Ankaios agent with a chosen set of runtimes.
///
/// Config is resolved by the caller (e.g. via `AgentConfig::load()`) before building runtimes or
/// a communications client, so both can be constructed from the resolved `AgentConfig`.
pub struct AgentBuilder {
    available_runtimes: RuntimeRegistry,
    communications_client: Box<dyn CommunicationsClient>,
}

struct RunningAgent {
    communications_client: Box<dyn CommunicationsClient>,
    server_receiver: ToServerReceiver,
    to_manager: FromServerSender,
    agent_manager: AgentManager,
    // Held for its Drop impl, which cleans up the run folder; must outlive the agent's run.
    run_directory: Directory,
}

impl AgentBuilder {
    pub fn new(communications_client: Box<dyn CommunicationsClient>) -> Self {
        Self {
            available_runtimes: RuntimeRegistry::default(),
            communications_client,
        }
    }

    pub fn with_runtime(mut self, runtime: Box<dyn OwnableRuntime>) -> Self {
        self.available_runtimes.register(runtime);
        self
    }

    // [impl->swdd~agent-default-communication-grpc~1]
    pub fn default_communications_client(
        agent_config: &AgentConfig,
    ) -> Box<dyn CommunicationsClient> {
        let tls_config = agent_config.build_tls_config();
        Box::new(
            GRPCCommunicationsClient::new_agent_communication(
                agent_config.name.clone(),
                agent_config.server_url.clone(),
                agent_config.tags.clone(),
                tls_config,
            )
            .unwrap_or_exit("Failed to create communications client."),
        )
    }

    pub async fn run(self, agent_config: AgentConfig) {
        log::debug!(
            "Starting the Ankaios agent with \n\tname: '{}', \n\tserver url: '{}', \n\trun directory: '{}'",
            agent_config.name,
            agent_config.server_url,
            agent_config.run_folder,
        );

        let RunningAgent {
            mut communications_client,
            server_receiver,
            to_manager,
            mut agent_manager,
            run_directory: _run_directory,
        } = self.build(&agent_config);

        tokio::select! {
            // [impl->swdd~agent-sends-hello~1]
            communication_result = communications_client.run(server_receiver, to_manager) => {
                communication_result.unwrap_or_exit("agent error")
            }
            _agent_mgr_result = agent_manager.start() => {
                log::info!("AgentManager exited.");
            }
        }
    }

    fn build(self, agent_config: &AgentConfig) -> RunningAgent {
        // [impl->swdd~agent-uses-async-channels~1]
        let (to_manager, manager_receiver) = tokio::sync::mpsc::channel::<FromServer>(BUFFER_SIZE);
        let (to_server, server_receiver) = tokio::sync::mpsc::channel::<ToServer>(BUFFER_SIZE);
        let (workload_state_sender, workload_state_receiver) =
            tokio::sync::mpsc::channel::<WorkloadStateSpec>(BUFFER_SIZE);

        let run_directory = Self::prepare_run_directory(agent_config);
        let run_directory_path = run_directory.get_path();

        let selected_runtimes =
            Self::select_runtimes(self.available_runtimes, &agent_config.runtimes);

        let runtime_facade_map: HashMap<String, RuntimeFacade> = selected_runtimes
            .into_iter()
            .map(|(name, runtime)| {
                let facade = RuntimeFacade::new(runtime, run_directory_path.clone());
                (name, facade)
            })
            .collect();

        // The RuntimeManager currently directly gets the server ToServerInterface, but it shall get the agent manager interface
        // This is needed to be able to filter/authorize the commands towards the Ankaios server
        // The pipe connecting the workload to Ankaios must be in the runtime adapter
        let runtime_manager = RuntimeManager::new(
            AgentName::from(agent_config.name.as_str()),
            run_directory_path,
            to_server.clone(),
            runtime_facade_map,
            workload_state_sender,
        );

        let agent_manager = AgentManager::new(
            agent_config.name.clone(),
            manager_receiver,
            runtime_manager,
            to_server,
            workload_state_receiver,
        );

        RunningAgent {
            communications_client: self.communications_client,
            server_receiver,
            to_manager,
            agent_manager,
            run_directory,
        }
    }

    // [impl->swdd~agent-prepares-dedicated-run-folder~3]
    fn prepare_run_directory(agent_config: &AgentConfig) -> Directory {
        io_utils::prepare_agent_run_directory(
            agent_config.run_folder.as_str(),
            agent_config.name.as_str(),
        )
        .unwrap_or_exit("Run folder creation failed. Cannot continue without run folder.")
    }

    // [impl->swdd~agent-allows-enabled-runtimes~1]
    fn select_runtimes(
        available_runtimes: RuntimeRegistry,
        enabled_runtimes: &Option<Vec<String>>,
    ) -> HashMap<String, Box<dyn OwnableRuntime>> {
        match enabled_runtimes {
            Some(enabled_runtimes) => {
                let selected = available_runtimes.select(enabled_runtimes);
                if selected.is_empty() {
                    Err(format!(
                        "No valid runtimes configured. Enabled runtimes: {enabled_runtimes:?}"
                    ))
                } else {
                    Ok(selected)
                }
                .unwrap_or_exit("Invalid runtime configuration")
            }
            None => available_runtimes.into(),
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
    use super::{AgentBuilder, GRPCCommunicationsClient, OwnableRuntime};
    use crate::agent_config::AgentConfig;
    use crate::runtime_connectors::test::MockRuntimeConnector;
    use crate::runtime_registry::RuntimeRegistry;
    use common::communications_client::CommunicationsClient;
    use common::communications_error::CommunicationMiddlewareError;
    use common::config::ServerUrl;
    use common::from_server_interface::FromServerSender;
    use common::objects::AgentName;
    use common::to_server_interface::ToServerReceiver;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TEST_AGENT_NAME: &str = "agent_A";
    const TEST_SERVER_URL_TCP: &str = "http://127.0.0.1:25551";
    const TEST_SERVER_URL_UNIX: &str = "/tmp/ankaios.sock";
    const TEST_TAG_KEY: &str = "key";
    const TEST_TAG_VALUE: &str = "value";
    const TEST_CA_PEM: &str = "ca_pem";
    const TEST_CRT_PEM: &str = "crt_pem";
    const TEST_KEY_PEM: &str = "key_pem";

    // [utest->swdd~agent-default-communication-grpc~1]
    #[test]
    fn utest_default_communications_client_forwards_agent_config() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let mut agent_config = AgentConfig::default();
        agent_config.name = TEST_AGENT_NAME.to_string();
        agent_config.server_url = ServerUrl::Tcp(TEST_SERVER_URL_TCP.to_string());
        agent_config.insecure = true;
        agent_config
            .tags
            .insert(TEST_TAG_KEY.to_string(), TEST_TAG_VALUE.to_string());

        let expected_name = agent_config.name.clone();
        let expected_server_url = agent_config.server_url.clone();
        let expected_tags = agent_config.tags.clone();

        let ctx = GRPCCommunicationsClient::new_agent_communication_context();
        ctx.expect()
            .withf(move |name, server_url, tags, tls_config| {
                name == &expected_name
                    && server_url == &expected_server_url
                    && tags == &expected_tags
                    && tls_config.is_none()
            })
            .once()
            .return_once(|_, _, _, _| Ok(GRPCCommunicationsClient::default()));

        let _client = AgentBuilder::default_communications_client(&agent_config);
    }

    // [utest->swdd~agent-default-communication-grpc~1]
    #[test]
    fn utest_default_communications_client_builds_no_tls_config_for_unix_socket() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let mut agent_config = AgentConfig::default();
        agent_config.name = TEST_AGENT_NAME.to_string();
        agent_config.server_url = ServerUrl::Unix(PathBuf::from(TEST_SERVER_URL_UNIX));

        let expected_name = agent_config.name.clone();
        let expected_server_url = agent_config.server_url.clone();

        let ctx = GRPCCommunicationsClient::new_agent_communication_context();
        ctx.expect()
            .withf(move |name, server_url, _tags, tls_config| {
                name == &expected_name && server_url == &expected_server_url && tls_config.is_none()
            })
            .once()
            .return_once(|_, _, _, _| Ok(GRPCCommunicationsClient::default()));

        let _client = AgentBuilder::default_communications_client(&agent_config);
    }

    // [utest->swdd~agent-default-communication-grpc~1]
    #[test]
    fn utest_default_communications_client_forwards_tls_config() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let mut agent_config = AgentConfig::default();
        agent_config.name = TEST_AGENT_NAME.to_string();
        agent_config.server_url = ServerUrl::Tcp(TEST_SERVER_URL_TCP.to_string());
        agent_config.ca_pem_content = Some(TEST_CA_PEM.to_string());
        agent_config.crt_pem_content = Some(TEST_CRT_PEM.to_string());
        agent_config.key_pem_content = Some(TEST_KEY_PEM.to_string());

        let ctx = GRPCCommunicationsClient::new_agent_communication_context();
        ctx.expect()
            .withf(|_, _, _, tls_config| {
                tls_config.as_ref().is_some_and(|tls| {
                    tls.ca_pem == TEST_CA_PEM
                        && tls.crt_pem == TEST_CRT_PEM
                        && tls.key_pem == TEST_KEY_PEM
                })
            })
            .once()
            .return_once(|_, _, _, _| Ok(GRPCCommunicationsClient::default()));

        let _client = AgentBuilder::default_communications_client(&agent_config);
    }

    #[test]
    fn utest_with_runtime_registers_runtime() {
        let builder =
            AgentBuilder::new(Box::new(FakeCommunicationsClient) as Box<dyn CommunicationsClient>)
                .with_runtime(Box::new(MockRuntimeConnector::default()) as Box<dyn OwnableRuntime>);

        let selected = builder
            .available_runtimes
            .select(&["mock-runtime".to_string()]);
        assert_eq!(selected.len(), 1);
    }

    // [utest->swdd~agent-uses-async-channels~1]
    // [utest->swdd~agent-prepares-dedicated-run-folder~3]
    #[test]
    fn utest_build_passes_agent_name_and_run_directory_to_runtime_manager() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let mut agent_config = AgentConfig::default();
        agent_config.name = TEST_AGENT_NAME.to_string();
        let run_folder = agent_config.run_folder.clone();

        let exists_context = crate::io_utils::mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(mockall::predicate::eq(PathBuf::from(&run_folder)))
            .return_const(true);

        let is_secure_context = crate::io_utils::mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(mockall::predicate::eq(PathBuf::from(&run_folder)))
            .return_const(true);

        let expected_run_directory =
            PathBuf::from(&run_folder).join(format!("{}_io", agent_config.name));
        let _directory_context = crate::io_utils::generate_test_directory_mock(
            run_folder.as_str(),
            &format!("{}_io", agent_config.name),
        );

        let expected_agent_name = AgentName::from(agent_config.name.as_str());
        let runtime_manager_new_context = crate::runtime_manager::MockRuntimeManager::new_context();
        runtime_manager_new_context
            .expect()
            .withf(
                move |agent_name,
                      run_directory,
                      _control_interface_tx,
                      _runtime_map,
                      _update_state_tx| {
                    agent_name == &expected_agent_name && run_directory == &expected_run_directory
                },
            )
            .once()
            .return_once(|_, _, _, _, _| crate::runtime_manager::MockRuntimeManager::default());

        let resource_monitor_new_context =
            crate::resource_monitor::MockResourceMonitor::new_context();
        resource_monitor_new_context
            .expect()
            .return_once(crate::resource_monitor::MockResourceMonitor::default);

        let builder =
            AgentBuilder::new(Box::new(FakeCommunicationsClient) as Box<dyn CommunicationsClient>);

        // build() never drives communications_client or agent_manager, so no tokio runtime is needed here.
        let _running_agent = builder.build(&agent_config);
    }

    // [utest->swdd~agent-prepares-dedicated-run-folder~3]
    #[test]
    fn utest_build_keeps_run_directory_alive_until_running_agent_is_dropped() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let mut agent_config = AgentConfig::default();
        agent_config.name = TEST_AGENT_NAME.to_string();
        let run_folder = agent_config.run_folder.clone();

        let exists_context = crate::io_utils::mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(mockall::predicate::eq(PathBuf::from(&run_folder)))
            .return_const(true);

        let is_secure_context = crate::io_utils::mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(mockall::predicate::eq(PathBuf::from(&run_folder)))
            .return_const(true);

        let expected_path = PathBuf::from(&run_folder).join(format!("{}_io", agent_config.name));
        let drop_count = Arc::new(AtomicUsize::new(0));
        let drop_count_for_mock = Arc::clone(&drop_count);

        let directory_new_context = crate::io_utils::MockDirectory::new_context();
        directory_new_context
            .expect()
            .with(mockall::predicate::eq(expected_path.clone()))
            .return_once(move |_| {
                let mut mock = crate::io_utils::MockDirectory::default();
                mock.expect_get_path().return_const(expected_path);
                mock.expect_drop().returning(move || {
                    drop_count_for_mock.fetch_add(1, Ordering::SeqCst);
                });
                Ok(mock)
            });

        let runtime_manager_new_context = crate::runtime_manager::MockRuntimeManager::new_context();
        runtime_manager_new_context
            .expect()
            .return_once(|_, _, _, _, _| crate::runtime_manager::MockRuntimeManager::default());

        let resource_monitor_new_context =
            crate::resource_monitor::MockResourceMonitor::new_context();
        resource_monitor_new_context
            .expect()
            .return_once(crate::resource_monitor::MockResourceMonitor::default);

        let builder =
            AgentBuilder::new(Box::new(FakeCommunicationsClient) as Box<dyn CommunicationsClient>);

        let running_agent = builder.build(&agent_config);
        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            0,
            "run directory must not be deleted while the running agent is still alive"
        );

        drop(running_agent);
        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            1,
            "run directory must be cleaned up once the running agent is dropped"
        );
    }

    // [utest->swdd~agent-allows-enabled-runtimes~1]
    // [utest->swdd~agent-supports-multiple-runtime-connectors~1]
    #[test]
    fn utest_build_only_includes_selected_runtimes_in_runtime_facade_map() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC.get_lock();

        let mut agent_config = AgentConfig::default();
        agent_config.name = TEST_AGENT_NAME.to_string();
        agent_config.runtimes = Some(vec!["mock-runtime".to_string()]);
        let run_folder = agent_config.run_folder.clone();

        let exists_context = crate::io_utils::mock_filesystem::exists_context();
        exists_context
            .expect()
            .with(mockall::predicate::eq(PathBuf::from(&run_folder)))
            .return_const(true);

        let is_secure_context = crate::io_utils::mock_filesystem::is_owner_exclusive_context();
        is_secure_context
            .expect()
            .with(mockall::predicate::eq(PathBuf::from(&run_folder)))
            .return_const(true);

        let _directory_context = crate::io_utils::generate_test_directory_mock(
            run_folder.as_str(),
            &format!("{}_io", agent_config.name),
        );

        let runtime_facade_new_context =
            crate::runtime_connectors::MockRuntimeFacade::new_context();
        runtime_facade_new_context
            .expect()
            .returning(|_, _| crate::runtime_connectors::MockRuntimeFacade::default());

        let runtime_manager_new_context = crate::runtime_manager::MockRuntimeManager::new_context();
        runtime_manager_new_context
            .expect()
            .withf(|_, _, _, runtime_map, _| {
                runtime_map.len() == 1 && runtime_map.contains_key("mock-runtime")
            })
            .once()
            .return_once(|_, _, _, _, _| crate::runtime_manager::MockRuntimeManager::default());

        let resource_monitor_new_context =
            crate::resource_monitor::MockResourceMonitor::new_context();
        resource_monitor_new_context
            .expect()
            .return_once(crate::resource_monitor::MockResourceMonitor::default);

        let builder =
            AgentBuilder::new(Box::new(FakeCommunicationsClient) as Box<dyn CommunicationsClient>)
                .with_runtime(Box::new(MockRuntimeConnector::default()) as Box<dyn OwnableRuntime>)
                .with_runtime(
                    Box::new(FakeRuntimeConnector::new("other-runtime")) as Box<dyn OwnableRuntime>
                );

        // build() never drives communications_client or agent_manager, so no tokio runtime is needed here.
        let _running_agent = builder.build(&agent_config);
    }

    // [utest->swdd~agent-allows-enabled-runtimes~1]
    #[test]
    fn utest_select_runtimes_selects_subset_of_configured_runtimes() {
        let mut available_runtimes = RuntimeRegistry::default();
        available_runtimes
            .register(Box::new(MockRuntimeConnector::default()) as Box<dyn OwnableRuntime>);
        available_runtimes.register(
            Box::new(FakeRuntimeConnector::new("other-runtime")) as Box<dyn OwnableRuntime>
        );

        let selected = AgentBuilder::select_runtimes(
            available_runtimes,
            &Some(vec!["mock-runtime".to_string()]),
        );

        assert_eq!(selected.len(), 1);
        assert!(selected.contains_key("mock-runtime"));
    }

    // [utest->swdd~agent-allows-enabled-runtimes~1]
    #[test]
    fn utest_select_runtimes_ignores_unknown_configured_runtime() {
        let mut available_runtimes = RuntimeRegistry::default();
        available_runtimes
            .register(Box::new(MockRuntimeConnector::default()) as Box<dyn OwnableRuntime>);

        let selected = AgentBuilder::select_runtimes(
            available_runtimes,
            &Some(vec![
                "mock-runtime".to_string(),
                "unknown-runtime".to_string(),
            ]),
        );

        assert_eq!(selected.len(), 1);
        assert!(selected.contains_key("mock-runtime"));
    }

    #[derive(Debug, Default)]
    struct FakeCommunicationsClient;

    #[async_trait::async_trait]
    impl CommunicationsClient for FakeCommunicationsClient {
        async fn run(
            &mut self,
            _receiver: ToServerReceiver,
            _manager_interface: FromServerSender,
        ) -> Result<(), CommunicationMiddlewareError> {
            Ok(())
        }
    }

    // Only `name()` is exercised in these tests; the other methods are never invoked.
    #[derive(Debug, Clone)]
    struct FakeRuntimeConnector {
        name: String,
    }

    impl FakeRuntimeConnector {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::runtime_connectors::RuntimeConnector for FakeRuntimeConnector {
        fn name(&self) -> String {
            self.name.clone()
        }

        async fn get_reusable_workloads(
            &self,
            _agent_name: &common::objects::AgentName,
        ) -> Result<
            Vec<crate::runtime_connectors::ReusableWorkloadState>,
            crate::runtime_connectors::RuntimeError,
        > {
            unimplemented!("not used in this test")
        }

        async fn create_workload(
            &self,
            _runtime_workload_config: ankaios_api::ank_base::WorkloadNamed,
            _reusable_workload_id: Option<crate::runtime_connectors::RuntimeWorkloadId>,
            _control_interface_path: Option<PathBuf>,
            _update_state_tx: crate::workload_state::WorkloadStateSender,
            _host_workload_file_path_mappings: std::collections::HashMap<PathBuf, PathBuf>,
        ) -> Result<
            (
                crate::runtime_connectors::RuntimeWorkloadId,
                crate::runtime_connectors::StateCheckerHandle,
            ),
            crate::runtime_connectors::RuntimeError,
        > {
            unimplemented!("not used in this test")
        }

        async fn get_workload_id(
            &self,
            _instance_name: &ankaios_api::ank_base::WorkloadInstanceNameSpec,
        ) -> Result<
            crate::runtime_connectors::RuntimeWorkloadId,
            crate::runtime_connectors::RuntimeError,
        > {
            unimplemented!("not used in this test")
        }

        async fn start_checker(
            &self,
            _workload_id: &crate::runtime_connectors::RuntimeWorkloadId,
            _runtime_workload_config: ankaios_api::ank_base::WorkloadNamed,
            _update_state_tx: crate::workload_state::WorkloadStateSender,
        ) -> Result<
            crate::runtime_connectors::StateCheckerHandle,
            crate::runtime_connectors::RuntimeError,
        > {
            unimplemented!("not used in this test")
        }

        fn get_log_fetcher(
            &self,
            _workload_id: crate::runtime_connectors::RuntimeWorkloadId,
            _options: &crate::runtime_connectors::LogRequestOptions,
        ) -> Result<
            Box<dyn crate::runtime_connectors::log_fetcher::LogFetcher + Send>,
            crate::runtime_connectors::RuntimeError,
        > {
            unimplemented!("not used in this test")
        }

        async fn delete_workload(
            &self,
            _workload_id: &crate::runtime_connectors::RuntimeWorkloadId,
        ) -> Result<(), crate::runtime_connectors::RuntimeError> {
            unimplemented!("not used in this test")
        }
    }
}

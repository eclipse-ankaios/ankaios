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

use super::systemd_runtime_config::{ServiceState, SystemdRuntimeConfig};
#[cfg_attr(test, double)]
use crate::runtime_connectors::systemd::systemd_cli::SystemdCli;
use crate::{
    generic_polling_state_checker::GenericPollingStateChecker,
    runtime_connectors::{
        generic_log_fetcher::GenericLogFetcher, log_fetcher::LogFetcher,
        runtime_connector::LogRequestOptions, ReusableWorkloadState, RuntimeConnector,
        RuntimeError, RuntimeStateGetter, RuntimeWorkloadId, StateCheckerHandle,
    },
    workload_state::WorkloadStateSender,
};

use ankaios_api::ank_base::{ExecutionStateSpec, WorkloadInstanceNameSpec, WorkloadNamed};
use common::objects::AgentName;

use async_trait::async_trait;
#[cfg(test)]
use mockall_double::double;
use std::{collections::HashMap, path::PathBuf};

pub const SYSTEMD_RUNTIME_NAME: &str = "systemd";

// Guards against duplicate restart when the agent itself is restarted by systemd
const RECENT_RESTART_THRESHOLD_SECS: u64 = 3;

#[derive(Debug, Clone)]
pub struct SystemdRuntime {}

#[derive(Debug, Clone)]
pub struct SystemdStateGetter {}

#[async_trait]
impl RuntimeStateGetter for SystemdStateGetter {
    async fn get_state(&self, workload_id: &RuntimeWorkloadId) -> ExecutionStateSpec {
        let unit_name = workload_id.as_ref();
        log::trace!("Getting the state for systemd unit '{}'", unit_name);

        let exec_state = match SystemdCli::get_unit_state(unit_name).await {
            Ok(state) => map_systemd_state_to_execution_state(state),
            Err(err) => {
                log::warn!(
                    "Could not get state of unit '{}': '{}'. Returning unknown.",
                    unit_name,
                    err
                );
                ExecutionStateSpec::unknown("Error getting state from systemd.")
            }
        };

        log::trace!(
            "Returning the state '{}' for systemd unit '{}'",
            exec_state,
            unit_name
        );
        exec_state
    }
}

/// Map systemd unit state to Ankaios execution state
fn map_systemd_state_to_execution_state(
    state: crate::runtime_connectors::systemd::systemd_cli::SystemdUnitState,
) -> ExecutionStateSpec {
    match state.active_state.as_str() {
        "active" if state.sub_state == "running" => ExecutionStateSpec::running(),
        "inactive" if state.sub_state == "dead" => {
            // Check exit code to determine success or failure
            match state.exit_code {
                Some(0) => ExecutionStateSpec::succeeded(),
                Some(code) => ExecutionStateSpec::failed(format!("Exit code: {}", code)),
                None => ExecutionStateSpec::succeeded(), // Treat no exit code as success for inactive services
            }
        }
        "failed" => ExecutionStateSpec::failed(format!("Service failed: {}", state.sub_state)),
        "activating" => ExecutionStateSpec::starting(state.sub_state),
        "deactivating" => ExecutionStateSpec::stopping(state.sub_state),
        _ => ExecutionStateSpec::unknown(state.active_state),
    }
}

#[async_trait]
impl RuntimeConnector for SystemdRuntime {
    fn name(&self) -> String {
        SYSTEMD_RUNTIME_NAME.to_string()
    }

    async fn get_reusable_workloads(
        &self,
        _agent_name: &AgentName,
    ) -> Result<Vec<ReusableWorkloadState>, RuntimeError> {
        Ok(Vec::new())
    }

    async fn create_workload(
        &self,
        workload_named: WorkloadNamed,
        _reusable_workload_id: Option<RuntimeWorkloadId>,
        _control_interface_path: Option<PathBuf>,
        update_state_tx: WorkloadStateSender,
        _workload_file_path_mappings: HashMap<PathBuf, PathBuf>,
    ) -> Result<(RuntimeWorkloadId, StateCheckerHandle), RuntimeError> {
        let config = SystemdRuntimeConfig::try_from(&workload_named.workload)
            .map_err(RuntimeError::Unsupported)?;

        log::debug!(
            "Processing systemd workload '{}' for service '{}'",
            workload_named.instance_name,
            config.service_name
        );

        let result = match config.desired_state {
            ServiceState::Running => {
                log::debug!("Starting systemd unit: {}", config.service_name);
                SystemdCli::start_unit(&config.service_name).await
            }
            ServiceState::Stopped => {
                log::debug!("Stopping systemd unit: {}", config.service_name);
                SystemdCli::stop_unit(&config.service_name).await
            }
            ServiceState::Restarted => {
                match SystemdCli::get_unit_uptime_seconds(&config.service_name).await {
                    Ok(Some(uptime_secs)) if uptime_secs <= RECENT_RESTART_THRESHOLD_SECS => {
                        log::info!(
                            "Service '{}' was restarted {}s ago, skipping restart to avoid duplicate action",
                            config.service_name,
                            uptime_secs
                        );
                        Ok(())
                    }
                    _ => {
                        log::debug!("Restarting systemd unit: {}", config.service_name);
                        SystemdCli::restart_unit(&config.service_name).await
                    }
                }
            }
        };

        match result {
            Ok(()) => {
                let workload_id = RuntimeWorkloadId::from(config.service_name);

                log::debug!(
                    "The systemd workload '{}' has been reconciled to desired state",
                    workload_named.instance_name
                );

                let state_checker = self
                    .start_checker(&workload_id, workload_named, update_state_tx)
                    .await?;

                Ok((workload_id, state_checker))
            }
            Err(err) => {
                log::debug!("Systemd operation failed. Error: '{}'", err);
                Err(RuntimeError::Create(err))
            }
        }
    }

    async fn get_workload_id(
        &self,
        _instance_name: &WorkloadInstanceNameSpec,
    ) -> Result<RuntimeWorkloadId, RuntimeError> {
        Err(RuntimeError::List(
            "Systemd runtime does not support reusability".to_string(),
        ))
    }

    async fn start_checker(
        &self,
        workload_id: &RuntimeWorkloadId,
        workload_named: WorkloadNamed,
        update_state_tx: WorkloadStateSender,
    ) -> Result<StateCheckerHandle, RuntimeError> {
        log::debug!(
            "Starting the checker for systemd unit '{}'",
            workload_id
        );
        let checker = GenericPollingStateChecker::start_checker(
            &workload_named,
            workload_id.clone(),
            update_state_tx,
            SystemdStateGetter {},
        );
        Ok(Box::new(checker))
    }

    fn get_log_fetcher(
        &self,
        workload_id: RuntimeWorkloadId,
        options: &LogRequestOptions,
    ) -> Result<Box<dyn LogFetcher + Send>, RuntimeError> {
        let systemd_log_fetcher =
            super::systemd_log_fetcher::SystemdLogFetcher::new(&workload_id, options)?;
        let log_fetcher = GenericLogFetcher::new(systemd_log_fetcher);
        Ok(Box::new(log_fetcher))
    }

    async fn delete_workload(&self, workload_id: &RuntimeWorkloadId) -> Result<(), RuntimeError> {
        let unit_name = workload_id.as_ref();
        log::debug!("Stopping systemd unit '{}'", unit_name);
        SystemdCli::stop_unit(unit_name)
            .await
            .map_err(|err| RuntimeError::Delete(err.to_string()))
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
        map_systemd_state_to_execution_state, SystemdCli, SystemdRuntime, SystemdStateGetter,
        SYSTEMD_RUNTIME_NAME,
    };
    use crate::runtime_connectors::{
        RuntimeConnector, RuntimeError, RuntimeStateGetter, RuntimeWorkloadId,
    };
    use crate::runtime_connectors::systemd::systemd_cli::SystemdUnitState;
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

    use ankaios_api::ank_base::{ExecutionStateSpec, WorkloadNamed};
    use ankaios_api::test_utils::{fixtures, generate_test_workload_named_with_params};
    use common::objects::AgentName;

    use std::collections::HashMap;

    fn generate_test_systemd_workload() -> WorkloadNamed {
        let mut workload = generate_test_workload_named_with_params(
            fixtures::WORKLOAD_NAMES[0],
            fixtures::AGENT_NAMES[0],
            SYSTEMD_RUNTIME_NAME,
        );
        workload.workload.runtime_config = "serviceName: test.service\n".to_string();
        workload
    }

    #[test]
    fn utest_name_systemd() {
        let systemd_runtime = SystemdRuntime {};
        assert_eq!(systemd_runtime.name(), "systemd".to_string());
    }

    #[tokio::test]
    async fn utest_get_reusable_workloads_always_empty() {
        let systemd_runtime = SystemdRuntime {};
        let agent_name = AgentName::from(fixtures::AGENT_NAMES[0]);
        let result = systemd_runtime.get_reusable_workloads(&agent_name).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn utest_state_getter_returns_running() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let get_unit_state_context = SystemdCli::get_unit_state_context();
        get_unit_state_context.expect().return_const(Ok(SystemdUnitState {
            active_state: "active".to_string(),
            sub_state: "running".to_string(),
            exit_code: Some(0),
        }));

        let state_getter = SystemdStateGetter {};
        let workload_id = RuntimeWorkloadId::from("test.service");
        let state = state_getter.get_state(&workload_id).await;

        assert_eq!(state, ExecutionStateSpec::running());
    }

    #[tokio::test]
    async fn utest_state_getter_returns_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let get_unit_state_context = SystemdCli::get_unit_state_context();
        get_unit_state_context.expect().return_const(Ok(SystemdUnitState {
            active_state: "failed".to_string(),
            sub_state: "failed".to_string(),
            exit_code: Some(1),
        }));

        let state_getter = SystemdStateGetter {};
        let workload_id = RuntimeWorkloadId::from("test.service");
        let state = state_getter.get_state(&workload_id).await;

        assert_eq!(state, ExecutionStateSpec::failed("Service failed: failed"));
    }

    #[tokio::test]
    async fn utest_create_workload_starts_service() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let start_unit_context = SystemdCli::start_unit_context();
        start_unit_context.expect().return_const(Ok(()));

        let runtime = SystemdRuntime {};
        let workload = generate_test_systemd_workload();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);

        let result = runtime
            .create_workload(workload, None, None, sender, HashMap::new())
            .await;

        assert!(result.is_ok());
        let (workload_id, _checker) = result.unwrap();
        assert_eq!(workload_id.to_string(), "test.service");
    }

    #[tokio::test]
    async fn utest_create_workload_stops_service() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let stop_unit_context = SystemdCli::stop_unit_context();
        stop_unit_context.expect().return_const(Ok(()));

        let runtime = SystemdRuntime {};
        let mut workload = generate_test_systemd_workload();
        workload.workload.runtime_config =
            "serviceName: test.service\ndesiredState: stopped\n".to_string();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);

        let result = runtime
            .create_workload(workload, None, None, sender, HashMap::new())
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn utest_create_workload_restarts_service() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        // Mock uptime check to return old uptime (service not recently restarted)
        let get_uptime_context = SystemdCli::get_unit_uptime_seconds_context();
        get_uptime_context.expect().return_const(Ok(Some(100)));  // 100 seconds uptime

        let restart_unit_context = SystemdCli::restart_unit_context();
        restart_unit_context.expect().return_const(Ok(()));

        let runtime = SystemdRuntime {};
        let mut workload = generate_test_systemd_workload();
        workload.workload.runtime_config =
            "serviceName: test.service\ndesiredState: restarted\n".to_string();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);

        let result = runtime
            .create_workload(workload, None, None, sender, HashMap::new())
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn utest_create_workload_skips_recent_restart() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        // Mock uptime check to return recent restart (2 seconds ago)
        let get_uptime_context = SystemdCli::get_unit_uptime_seconds_context();
        get_uptime_context.expect().return_const(Ok(Some(2)));  // Recently restarted

        // restart_unit should NOT be called
        let restart_unit_context = SystemdCli::restart_unit_context();
        restart_unit_context.expect().times(0);

        let runtime = SystemdRuntime {};
        let mut workload = generate_test_systemd_workload();
        workload.workload.runtime_config =
            "serviceName: test.service\ndesiredState: restarted\n".to_string();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);

        let result = runtime
            .create_workload(workload, None, None, sender, HashMap::new())
            .await;

        // Should still succeed even though restart was skipped
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn utest_delete_workload_stops_service() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let stop_unit_context = SystemdCli::stop_unit_context();
        stop_unit_context.expect().return_const(Ok(()));

        let runtime = SystemdRuntime {};
        let workload_id = RuntimeWorkloadId::from("test.service");

        let result = runtime.delete_workload(&workload_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn utest_create_workload_start_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let start_unit_context = SystemdCli::start_unit_context();
        start_unit_context
            .expect()
            .return_const(Err("unit not found".to_string()));

        let runtime = SystemdRuntime {};
        let workload = generate_test_systemd_workload();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);

        let result = runtime
            .create_workload(workload, None, None, sender, HashMap::new())
            .await;

        assert!(matches!(result, Err(RuntimeError::Create(_))));
    }

    #[tokio::test]
    async fn utest_delete_workload_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let stop_unit_context = SystemdCli::stop_unit_context();
        stop_unit_context
            .expect()
            .return_const(Err("permission denied".to_string()));

        let runtime = SystemdRuntime {};
        let workload_id = RuntimeWorkloadId::from("test.service");

        let result = runtime.delete_workload(&workload_id).await;
        assert!(matches!(result, Err(RuntimeError::Delete(_))));
    }

    #[tokio::test]
    async fn utest_state_getter_returns_unknown_on_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let get_unit_state_context = SystemdCli::get_unit_state_context();
        get_unit_state_context
            .expect()
            .return_const(Err("dbus error".to_string()));

        let state_getter = SystemdStateGetter {};
        let workload_id = RuntimeWorkloadId::from("test.service");
        let state = state_getter.get_state(&workload_id).await;

        assert_eq!(state, ExecutionStateSpec::unknown("Error getting state from systemd."));
    }

    #[test]
    fn utest_map_state_activating() {
        let state = SystemdUnitState {
            active_state: "activating".to_string(),
            sub_state: "start".to_string(),
            exit_code: None,
        };
        assert_eq!(
            map_systemd_state_to_execution_state(state),
            ExecutionStateSpec::starting("start")
        );
    }

    #[test]
    fn utest_map_state_deactivating() {
        let state = SystemdUnitState {
            active_state: "deactivating".to_string(),
            sub_state: "stop-sigterm".to_string(),
            exit_code: None,
        };
        assert_eq!(
            map_systemd_state_to_execution_state(state),
            ExecutionStateSpec::stopping("stop-sigterm")
        );
    }

    #[test]
    fn utest_map_state_inactive_success() {
        let state = SystemdUnitState {
            active_state: "inactive".to_string(),
            sub_state: "dead".to_string(),
            exit_code: Some(0),
        };
        assert_eq!(
            map_systemd_state_to_execution_state(state),
            ExecutionStateSpec::succeeded()
        );
    }

    #[test]
    fn utest_map_state_inactive_failed() {
        let state = SystemdUnitState {
            active_state: "inactive".to_string(),
            sub_state: "dead".to_string(),
            exit_code: Some(1),
        };
        assert_eq!(
            map_systemd_state_to_execution_state(state),
            ExecutionStateSpec::failed("Exit code: 1")
        );
    }

    #[test]
    fn utest_map_state_unknown() {
        let state = SystemdUnitState {
            active_state: "reloading".to_string(),
            sub_state: "reload".to_string(),
            exit_code: None,
        };
        assert_eq!(
            map_systemd_state_to_execution_state(state),
            ExecutionStateSpec::unknown("reloading")
        );
    }
}

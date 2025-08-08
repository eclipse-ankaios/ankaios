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

use common::objects::ExecutionState;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::HashMap,
    ops::Deref,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_connectors::cli_command::CliCommand;

const NERDCTL_CMD: &str = "nerdctl";
const API_PIPES_MOUNT_POINT: &str = "/run/ankaios/control_interface";
const NERDCTL_PS_CACHE_MAX_AGE: Duration = Duration::from_millis(1000);

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ContainerState {
    Starting,
    Exited(u8),
    Paused,
    Running,
    Unknown,
    Stopping,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NerdctlRunConfig {
    pub general_options: Vec<String>,
    pub command_options: Vec<String>,
    pub image: String,
    pub command_args: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NerdctlStartConfig {
    pub general_options: Vec<String>,
    pub container_id: String,
}

impl From<NerdctlContainerInfo> for ContainerState {
    fn from(value: NerdctlContainerInfo) -> Self {
        match value.state.to_lowercase().as_str() {
            "created" => ContainerState::Starting,
            "configured" => ContainerState::Starting,
            "initialized" => ContainerState::Starting,
            "exited" => ContainerState::Exited(value.exit_code),
            "paused" => ContainerState::Paused,
            "running" => ContainerState::Running,
            "stopping" => ContainerState::Stopping,
            "stopped" => ContainerState::Stopping,
            "removing" => ContainerState::Stopping,
            "unknown" => ContainerState::Unknown,
            state => {
                log::trace!(
                    "Mapping the container state '{state}' to the container state 'Unknown'"
                );
                ContainerState::Unknown
            }
        }
    }
}

// [impl->swdd~nerdctl-state-getter-maps-state~3]
impl From<NerdctlContainerInfo> for ExecutionState {
    fn from(value: NerdctlContainerInfo) -> Self {
        match value.state.to_lowercase().as_str() {
            "created" => ExecutionState::starting(value.state),
            "configured" => ExecutionState::starting(value.state),
            "initialized" => ExecutionState::starting(value.state),
            "exited" if value.exit_code == 0 => ExecutionState::succeeded(),
            "exited" if value.exit_code != 0 => {
                ExecutionState::failed(format!("Exit code: '{}'", value.exit_code))
            }
            "running" => ExecutionState::running(),
            "stopping" => ExecutionState::stopping(value.state),
            "stopped" => ExecutionState::stopping(value.state),
            "removing" => ExecutionState::stopping(value.state),
            "unknown" => ExecutionState::unknown(value.state),
            state => {
                log::trace!(
                    "Mapping the container state '{state}' to the execution state 'ExecUnknown'"
                );
                ExecutionState::unknown(state)
            }
        }
    }
}

struct NerdctlPsCache {
    last_update: Instant,
    cache: Arc<NerdctlPsResult>,
}

struct TimedNerdctlPsResult(Mutex<Option<NerdctlPsCache>>);

impl TimedNerdctlPsResult {
    async fn reset(&self) {
        *self.lock().await = None;
    }

    // [impl->swdd~nerdctlcli-container-state-cache-refresh~1]
    async fn get(&self) -> Arc<NerdctlPsResult> {
        let mut guard = self.lock().await;

        if let Some(value) = &mut *guard {
            if value.last_update.elapsed() > NERDCTL_PS_CACHE_MAX_AGE {
                *value = Self::new_inner().await;
            }
            value.cache.clone()
        } else {
            let ps_result = Self::new_inner().await;
            let result = ps_result.cache.clone();
            *guard = Some(ps_result);
            result
        }
    }

    async fn new_inner() -> NerdctlPsCache {
        let mut res = NerdctlCli::list_states_internal().await;

        // TODO: remove this workaround when the nerdctl does not have this issue.
        if res.is_err() {
            // This is a workaround for the known issue in nerdctl (nerdctl ps sometimes fails).
            log::trace!("'nerdctl ps' has returned error - let's retry it.");
            res = NerdctlCli::list_states_internal().await;
        }
        NerdctlPsCache {
            last_update: Instant::now(),
            cache: Arc::new(res.into()),
        }
    }
}

impl Deref for TimedNerdctlPsResult {
    type Target = Mutex<Option<NerdctlPsCache>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// [impl->swdd~nerdctlcli-container-state-cache-all-containers~1]
#[derive(Debug)]
struct NerdctlPsResult {
    container_states: Result<HashMap<String, ExecutionState>, String>,
}

impl From<Result<Vec<NerdctlContainerInfo>, String>> for NerdctlPsResult {
    fn from(value: Result<Vec<NerdctlContainerInfo>, String>) -> Self {
        match value {
            Ok(container_infos) => {
                let mut container_states = HashMap::new();
                let mut pod_states: HashMap<String, Vec<ContainerState>> = HashMap::new();

                for container_entry in container_infos {
                    container_states
                        .insert(container_entry.id.clone(), container_entry.clone().into());
                    pod_states
                        .entry(container_entry.pod.clone())
                        .or_default()
                        .push(container_entry.into());
                }
                Self {
                    container_states: Ok(container_states),
                }
            }
            Err(err) => Self {
                container_states: Err(err.clone()),
            },
        }
    }
}

static LAST_PS_RESULT: TimedNerdctlPsResult = TimedNerdctlPsResult(Mutex::const_new(Option::None));

pub struct NerdctlCli {}

#[cfg_attr(test, automock)]
impl NerdctlCli {
    pub async fn reset_ps_cache() {
        LAST_PS_RESULT.reset().await;
    }

    pub async fn list_workload_ids_by_label(key: &str, value: &str) -> Result<Vec<String>, String> {
        log::debug!("Listing workload ids for: {key}='{value}'",);
        let output = CliCommand::new(NERDCTL_CMD)
            .args(&[
                "ps",
                "--all",
                "--filter",
                &format!("label={key}={value}"),
                "--format=json",
            ])
            .exec()
            .await?;

        let res: Vec<NerdctlContainerInfo> = serde_json::from_str(&output)
            .map_err(|err| format!("Could not parse nerdctl output: '{err}'"))?;

        Ok(res.into_iter().map(|x| x.id).collect())
    }

    pub async fn list_workload_names_by_label(
        key: &str,
        value: &str,
    ) -> Result<Vec<String>, String> {
        log::trace!("Listing workload names for: '{key}'='{value}'",);
        let output = CliCommand::new(NERDCTL_CMD)
            .args(&[
                "ps",
                "--all",
                "--filter",
                &format!("label={key}={value}"),
                "--format=json",
            ])
            .exec()
            .await?;

        let res: Vec<NerdctlContainerInfo> = serde_json::from_str(&output)
            .map_err(|err| format!("Could not parse nerdctl output: '{err}'"))?;

        let mut names: Vec<String> = Vec::new();
        for mut nerdctl_info in res {
            if let Some(name_val) = nerdctl_info.labels.get_mut("name") {
                names.push(name_val.to_string());
            }
        }
        Ok(names)
    }

    pub async fn nerdctl_run(
        mut run_config: NerdctlRunConfig,
        workload_name: &str,
        agent: &str,
        control_interface_path: Option<PathBuf>,
        workload_file_path_mappings: HashMap<PathBuf, PathBuf>,
    ) -> Result<String, String> {
        log::debug!(
            "Creating the workload '{}' with image '{}'",
            workload_name,
            run_config.image
        );

        let mut args = run_config.general_options;

        args.push("run".into());
        args.push("--detach".into());

        // Setting "--name" flag is intentionally here before reading "command_options".
        // We want to give the user chance to set own container name.
        // In other words the user can overwrite our container name.
        // We store workload name as a label (and use them from there).
        // Therefore we do insist on container names in particular format.
        //
        // [impl->swdd~nerdctl-create-workload-sets-optionally-container-name~2]
        args.append(&mut vec!["--name".into(), workload_name.to_string()]);

        args.append(&mut run_config.command_options);

        // [impl->swdd~nerdctl-create-workload-mounts-fifo-files~1]
        if let Some(path) = control_interface_path {
            args.push(
                [
                    "--mount=type=bind,source=",
                    &path.to_string_lossy(),
                    ",destination=",
                    API_PIPES_MOUNT_POINT,
                ]
                .concat(),
            );
        }

        // [impl->swdd~nerdctl-create-mounts-workload-files~1]
        for (host_file_path, mount_point) in workload_file_path_mappings {
            args.push(
                [
                    "--mount=type=bind,source=",
                    &host_file_path.to_string_lossy(),
                    ",destination=",
                    &mount_point.to_string_lossy(),
                    ",readonly=true",
                ]
                .concat(),
            );
        }

        // [impl->swdd~nerdctl-create-workload-creates-labels~2]
        args.push(format!("--label=name={workload_name}"));
        args.push(format!("--label=agent={agent}"));
        args.push(run_config.image);

        args.append(&mut run_config.command_args);

        log::debug!("The args are: '{args:?}'");
        let id = CliCommand::new(NERDCTL_CMD)
            .args(&args.iter().map(|x| &**x).collect::<Vec<&str>>())
            .exec()
            .await?
            .trim()
            .to_string();
        Ok(id)
    }

    pub async fn nerdctl_start(
        start_config: NerdctlStartConfig,
        workload_name: &str,
    ) -> Result<String, String> {
        log::debug!(
            "Starting the workload '{}' with id '{}'",
            workload_name,
            start_config.container_id
        );

        let mut args = start_config.general_options;

        args.push("start".into());

        args.push(start_config.container_id);

        let id = CliCommand::new(NERDCTL_CMD)
            .args(&args.iter().map(|x| &**x).collect::<Vec<&str>>())
            .exec()
            .await?
            .trim()
            .to_string();
        Ok(id)
    }

    // [impl->swdd~nerdctlcli-uses-container-state-cache~1]
    pub async fn list_states_by_id(workload_id: &str) -> Result<Option<ExecutionState>, String> {
        let ps_result = LAST_PS_RESULT.get().await;
        let all_containers_states = ps_result
            .as_ref()
            .container_states
            .as_ref()
            .map_err(|err| err.to_owned())?;
        Ok(all_containers_states
            .get(workload_id)
            .map(ToOwned::to_owned))
    }

    async fn list_states_internal() -> Result<Vec<NerdctlContainerInfo>, String> {
        let output = CliCommand::new(NERDCTL_CMD)
            .args(&["ps", "--all", "--format=json"])
            .exec()
            .await?;

        serde_json::from_str(&output).map_err(|err| format!("Could not parse nerdctl output:{err}"))
    }

    pub async fn remove_workloads_by_id(workload_id: &str) -> Result<(), String> {
        // Containers may have "--rm" flag -> it can happen, that they already do not exist.
        let args = vec!["stop", "--ignore", workload_id];
        CliCommand::new(NERDCTL_CMD).args(&args).exec().await?;
        let args = vec!["rm", "--ignore", workload_id];
        CliCommand::new(NERDCTL_CMD).args(&args).exec().await?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "PascalCase")]
struct NerdctlContainerInfo {
    state: String,
    exit_code: u8,
    #[serde(deserialize_with = "nullable_labels")]
    labels: HashMap<String, String>,
    #[serde(deserialize_with = "nullable_labels")]
    id: String,
    #[serde(deserialize_with = "nullable_labels")]
    pod: String,
}

fn nullable_labels<'a, D, V>(deserializer: D) -> Result<V, D::Error>
where
    D: Deserializer<'a>,
    V: Deserialize<'a> + Default,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
// [utest->swdd~nerdctl-uses-nerdctl-cli~1]
// [utest->swdd~nerdctl-kube-uses-nerdctl-cli~1]
#[cfg(test)]
mod tests {
    use super::{ContainerState, NerdctlCli, NerdctlPsCache};

    use super::NerdctlContainerInfo;
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;
    use common::objects::ExecutionState;
    use common::test_utils::serialize_as_map;
    use serde::Serialize;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{self, Duration};

    const SAMPLE_ERROR_MESSAGE: &str = "error message";

    #[test]
    fn utest_container_state_from_nerdctl_container_info_created() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Created".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Starting));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_configured() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Configured".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Starting));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_initialized() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Initialized".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Starting));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_exited() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Exited".to_string(),
            exit_code: 23,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Exited(23)));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_paused() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Paused".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Paused));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_running() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Running".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Running));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_stopping() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Stopping".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Stopping));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_stopped() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Stopped".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Stopping));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_removing() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Removing".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Stopping));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_undefined() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Undefined".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Unknown));
    }

    #[test]
    fn utest_container_state_from_nerdctl_container_info_unknown() {
        let container_state: ContainerState = NerdctlContainerInfo {
            state: "Unknown".to_string(),
            exit_code: 0,
            labels: Default::default(),
            pod: "".into(),
            id: "".into(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Unknown));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok([
                    TestNerdctlContainerInfo {
                        id: "result1",
                        ..Default::default()
                    },
                    TestNerdctlContainerInfo {
                        id: "result2",
                        ..Default::default()
                    },
                ]
                .to_json())),
        );

        let res = NerdctlCli::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Ok(res) if res == vec!["result1", "result2"]));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = NerdctlCli::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_broken_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok("non-json response from nerdctl".into())),
        );

        let res = NerdctlCli::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg.contains("Could not parse nerdctl output")));
    }

    #[tokio::test]
    async fn utest_list_workload_names_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    labels: &[("name", "workload_name")],
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec!["workload_name".into()]));
    }

    #[tokio::test]
    async fn utest_list_workload_names_not_found_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok([TestNerdctlContainerInfo::default()].to_json())),
        );

        let res = NerdctlCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec![]));
    }

    #[tokio::test]
    async fn utest_list_workload_names_nerdctl_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Err("simulated error".to_string())),
        );

        let res = NerdctlCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_workload_names_broken_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok("non-json response from nerdctl".to_string())),
        );

        let res = NerdctlCli::list_workload_names_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse nerdctl output") ));
    }

    // [utest->swdd~nerdctl-create-workload-creates-labels~2]
    // [utest->swdd~nerdctl-create-workload-sets-optionally-container-name~2]
    // [utest->swdd~nerdctl-create-workload-mounts-fifo-files~1]
    // [utest->swdd~nerdctl-create-mounts-workload-files~1]
    #[tokio::test]
    async fn utest_run_container_success_no_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "run",
                    "--detach",
                    "--name",
                    "test_workload_name",
                    "--label=name=test_workload_name",
                    "--label=agent=test_agent",
                    "alpine:latest",
                ])
                .exec_returns(Ok("test_id".to_string())),
        );

        let run_config = super::NerdctlRunConfig {
            general_options: Vec::new(),
            command_options: Vec::new(),
            image: "alpine:latest".into(),
            command_args: Vec::new(),
        };
        let res = NerdctlCli::nerdctl_run(
            run_config,
            "test_workload_name",
            "test_agent",
            None,
            Default::default(),
        )
        .await;
        assert_eq!(res, Ok("test_id".to_string()));
    }

    #[tokio::test]
    async fn utest_run_container_fail_no_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "run",
                    "--detach",
                    "--name",
                    "test_workload_name",
                    "--label=name=test_workload_name",
                    "--label=agent=test_agent",
                    "alpine:latest",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let run_config = super::NerdctlRunConfig {
            general_options: Vec::new(),
            command_options: Vec::new(),
            image: "alpine:latest".into(),
            command_args: Vec::new(),
        };
        let res = NerdctlCli::nerdctl_run(
            run_config,
            "test_workload_name",
            "test_agent",
            None,
            Default::default(),
        )
        .await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    // [utest->swdd~nerdctl-create-workload-sets-optionally-container-name~2]
    // [utest->swdd~nerdctl-create-workload-mounts-fifo-files~1]
    // [utest->swdd~nerdctl-create-mounts-workload-files~1]
    #[tokio::test]
    async fn utest_run_container_success_with_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        const HOST_WORKLOAD_FILE_PATH: &str = "/some/path/on/host/file/system/file.conf";
        const MOUNT_POINT_PATH: &str = "/mount/point/in/container/test.conf";

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&[
                    "--remote",
                    "run",
                    "--detach",
                    "--name",
                    "test_workload_name",
                    "--network=host",
                    "--name",
                    "myCont",
                    "--mount=type=bind,source=/test/path,destination=/run/ankaios/control_interface",
                    &format!("--mount=type=bind,source={HOST_WORKLOAD_FILE_PATH},destination={MOUNT_POINT_PATH},readonly=true"),
                    "--label=name=test_workload_name",
                    "--label=agent=test_agent",
                    "alpine:latest",
                    "sh",
                ])
                .exec_returns(Ok("test_id".to_string())),
        );

        let run_config = super::NerdctlRunConfig {
            general_options: vec!["--remote".into()],
            command_options: vec!["--network=host".into(), "--name".into(), "myCont".into()],
            image: "alpine:latest".into(),
            command_args: vec!["sh".into()],
        };
        let res = NerdctlCli::nerdctl_run(
            run_config,
            "test_workload_name",
            "test_agent",
            Some("/test/path".into()),
            HashMap::from([(HOST_WORKLOAD_FILE_PATH.into(), MOUNT_POINT_PATH.into())]),
        )
        .await;
        assert_eq!(res, Ok("test_id".to_string()));
    }

    // [utest->swdd~nerdctl-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_start_container_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        const ID: &str = "test_id";

        super::CliCommand::reset();
        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["--remote", "start", ID])
                .exec_returns(Ok(ID.to_string())),
        );

        let start_config = super::NerdctlStartConfig {
            general_options: vec!["--remote".into()],
            container_id: ID.into(),
        };
        let res = NerdctlCli::nerdctl_start(start_config, "test_workload_name").await;
        assert_eq!(res, Ok(ID.to_string()));
    }

    // [utest->swdd~nerdctl-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_start_container_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        static ID: &str = "unknown_id";

        super::CliCommand::reset();
        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["start", ID])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let start_config = super::NerdctlStartConfig {
            general_options: vec![],
            container_id: ID.into(),
        };
        let res = NerdctlCli::nerdctl_start(start_config, "test_workload_name").await;
        assert_eq!(res, Err(SAMPLE_ERROR_MESSAGE.to_string()));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_created() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "created",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("created"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_configured() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "configured",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("configured"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_initialized() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "initialized",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("initialized"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_succeeded() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "exited",
                    exit_code: 0,
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::succeeded())));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "exited",
                    exit_code: 1,
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::failed("Exit code: '1'"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_running() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_stopping() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "stopping",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::stopping("stopping"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_stopped() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "stopped",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::stopping("stopped"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_removing() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "removing",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::stopping("removing"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_unknown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "unknown",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::unknown("unknown"))));
    }

    // [utest->swdd~nerdctl-state-getter-maps-state~3]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_undefined() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "undefined",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::unknown("undefined"))));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_nerdctl_error_retry_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--format=json"])
            .exec_returns(Err("simulated error".to_string()));
        // NerdctlCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect("nerdctl", mock_cli_command.clone());
        super::CliCommand::new_expect("nerdctl", mock_cli_command);

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_nerdctl_error_retry_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Err("simulated error".to_string())),
        );
        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_nerdctl_existing_ps_result_to_old() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        let old_time_stamp = time::Instant::now() - Duration::from_secs(10);

        *super::LAST_PS_RESULT.lock().await = Some(NerdctlPsCache {
            last_update: old_time_stamp,
            cache: Arc::new(super::NerdctlPsResult {
                container_states: Ok([("test_id".into(), ExecutionState::failed("Some error"))]
                    .into_iter()
                    .collect()),
            }),
        });

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response_retry_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--format=json"])
            .exec_returns(Ok("non-json response from nerdctl".to_string()));
        // NerdctlCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect("nerdctl", mock_cli_command.clone());
        super::CliCommand::new_expect("nerdctl", mock_cli_command);

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse nerdctl output") ));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response_retry_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok("non-json response from nerdctl".to_string())),
        );
        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_stop_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["stop", "--ignore", "test_id"])
                .exec_returns(Err("simulated error".to_string())),
        );

        assert_eq!(
            NerdctlCli::remove_workloads_by_id("test_id").await,
            Err("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_remove_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["stop", "--ignore", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["rm", "--ignore", "test_id"])
                .exec_returns(Err("simulated error".to_string())),
        );

        assert_eq!(
            NerdctlCli::remove_workloads_by_id("test_id").await,
            Err("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["stop", "--ignore", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        super::CliCommand::new_expect(
            "nerdctl",
            super::CliCommand::default()
                .expect_args(&["rm", "--ignore", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        let res = NerdctlCli::remove_workloads_by_id("test_id").await;
        assert_eq!(res, Ok(()));
    }

    #[derive(Serialize, Clone, Default)]
    #[serde(rename_all = "PascalCase")]
    struct TestNerdctlContainerInfo<'a> {
        state: &'a str,
        exit_code: u8,
        #[serde(serialize_with = "serialize_as_map")]
        labels: &'a [(&'a str, &'a str)],
        id: &'a str,
        pod: &'a str,
    }

    impl ToJson for [TestNerdctlContainerInfo<'_>] {
        fn to_json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }

    trait ToJson {
        fn to_json(&self) -> String;
    }
}

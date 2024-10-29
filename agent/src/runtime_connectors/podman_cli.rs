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

use base64::Engine;
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

const PODMAN_CMD: &str = "podman";
const API_PIPES_MOUNT_POINT: &str = "/run/ankaios/control_interface";
const PODMAN_PS_CACHE_MAX_AGE: Duration = Duration::from_millis(1000);

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
pub struct PodmanRunConfig {
    pub general_options: Vec<String>,
    pub command_options: Vec<String>,
    pub image: String,
    pub command_args: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PodmanStartConfig {
    pub general_options: Vec<String>,
    pub container_id: String,
}

impl From<PodmanContainerInfo> for ContainerState {
    fn from(value: PodmanContainerInfo) -> Self {
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
                    "Mapping the container state '{}' to the container state 'Unknown'",
                    state
                );
                ContainerState::Unknown
            }
        }
    }
}

// [impl->swdd~podman-state-getter-maps-state~3]
impl From<PodmanContainerInfo> for ExecutionState {
    fn from(value: PodmanContainerInfo) -> Self {
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
                    "Mapping the container state '{}' to the execution state 'ExecUnknown'",
                    state
                );
                ExecutionState::unknown(state)
            }
        }
    }
}

struct PodmanPsCache {
    last_update: Instant,
    cache: Arc<PodmanPsResult>,
}

struct TimedPodmanPsResult(Mutex<Option<PodmanPsCache>>);

impl TimedPodmanPsResult {
    async fn reset(&self) {
        *self.lock().await = None;
    }

    // [impl->swdd~podmancli-container-state-cache-refresh~1]
    async fn get(&self) -> Arc<PodmanPsResult> {
        let mut guard = self.lock().await;

        if let Some(value) = &mut *guard {
            if value.last_update.elapsed() > PODMAN_PS_CACHE_MAX_AGE {
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

    async fn new_inner() -> PodmanPsCache {
        let mut res = PodmanCli::list_states_internal().await;
        if res.is_err() {
            // This is a workaround for the known issue in podman (podman ps sometimes fails).
            log::trace!("'podman ps' has returned error - let's retry it.");
            res = PodmanCli::list_states_internal().await;
        }
        PodmanPsCache {
            last_update: Instant::now(),
            cache: Arc::new(res.into()),
        }
    }
}

impl Deref for TimedPodmanPsResult {
    type Target = Mutex<Option<PodmanPsCache>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// [impl->swdd~podmancli-container-state-cache-all-containers~1]
#[derive(Debug)]
struct PodmanPsResult {
    container_states: Result<HashMap<String, ExecutionState>, String>,
    pod_states: Result<HashMap<String, Vec<ContainerState>>, String>,
}

impl From<Result<Vec<PodmanContainerInfo>, String>> for PodmanPsResult {
    fn from(value: Result<Vec<PodmanContainerInfo>, String>) -> Self {
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
                    pod_states: Ok(pod_states),
                }
            }
            Err(err) => Self {
                container_states: Err(err.clone()),
                pod_states: Err(err),
            },
        }
    }
}

static LAST_PS_RESULT: TimedPodmanPsResult = TimedPodmanPsResult(Mutex::const_new(Option::None));

pub struct PodmanCli {}

#[cfg_attr(test, automock)]
impl PodmanCli {
    pub async fn reset_ps_cache() {
        LAST_PS_RESULT.reset().await;
    }

    pub async fn play_kube(
        general_options: &[String],
        play_options: &[String],
        kube_yml: &[u8],
    ) -> Result<Vec<String>, String> {
        let mut args: Vec<&str> = general_options.iter().map(|x| x as &str).collect();
        args.extend(["kube", "play", "--quiet"]);
        args.extend(play_options.iter().map(|x| x as &str));
        args.push("-");
        log::debug!("Executing play kube with args: {args:?}");
        let result = CliCommand::new(PODMAN_CMD)
            .args(&args)
            .stdin(kube_yml)
            .exec()
            .await?;
        Ok(Self::parse_pods_from_output(result))
    }

    fn parse_pods_from_output(input: String) -> Vec<String> {
        let mut result = Vec::new();
        let mut is_pod = false;
        for line in input.split('\n') {
            let line = line.trim();
            if line == "Pod:" {
                is_pod = true;
                continue;
            }
            if line.ends_with(':') {
                is_pod = false;
                continue;
            }
            if is_pod && !line.is_empty() {
                result.push(line.into())
            }
        }
        result
    }

    pub async fn down_kube(down_options: &[String], kube_yml: &[u8]) -> Result<(), String> {
        let mut args = vec!["kube", "down"];
        args.extend(down_options.iter().map(|x| x as &str));
        args.push("-");

        CliCommand::new(PODMAN_CMD)
            .args(&args)
            .stdin(kube_yml)
            .exec()
            .await?;
        Ok(())
    }

    pub async fn list_workload_ids_by_label(key: &str, value: &str) -> Result<Vec<String>, String> {
        log::debug!("Listing workload ids for: {}='{}'", key, value,);
        let output = CliCommand::new(PODMAN_CMD)
            .args(&[
                "ps",
                "--all",
                "--filter",
                &format!("label={key}={value}"),
                "--format=json",
            ])
            .exec()
            .await?;

        let res: Vec<PodmanContainerInfo> = serde_json::from_str(&output)
            .map_err(|err| format!("Could not parse podman output: '{}'", err))?;

        Ok(res.into_iter().map(|x| x.id).collect())
    }

    pub async fn list_workload_names_by_label(
        key: &str,
        value: &str,
    ) -> Result<Vec<String>, String> {
        log::trace!("Listing workload names for: '{}'='{}'", key, value,);
        let output = CliCommand::new(PODMAN_CMD)
            .args(&[
                "ps",
                "--all",
                "--filter",
                &format!("label={key}={value}"),
                "--format=json",
            ])
            .exec()
            .await?;

        let res: Vec<PodmanContainerInfo> = serde_json::from_str(&output)
            .map_err(|err| format!("Could not parse podman output: '{}'", err))?;

        let mut names: Vec<String> = Vec::new();
        for mut podman_info in res {
            if let Some(name_val) = podman_info.labels.get_mut("name") {
                names.push(name_val.to_string());
            }
        }
        Ok(names)
    }

    pub async fn podman_run(
        mut run_config: PodmanRunConfig,
        workload_name: &str,
        agent: &str,
        control_interface_path: Option<PathBuf>,
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
        // [impl->swdd~podman-create-workload-sets-optionally-container-name~2]
        args.append(&mut vec!["--name".into(), workload_name.to_string()]);

        args.append(&mut run_config.command_options);

        // [impl->swdd~podman-create-workload-mounts-fifo-files~1]
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

        // [impl->swdd~podman-create-workload-creates-labels~2]
        args.push(format!("--label=name={workload_name}"));
        args.push(format!("--label=agent={agent}"));
        args.push(run_config.image);

        args.append(&mut run_config.command_args);

        log::debug!("The args are: '{:?}'", args);
        let id = CliCommand::new(PODMAN_CMD)
            .args(&args.iter().map(|x| &**x).collect::<Vec<&str>>())
            .exec()
            .await?
            .trim()
            .to_string();
        Ok(id)
    }

    pub async fn podman_start(
        start_config: PodmanStartConfig,
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

        let id = CliCommand::new(PODMAN_CMD)
            .args(&args.iter().map(|x| &**x).collect::<Vec<&str>>())
            .exec()
            .await?
            .trim()
            .to_string();
        Ok(id)
    }

    // [impl->swdd~podmancli-uses-container-state-cache~1]
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

    // [impl->swdd~podmancli-uses-container-state-cache~1]
    // [impl->swdd~podman-kube-state-getter-treats-missing-pods-as-unknown~1]
    pub async fn list_states_from_pods(pods: &[String]) -> Result<Vec<ContainerState>, String> {
        let ps_result = LAST_PS_RESULT.get().await;
        let all_pod_states = ps_result
            .as_ref()
            .pod_states
            .as_ref()
            .map_err(|err| err.to_owned())?;
        Ok(pods
            .iter()
            .flat_map(|key| {
                all_pod_states.get(key).cloned().unwrap_or_else(|| {
                    log::warn!("The pod '{}' is missing.", key);
                    vec![ContainerState::Unknown]
                })
            })
            .collect())
    }

    async fn list_states_internal() -> Result<Vec<PodmanContainerInfo>, String> {
        let output = CliCommand::new(PODMAN_CMD)
            .args(&["ps", "--all", "--format=json"])
            .exec()
            .await?;

        serde_json::from_str(&output)
            .map_err(|err| format!("Could not parse podman output:{}", err))
    }

    pub async fn list_volumes_by_name(name: &str) -> Result<Vec<String>, String> {
        let output = CliCommand::new(PODMAN_CMD)
            .args(&[
                "volume",
                "ls",
                "--filter",
                &format!("name={name}"),
                "--format={{.Name}}",
            ])
            .exec()
            .await?;
        Ok(output
            .split('\n')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect())
    }

    // [impl->swdd~podman-kube-create-workload-creates-config-volume~1]
    // [impl->swdd~podman-kube-create-workload-creates-pods-volume~1]
    pub async fn store_data_as_volume(volume_name: &str, data: &str) -> Result<(), String> {
        let _ = Self::remove_volume(volume_name).await;

        let mut label = "--label=data=".into();
        base64::engine::general_purpose::STANDARD_NO_PAD.encode_string(data.as_bytes(), &mut label);
        CliCommand::new(PODMAN_CMD)
            .args(&["volume", "create", &label, volume_name])
            .exec()
            .await?;
        Ok(())
    }

    pub async fn read_data_from_volume(volume_name: &str) -> Result<String, String> {
        let result = CliCommand::new(PODMAN_CMD)
            .args(&["volume", "inspect", volume_name])
            .exec()
            .await?;

        let res: Vec<Volume> = serde_json::from_str(&result)
            .map_err(|err| format!("Could not decoded volume information as JSON: {}", err))?;
        let res = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(
                &res.first()
                    .ok_or_else(|| "No volume returned".to_string())?
                    .labels
                    .data,
            )
            .map_err(|err| format!("Could not base64 decoded volume's data label: {}", err))?;
        let res = String::from_utf8(res)
            .map_err(|err| format!("Could not decode data stored in volume: {}", err))?;

        Ok(res)
    }

    pub async fn remove_volume(volume_name: &str) -> Result<(), String> {
        CliCommand::new(PODMAN_CMD)
            .args(&["volume", "rm", volume_name])
            .exec()
            .await?;
        Ok(())
    }

    pub async fn remove_workloads_by_id(workload_id: &str) -> Result<(), String> {
        // Containers may have "--rm" flag -> it can happen, that they already do not exist.
        let args = vec!["stop", "--ignore", workload_id];
        CliCommand::new(PODMAN_CMD).args(&args).exec().await?;
        let args = vec!["rm", "--ignore", workload_id];
        CliCommand::new(PODMAN_CMD).args(&args).exec().await?;
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Volume {
    labels: DataLabel,
}

#[derive(Deserialize, Debug)]
struct DataLabel {
    data: String,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "PascalCase")]
struct PodmanContainerInfo {
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
// [utest->swdd~podman-uses-podman-cli~1]
// [utest->swdd~podman-kube-uses-podman-cli~1]
#[cfg(test)]
mod tests {
    use super::{ContainerState, PodmanCli, PodmanPsCache};

    use super::PodmanContainerInfo;
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;
    use common::objects::ExecutionState;
    use common::test_utils::serialize_as_map;
    use serde::Serialize;
    use std::sync::Arc;
    use std::time::{self, Duration};

    const SAMPLE_ERROR_MESSAGE: &str = "error message";

    #[test]
    fn utest_container_state_from_podman_container_info_created() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_configured() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_initialized() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_exited() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_paused() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_running() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_stopping() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_stopped() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_removing() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_undefined() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    fn utest_container_state_from_podman_container_info_unknown() {
        let container_state: ContainerState = PodmanContainerInfo {
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
    async fn utest_play_kube_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "-gen",
                    "--eral",
                    "kube",
                    "play",
                    "--quiet",
                    "-play",
                    "--options",
                    "-",
                ])
                .expect_stdin(sample_input)
                .exec_returns(Ok(concat!(
                    "Not-Pod:\n",
                    "1\n",
                    "2\n",
                    "Pod:\n",
                    "3\n",
                    "\n",
                    "Not-Pod:\n",
                    "4\n",
                    "Pod:\n",
                    "5\n",
                    "6\n",
                    "Not-Pod:\n",
                    "7\n",
                )
                .into())),
        );

        let res = PodmanCli::play_kube(
            &["-gen".into(), "--eral".into()],
            &["-play".into(), "--options".into()],
            sample_input.as_bytes(),
        )
        .await;
        assert!(
            matches!(res, Ok(pods) if pods == ["3".to_string(), "5".to_string(), "6".to_string()])
        );
    }

    #[tokio::test]
    async fn utest_play_kube_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "-gen",
                    "--eral",
                    "kube",
                    "play",
                    "--quiet",
                    "-play",
                    "--options",
                    "-",
                ])
                .expect_stdin(sample_input)
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = PodmanCli::play_kube(
            &["-gen".into(), "--eral".into()],
            &["-play".into(), "--options".into()],
            sample_input.as_bytes(),
        )
        .await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_down_kube_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "down", "-a", "-b", "-"])
                .expect_stdin(sample_input)
                .exec_returns(Ok("".into())),
        );

        let res = PodmanCli::down_kube(&["-a".into(), "-b".into()], sample_input.as_bytes()).await;
        assert!(matches!(res, Ok(..)));
    }

    #[tokio::test]
    async fn utest_down_kube_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "down", "-a", "-b", "-"])
                .expect_stdin(sample_input)
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = PodmanCli::down_kube(&["-a".into(), "-b".into()], sample_input.as_bytes()).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok([
                    TestPodmanContainerInfo {
                        id: "result1",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        id: "result2",
                        ..Default::default()
                    },
                ]
                .to_json())),
        );

        let res = PodmanCli::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Ok(res) if res == vec!["result1", "result2"]));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
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

        let res = PodmanCli::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_broken_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok("non-json response from podman".into())),
        );

        let res = PodmanCli::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg.contains("Could not parse podman output")));
    }

    #[tokio::test]
    async fn utest_list_workload_names_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    labels: &[("name", "workload_name")],
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec!["workload_name".into()]));
    }

    #[tokio::test]
    async fn utest_list_workload_names_not_found_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok([TestPodmanContainerInfo::default()].to_json())),
        );

        let res = PodmanCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec![]));
    }

    #[tokio::test]
    async fn utest_list_workload_names_podman_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
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

        let res = PodmanCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_workload_names_broken_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok("non-json response from podman".to_string())),
        );

        let res = PodmanCli::list_workload_names_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output") ));
    }

    // [utest->swdd~podman-create-workload-creates-labels~2]
    // [utest->swdd~podman-create-workload-sets-optionally-container-name~2]
    // [utest->swdd~podman-create-workload-mounts-fifo-files~1]
    #[tokio::test]
    async fn utest_run_container_success_no_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
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

        let run_config = super::PodmanRunConfig {
            general_options: Vec::new(),
            command_options: Vec::new(),
            image: "alpine:latest".into(),
            command_args: Vec::new(),
        };
        let res = PodmanCli::podman_run(run_config, "test_workload_name", "test_agent", None).await;
        assert_eq!(res, Ok("test_id".to_string()));
    }

    #[tokio::test]
    async fn utest_run_container_fail_no_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
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

        let run_config = super::PodmanRunConfig {
            general_options: Vec::new(),
            command_options: Vec::new(),
            image: "alpine:latest".into(),
            command_args: Vec::new(),
        };
        let res = PodmanCli::podman_run(run_config, "test_workload_name", "test_agent", None).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    // [utest->swdd~podman-create-workload-sets-optionally-container-name~2]
    // [utest->swdd~podman-create-workload-mounts-fifo-files~1]
    #[tokio::test]
    async fn utest_run_container_success_with_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
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
                    "--label=name=test_workload_name",
                    "--label=agent=test_agent",
                    "alpine:latest",
                    "sh",
                ])
                .exec_returns(Ok("test_id".to_string())),
        );

        let run_config = super::PodmanRunConfig {
            general_options: vec!["--remote".into()],
            command_options: vec!["--network=host".into(), "--name".into(), "myCont".into()],
            image: "alpine:latest".into(),
            command_args: vec!["sh".into()],
        };
        let res = PodmanCli::podman_run(
            run_config,
            "test_workload_name",
            "test_agent",
            Some("/test/path".into()),
        )
        .await;
        assert_eq!(res, Ok("test_id".to_string()));
    }

    // [utest->swdd~podman-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_start_container_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        const ID: &str = "test_id";

        super::CliCommand::reset();
        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["--remote", "start", ID])
                .exec_returns(Ok(ID.to_string())),
        );

        let start_config = super::PodmanStartConfig {
            general_options: vec!["--remote".into()],
            container_id: ID.into(),
        };
        let res = PodmanCli::podman_start(start_config, "test_workload_name").await;
        assert_eq!(res, Ok(ID.to_string()));
    }

    // [utest->swdd~podman-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_start_container_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        static ID: &str = "unknown_id";

        super::CliCommand::reset();
        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["start", ID])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let start_config = super::PodmanStartConfig {
            general_options: vec![],
            container_id: ID.into(),
        };
        let res = PodmanCli::podman_start(start_config, "test_workload_name").await;
        assert_eq!(res, Err(SAMPLE_ERROR_MESSAGE.to_string()));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_created() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        PodmanCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "created",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("created"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_configured() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        PodmanCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "configured",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("configured"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_initialized() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        PodmanCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "initialized",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("initialized"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_succeeded() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "exited",
                    exit_code: 0,
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::succeeded())));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "exited",
                    exit_code: 1,
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::failed("Exit code: '1'"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_running() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_stopping() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "stopping",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::stopping("stopping"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_stopped() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        PodmanCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "stopped",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::stopping("stopped"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_removing() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        PodmanCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "removing",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::stopping("removing"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_unknown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "unknown",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::unknown("unknown"))));
    }

    // [utest->swdd~podman-state-getter-maps-state~3]
    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_undefined() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        PodmanCli::reset_ps_cache().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "undefined",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::unknown("undefined"))));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_podman_error_retry_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--format=json"])
            .exec_returns(Err("simulated error".to_string()));
        // PodmanCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect("podman", mock_cli_command.clone());
        super::CliCommand::new_expect("podman", mock_cli_command);

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_podman_error_retry_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Err("simulated error".to_string())),
        );
        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    // [utest->podmancli-uses-container-state-cache~1]
    #[tokio::test]
    async fn utest_list_states_by_id_podman_use_existing_ps_result() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        *super::LAST_PS_RESULT.lock().await = Some(PodmanPsCache {
            last_update: time::Instant::now(),
            cache: Arc::new(super::PodmanPsResult {
                container_states: Ok([("test_id".into(), ExecutionState::running())]
                    .into_iter()
                    .collect()),
                pod_states: Err("".into()),
            }),
        });

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    // [utest->swdd~podmancli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_podman_existing_ps_result_to_old() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        let old_time_stamp = time::Instant::now() - Duration::from_secs(10);

        *super::LAST_PS_RESULT.lock().await = Some(PodmanPsCache {
            last_update: old_time_stamp,
            cache: Arc::new(super::PodmanPsResult {
                container_states: Ok([("test_id".into(), ExecutionState::failed("Some error"))]
                    .into_iter()
                    .collect()),
                pod_states: Err("".into()),
            }),
        });

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response_retry_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--format=json"])
            .exec_returns(Ok("non-json response from podman".to_string()));
        // PodmanCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect("podman", mock_cli_command.clone());
        super::CliCommand::new_expect("podman", mock_cli_command);

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output") ));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response_retry_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok("non-json response from podman".to_string())),
        );
        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    id: "test_id",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([
                    TestPodmanContainerInfo {
                        pod: "pod1",
                        state: "running",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod_other",
                        state: "created",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod2",
                        state: "exited",
                        exit_code: 42,
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod2",
                        state: "unknown",
                        ..Default::default()
                    },
                ]
                .to_json())),
        );
        let res = PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into()]).await;
        assert!(
            matches!(res, Ok(states) if states == [ContainerState::Running, ContainerState::Exited(42), ContainerState::Unknown] )
        );
    }

    // [utest->swdd~podman-kube-state-getter-treats-missing-pods-as-unknown~1]
    #[tokio::test]
    async fn utest_list_states_from_pods_some_missing_leads_to_unknown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([
                    TestPodmanContainerInfo {
                        pod: "pod1",
                        state: "running",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod3",
                        state: "exited",
                        exit_code: 42,
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod3",
                        state: "unknown",
                        ..Default::default()
                    },
                ]
                .to_json())),
        );
        let res =
            PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;
        assert!(
            matches!(res, Ok(states) if states == [ContainerState::Running, ContainerState::Unknown, ContainerState::Exited(42), ContainerState::Unknown] )
        );
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_command_fails_retry_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--format=json"])
            .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into()));
        // PodmanCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect("podman", mock_cli_command.clone());
        super::CliCommand::new_expect("podman", mock_cli_command);

        let res =
            PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;

        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE ));
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_command_fails_retry_succeeds() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );
        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([
                    TestPodmanContainerInfo {
                        pod: "pod1",
                        state: "running",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod_other",
                        state: "created",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod2",
                        state: "exited",
                        exit_code: 42,
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod2",
                        state: "unknown",
                        ..Default::default()
                    },
                ]
                .to_json())),
        );

        let res = PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into()]).await;
        assert!(
            matches!(res, Ok(states) if states == [ContainerState::Running, ContainerState::Exited(42), ContainerState::Unknown] )
        );
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_result_not_json_retry_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--format=json"])
            .exec_returns(Ok("non-json response from podman".into()));
        // PodmanCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect("podman", mock_cli_command.clone());
        super::CliCommand::new_expect("podman", mock_cli_command);

        let res =
            PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;

        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output:") ));
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_result_not_json_retry_succeeds() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok("non-json response from podman".into())),
        );
        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([
                    TestPodmanContainerInfo {
                        pod: "pod1",
                        state: "running",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod_other",
                        state: "created",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod2",
                        state: "exited",
                        exit_code: 42,
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        pod: "pod2",
                        state: "unknown",
                        ..Default::default()
                    },
                ]
                .to_json())),
        );

        let res = PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into()]).await;
        assert!(
            matches!(res, Ok(states) if states == [ContainerState::Running, ContainerState::Exited(42), ContainerState::Unknown] )
        );
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_empty_input() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([TestPodmanContainerInfo {
                    pod: "pod1",
                    state: "running",
                    ..Default::default()
                }]
                .to_json())),
        );

        assert!(PodmanCli::list_states_from_pods(&[])
            .await
            .unwrap()
            .is_empty());
    }

    // [utest->swdd~podmancli-container-state-cache-all-containers~1]
    // [utest->swdd~podmancli-uses-container-state-cache~1]
    #[tokio::test]
    async fn utest_list_states_uses_cache() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--format=json"])
                .exec_returns(Ok([
                    TestPodmanContainerInfo {
                        id: "id1",
                        pod: "pod1",
                        state: "running",
                        ..Default::default()
                    },
                    TestPodmanContainerInfo {
                        id: "id2",
                        pod: "pod2",
                        state: "exited",
                        exit_code: 0,
                        ..Default::default()
                    },
                ]
                .to_json())),
        );

        let _ = PodmanCli::list_states_by_id("id1").await;

        assert!(
            matches!(PodmanCli::list_states_by_id("id2").await, Ok(Some(state)) if state == ExecutionState::succeeded() )
        );
        assert!(
            matches!(PodmanCli::list_states_from_pods(&["pod2".into()]).await, Ok(states) if states == [ContainerState::Exited(0)] )
        );
    }

    #[tokio::test]
    async fn utest_list_volumes_by_name_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "volume",
                    "ls",
                    "--filter",
                    "name=volume_regex",
                    "--format={{.Name}}",
                ])
                .exec_returns(Ok("volume_1\nvolume_2\nvolume_3\n".into())),
        );

        let res = PodmanCli::list_volumes_by_name("volume_regex").await;

        assert!(matches!(res, Ok(volumes) if volumes == ["volume_1", "volume_2", "volume_3"] ));
    }

    #[tokio::test]
    async fn utest_list_volumes_by_name_command_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "volume",
                    "ls",
                    "--filter",
                    "name=volume_regex",
                    "--format={{.Name}}",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = PodmanCli::list_volumes_by_name("volume_regex").await;

        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE ));
    }

    // [utest->swdd~podman-kube-create-workload-creates-config-volume~1]
    // [utest->swdd~podman-kube-create-workload-creates-pods-volume~1]
    #[tokio::test]
    async fn utest_store_data_as_volume_success_volume_existed_before() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "rm", "volume_1"])
                .exec_returns(Ok("volume_1".into())),
        );

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "create", "--label=data=QUJDRA", "volume_1"])
                .exec_returns(Ok("".into())),
        );

        let res = PodmanCli::store_data_as_volume("volume_1", "ABCD").await;

        assert!(matches!(res, Ok(..)));
    }

    #[tokio::test]
    async fn utest_store_data_as_volume_success_volume_did_not_exist_before() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "rm", "volume_1"])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "create", "--label=data=QUJDRA", "volume_1"])
                .exec_returns(Ok("".into())),
        );

        let res = PodmanCli::store_data_as_volume("volume_1", "ABCD").await;

        assert!(matches!(res, Ok(..)));
    }

    #[tokio::test]
    async fn utest_read_data_from_volume_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "inspect", "volume_1"])
                .exec_returns(Ok(r#"[{"Labels": {"data": "QUJDRA"}}]"#.into())),
        );

        let res = PodmanCli::read_data_from_volume("volume_1").await;
        assert!(matches!(res, Ok(data) if data == "ABCD"));
    }

    #[tokio::test]
    async fn utest_read_data_from_volume_command_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "inspect", "volume_1"])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = PodmanCli::read_data_from_volume("volume_1").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_read_data_from_volume_command_returns_illegal_json() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "inspect", "volume_1"])
                .exec_returns(Ok("[{}]".into())),
        );

        let res = PodmanCli::read_data_from_volume("volume_1").await;

        assert!(
            matches!(res, Err(msg) if msg.starts_with("Could not decoded volume information as JSON:"))
        );
    }

    #[tokio::test]
    async fn utest_read_data_from_volume_command_returns_no_volume() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "inspect", "volume_1"])
                .exec_returns(Ok("[]".into())),
        );

        let res = PodmanCli::read_data_from_volume("volume_1").await;

        assert!(matches!(res, Err(msg) if msg == "No volume returned"));
    }

    #[tokio::test]
    async fn utest_read_data_from_volume_data_contains_illegal_base64() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "inspect", "volume_1"])
                .exec_returns(Ok(r#"[{"Labels": {"data": "a"}}]"#.into())),
        );

        let res = PodmanCli::read_data_from_volume("volume_1").await;

        assert!(
            matches!(res, Err(msg) if msg.starts_with("Could not base64 decoded volume's data label:"))
        );
    }

    #[tokio::test]
    async fn utest_read_data_from_volume_data_contains_illegal_utf8() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "inspect", "volume_1"])
                .exec_returns(Ok(r#"[{"Labels": {"data": "gA"}}]"#.into())),
        );

        let res = PodmanCli::read_data_from_volume("volume_1").await;

        assert!(
            matches!(res, Err(msg) if msg.starts_with("Could not decode data stored in volume:"))
        );
    }

    #[tokio::test]
    async fn utest_remove_volume_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "rm", "volume_1"])
                .exec_returns(Ok("".into())),
        );

        let res = PodmanCli::remove_volume("volume_1").await;

        assert!(matches!(res, Ok(..)));
    }

    #[tokio::test]
    async fn utest_remove_volume_command_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["volume", "rm", "volume_1"])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = PodmanCli::remove_volume("volume_1").await;

        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_stop_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["stop", "--ignore", "test_id"])
                .exec_returns(Err("simulated error".to_string())),
        );

        assert_eq!(
            PodmanCli::remove_workloads_by_id("test_id").await,
            Err("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_remove_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["stop", "--ignore", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["rm", "--ignore", "test_id"])
                .exec_returns(Err("simulated error".to_string())),
        );

        assert_eq!(
            PodmanCli::remove_workloads_by_id("test_id").await,
            Err("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["stop", "--ignore", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["rm", "--ignore", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        let res = PodmanCli::remove_workloads_by_id("test_id").await;
        assert_eq!(res, Ok(()));
    }

    #[derive(Serialize, Clone, Default)]
    #[serde(rename_all = "PascalCase")]
    struct TestPodmanContainerInfo<'a> {
        state: &'a str,
        exit_code: u8,
        #[serde(serialize_with = "serialize_as_map")]
        labels: &'a [(&'a str, &'a str)],
        id: &'a str,
        pod: &'a str,
    }

    impl<'a> ToJson for [TestPodmanContainerInfo<'a>] {
        fn to_json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }

    trait ToJson {
        fn to_json(&self) -> String;
    }
}

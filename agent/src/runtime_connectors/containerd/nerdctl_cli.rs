// Copyright (c) 2025 Elektrobit Automotive GmbH
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

// [impl->swdd~containerd-state-getter-maps-state~1]
impl From<NerdctlContainerInfo> for ExecutionState {
    fn from(value: NerdctlContainerInfo) -> Self {
        match value.state.status.to_lowercase().as_str() {
            "created" => ExecutionState::starting(value.state.status),
            "exited" if value.state.exit_code == 0 => ExecutionState::succeeded(),
            "exited" if value.state.exit_code != 0 => {
                ExecutionState::failed(format!("Exit code: '{}'", value.state.exit_code))
            }
            "running" => ExecutionState::running(),
            "removing" => ExecutionState::stopping(value.state.status),
            "paused" => ExecutionState::unknown(value.state.status),
            "restarting" => ExecutionState::starting(value.state.status),
            "dead" => ExecutionState::failed(format!("Exit code: '{}'", value.state.exit_code)),
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

                for container_entry in container_infos {
                    container_states.insert(
                        container_entry.id.id.clone(),
                        container_entry.clone().into(),
                    );
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
                "--no-trunc",
                "--filter",
                &format!("label={key}={value}"),
                "--format=json",
            ])
            .exec()
            .await?;

        let mut container_ids = Vec::new();
        for line in output.lines().filter(|l| !l.trim().is_empty()) {
            let container_id: NerdctlContainerId = serde_json::from_str(line)
                .map_err(|err| format!("Could not parse nerdctl output: '{err}'"))?;
            container_ids.push(container_id.id);
        }

        log::trace!("Parsed container ids: {container_ids:?}");

        Ok(container_ids)
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
                "--no-trunc",
                "--filter",
                &format!("label={key}={value}"),
                "--format=json",
            ])
            .exec()
            .await?;

        let mut names = Vec::new();
        for line in output.lines().filter(|l| !l.trim().is_empty()) {
            let mut container_labels: NerdctlContainerLabels = serde_json::from_str(line)
                .map_err(|err| format!("Could not parse nerdctl output: '{err}'"))?;
            if let Some(name) = container_labels.labels.remove("name") {
                names.push(name);
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
            .args(&["ps", "--all", "--no-trunc", "--format=json"])
            .exec()
            .await?;

        let mut container_ids = Vec::new();
        for line in output.lines().filter(|l| !l.trim().is_empty()) {
            let container_id: NerdctlContainerId = serde_json::from_str(line)
                .map_err(|err| format!("Could not parse nerdctl ps output: '{err}'"))?;
            container_ids.push(container_id.id);
        }

        if container_ids.is_empty() {
            log::warn!("No containers found.");
            return Ok(Vec::default());
        }

        let inspect_args: Vec<&str> = std::iter::once("inspect")
            .chain(container_ids.iter().map(String::as_str))
            .collect();

        let output = CliCommand::new(NERDCTL_CMD)
            .args(&inspect_args)
            .exec()
            .await?;

        let container_info: Vec<NerdctlContainerInfo> =
            serde_json::from_str(&output).map_err(|err| {
                format!("Could not parse nerdctl inspect output: '{err}', output: {output}")
            })?;

        Ok(container_info)
    }

    pub async fn remove_workloads_by_id(workload_id: &str) -> Result<(), String> {
        /* nerdctl does not support '-d' and '--rm' flags specified together
        (https://github.com/containerd/nerdctl/issues/3698) and no 'ignore' flag. */

        const CONTAINER_NOT_EXISTING: &str = "no such container";

        match CliCommand::new(NERDCTL_CMD)
            .args(&["stop", workload_id])
            .exec()
            .await
        {
            Ok(_) => {}
            Err(err) if err.contains(CONTAINER_NOT_EXISTING) => {
                log::debug!("Tried to stop container with id '{workload_id}' that does not exist.");
            }
            Err(err) => return Err(err),
        }

        match CliCommand::new(NERDCTL_CMD)
            .args(&["rm", workload_id])
            .exec()
            .await
        {
            Ok(_) => Ok(()),
            Err(err) if err.contains(CONTAINER_NOT_EXISTING) => {
                log::debug!(
                    "Tried to remove container with id '{workload_id}' that does not exist."
                );
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct NerdctlContainerInfo {
    #[serde(rename = "State")]
    state: NerdctlContainerState,
    #[serde(flatten)]
    id: NerdctlContainerId,
}

#[derive(Debug, Deserialize, Default, Clone, Serialize)]
struct NerdctlContainerId {
    #[serde(rename = "ID", alias = "Id")]
    id: String,
}

#[derive(Debug, Deserialize, Default, Clone, Serialize)]
struct NerdctlContainerLabels {
    #[serde(rename = "Labels", deserialize_with = "parse_labels")]
    labels: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Default, Clone, Serialize)]
struct NerdctlContainerState {
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "ExitCode")]
    exit_code: u8,
}

fn parse_labels<'a, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'a>,
{
    let raw_labels: String = Deserialize::deserialize(deserializer)?;
    let mut labels = HashMap::new();

    for part in raw_labels.split(',') {
        if let Some((key, value)) = part.split_once('=') {
            labels.insert(key.to_string(), value.to_string());
        }
    }
    Ok(labels)
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
// [utest->swdd~containerd-uses-nerdctl-cli~1]
// [utest->swdd~nerdctl-kube-uses-nerdctl-cli~1]
#[cfg(test)]
mod tests {
    use super::{NERDCTL_CMD, NerdctlCli, NerdctlPsCache};

    use crate::test_helper::MOCKALL_CONTEXT_SYNC;
    use common::objects::ExecutionState;
    use serde::Serialize;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{self, Duration};

    const SAMPLE_ERROR_MESSAGE: &str = "error message";
    const WORKLOAD_ID: &str = "test_id";

    #[tokio::test]
    async fn utest_list_workload_ids_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--no-trunc",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok([
                    TestNerdctlContainerId {
                        id: "result1".into(),
                    },
                    TestNerdctlContainerId {
                        id: "result2".into(),
                    },
                ]
                .to_json())),
        );

        let res = NerdctlCli::list_workload_ids_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec!["result1".to_owned(), "result2".to_owned()]));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--no-trunc",
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
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--no-trunc",
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
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--no-trunc",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok(TestNerdctlContainerLabels {
                    labels: HashMap::from([("name".to_owned(), "workload_name".to_owned())]),
                }
                .to_string())),
        );

        let res = NerdctlCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec!["workload_name".into()]));
    }

    #[tokio::test]
    async fn utest_list_workload_names_not_found_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--no-trunc",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok(TestNerdctlContainerLabels::default().to_string())),
        );

        let res = NerdctlCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(Vec::default()));
    }

    #[tokio::test]
    async fn utest_list_workload_names_nerdctl_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--no-trunc",
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
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--no-trunc",
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
            NERDCTL_CMD,
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
                .exec_returns(Ok(WORKLOAD_ID.to_owned())),
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
        assert_eq!(res, Ok("test_id".to_owned()));
    }

    #[tokio::test]
    async fn utest_run_container_fail_no_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
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
            NERDCTL_CMD,
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
                .exec_returns(Ok(WORKLOAD_ID.to_owned())),
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
        assert_eq!(res, Ok(WORKLOAD_ID.to_owned()));
    }

    // [utest->swdd~nerdctl-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_start_container_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["--remote", "start", WORKLOAD_ID])
                .exec_returns(Ok(WORKLOAD_ID.to_owned())),
        );

        let start_config = super::NerdctlStartConfig {
            general_options: vec!["--remote".into()],
            container_id: WORKLOAD_ID.into(),
        };
        let res = NerdctlCli::nerdctl_start(start_config, "test_workload_name").await;
        assert_eq!(res, Ok(WORKLOAD_ID.to_owned()));
    }

    // [utest->swdd~nerdctl-create-workload-starts-existing-workload~1]
    #[tokio::test]
    async fn utest_start_container_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        static ID: &str = "unknown_id";

        super::CliCommand::reset();
        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["start", ID])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let start_config = super::NerdctlStartConfig {
            general_options: Vec::default(),
            container_id: ID.into(),
        };
        let res = NerdctlCli::nerdctl_start(start_config, "test_workload_name").await;
        assert_eq!(res, Err(SAMPLE_ERROR_MESSAGE.to_string()));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_created() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "created".to_owned(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("created"))));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_succeeded() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "exited".to_owned(),
                        exit_code: 0,
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::succeeded())));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "exited".to_owned(),
                        exit_code: 1,
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::failed("Exit code: '1'"))));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_running() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "running".to_owned(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_removing() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };
        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "removing".to_owned(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::stopping("removing"))));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_paused() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };
        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        let container_state = TestNerdctlContainerState {
            status: "paused".to_owned(),
            ..Default::default()
        };
        let expected_container_status = container_state.status.clone();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: container_state,
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(
            res,
            Ok(Some(ExecutionState::unknown(expected_container_status)))
        );
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_restarting() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };
        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "restarting".to_owned(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::starting("restarting"))));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_dead() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        NerdctlCli::reset_ps_cache().await;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };
        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "dead".to_owned(),
                        exit_code: 1,
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::failed("Exit code: '1'"))));
    }

    // [utest->swdd~containerd-state-getter-maps-state~1]
    // [utest->swdd~nerdctlcli-container-state-cache-refresh~1]
    #[tokio::test]
    async fn utest_list_states_by_id_unknown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "unknown".to_owned(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::unknown("unknown"))));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_nerdctl_error_retry_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
            .exec_returns(Err("simulated error".to_string()));
        // NerdctlCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect(NERDCTL_CMD, mock_cli_command.clone());
        super::CliCommand::new_expect(NERDCTL_CMD, mock_cli_command);

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_nerdctl_error_retry_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Err("simulated error".to_string())),
        );

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "running".to_owned(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
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
                container_states: Ok([(WORKLOAD_ID.into(), ExecutionState::failed("Some error"))]
                    .into_iter()
                    .collect()),
            }),
        });

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.to_owned(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "running".to_owned(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response_retry_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        let mock_cli_command = super::CliCommand::default()
            .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
            .exec_returns(Ok("non-json response from nerdctl".to_string()));
        // NerdctlCli retries the command when the command fails -> we have to mock the command twice.
        super::CliCommand::new_expect(NERDCTL_CMD, mock_cli_command.clone());
        super::CliCommand::new_expect(NERDCTL_CMD, mock_cli_command);

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse nerdctl ps output") ));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response_retry_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();
        *super::LAST_PS_RESULT.lock().await = None;

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok("non-json response from nerdctl".to_string())),
        );

        let container_id = TestNerdctlContainerId {
            id: WORKLOAD_ID.into(),
        };

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--no-trunc", "--format=json"])
                .exec_returns(Ok(container_id.clone().to_json())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["inspect", WORKLOAD_ID])
                .exec_returns(Ok([TestNerdctlContainerInfo {
                    id: container_id,
                    state: TestNerdctlContainerState {
                        status: "running".into(),
                        ..Default::default()
                    },
                }]
                .to_json())),
        );

        let res = NerdctlCli::list_states_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(Some(ExecutionState::running())));
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_stop_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["stop", WORKLOAD_ID])
                .exec_returns(Err("simulated error".to_string())),
        );

        assert_eq!(
            NerdctlCli::remove_workloads_by_id(WORKLOAD_ID).await,
            Err("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_ignore_failed_stop_on_non_existing_container() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["stop", WORKLOAD_ID])
                .exec_returns(Err(format!("1 errors:\nno such container: {WORKLOAD_ID}"))),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["rm", WORKLOAD_ID])
                .exec_returns(Ok(WORKLOAD_ID.to_owned())),
        );

        assert!(
            NerdctlCli::remove_workloads_by_id(WORKLOAD_ID)
                .await
                .is_ok(),
            "Expected to ignore the failed stop of non-existing container."
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_remove_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["stop", WORKLOAD_ID])
                .exec_returns(Ok(WORKLOAD_ID.to_owned())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["rm", WORKLOAD_ID])
                .exec_returns(Err("simulated error".to_string())),
        );

        assert_eq!(
            NerdctlCli::remove_workloads_by_id(WORKLOAD_ID).await,
            Err("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_ignore_failed_remove_on_non_existing_container() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["stop", WORKLOAD_ID])
                .exec_returns(Ok(WORKLOAD_ID.to_owned())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["rm", WORKLOAD_ID])
                .exec_returns(Err(format!("1 errors:\nno such container: {WORKLOAD_ID}"))),
        );

        assert!(
            NerdctlCli::remove_workloads_by_id(WORKLOAD_ID)
                .await
                .is_ok(),
            "Expected to ignore the failed remove of non-existing container."
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["stop", WORKLOAD_ID])
                .exec_returns(Ok("".to_owned())),
        );

        super::CliCommand::new_expect(
            NERDCTL_CMD,
            super::CliCommand::default()
                .expect_args(&["rm", WORKLOAD_ID])
                .exec_returns(Ok("".to_owned())),
        );

        let res = NerdctlCli::remove_workloads_by_id(WORKLOAD_ID).await;
        assert_eq!(res, Ok(()));
    }

    #[derive(Debug, Default, Clone, Serialize)]
    struct TestNerdctlContainerLabels {
        #[serde(rename = "Labels")]
        labels: HashMap<String, String>,
    }

    impl std::fmt::Display for TestNerdctlContainerLabels {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            let labels = self.labels.iter().fold(String::new(), |acc, (key, value)| {
                format!("{acc},{key}={value}")
            });
            let raw_labels =
                serde_json::to_string(&HashMap::from([("Labels".to_owned(), labels)])).unwrap();

            write!(f, "{raw_labels}",)
        }
    }

    #[derive(Debug, Default, Clone, Serialize)]
    struct TestNerdctlContainerState {
        #[serde(rename = "Status")]
        status: String,
        #[serde(rename = "ExitCode")]
        exit_code: u8,
    }

    #[derive(Debug, Default, Clone, Serialize)]
    struct TestNerdctlContainerId {
        #[serde(rename = "ID", alias = "Id")]
        id: String,
    }

    impl ToJson for TestNerdctlContainerId {
        fn to_json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }

    impl ToJson for [TestNerdctlContainerId] {
        fn to_json(&self) -> String {
            self.iter()
                .map(|id| id.to_json())
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    #[derive(Serialize, Clone, Debug, Default)]
    struct TestNerdctlContainerInfo {
        #[serde(rename = "State")]
        state: TestNerdctlContainerState,
        #[serde(flatten)]
        id: TestNerdctlContainerId,
    }

    impl ToJson for [TestNerdctlContainerInfo] {
        fn to_json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }

    trait ToJson {
        fn to_json(&self) -> String;
    }
}

use base64::Engine;
use common::objects::ExecutionState;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[cfg_attr(test, mockall_double::double)]
use super::cli_command::CliCommand;
use super::podman_runtime_config::PodmanRuntimeConfig;

const PODMAN_CMD: &str = "podman";
const API_PIPES_MOUNT_POINT: &str = "/run/ankaios/control_interface";

#[derive(Debug, PartialEq, Eq)]
pub enum ContainerState {
    Created,
    Exited(u8),
    Paused,
    Running,
    Unknown,
}

impl From<PodmanContainerInfo> for ContainerState {
    fn from(value: PodmanContainerInfo) -> Self {
        match value.state {
            PodmanContainerState::Created => ContainerState::Created,
            PodmanContainerState::Exited => ContainerState::Exited(value.exit_code),
            PodmanContainerState::Paused => ContainerState::Paused,
            PodmanContainerState::Running => ContainerState::Running,
            PodmanContainerState::Unknown => ContainerState::Unknown,
        }
    }
}

// [impl->swdd~podman-state-getter-maps-state~1]
impl From<PodmanContainerInfo> for ExecutionState {
    fn from(value: PodmanContainerInfo) -> Self {
        match value.state {
            PodmanContainerState::Created => ExecutionState::ExecPending,
            PodmanContainerState::Exited if value.exit_code == 0 => ExecutionState::ExecSucceeded,
            PodmanContainerState::Exited if value.exit_code != 0 => ExecutionState::ExecFailed,
            PodmanContainerState::Running => ExecutionState::ExecRunning,
            _ => ExecutionState::ExecUnknown,
        }
    }
}

pub struct PodmanCli {}

#[cfg_attr(test, automock)]
impl PodmanCli {
    pub async fn play_kube(
        general_options: &[String],
        play_options: &[String],
        kube_yml: &[u8],
    ) -> Result<Vec<String>, String> {
        let mut args: Vec<&str> = general_options.iter().map(|x| x as &str).collect();
        args.extend(["kube", "play", "--quiet"]);
        args.extend(play_options.iter().map(|x| x as &str));
        args.push("-");
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
                "--format={{.ID}}",
            ])
            .exec()
            .await?;
        Ok(output
            .split('\n')
            .map(|x| x.trim().into())
            .filter(|x: &String| !x.is_empty())
            .collect())
    }

    pub async fn list_workload_names_by_label(
        key: &str,
        value: &str,
    ) -> Result<Vec<String>, String> {
        log::debug!("Listing workload names for: '{}'='{}'", key, value,);
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

    pub async fn run_workload(
        mut workload_cfg: PodmanRuntimeConfig,
        workload_name: &str,
        agent: &str,
        control_interface_path: Option<PathBuf>,
    ) -> Result<String, String> {
        log::debug!("Creating the workload: '{}'", workload_cfg.image);

        let mut args = workload_cfg.general_options;

        args.push("run".into());
        args.push("--detach".into());

        // Setting "--name" flag is intentionally here before reading "command_options".
        // We want to give the user chance to set own container name.
        // In other words the user can overwrite our container name.
        // We store workload name as a label (and use them from there).
        // Therefore we do insist on container names in particular format.
        //
        // [impl->swdd~podman-create-workload-sets-optionally-container-name~1]
        args.append(&mut vec!["--name".into(), workload_name.to_string()]);

        args.append(&mut workload_cfg.command_options);

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

        // [impl->swdd~podman-create-workload-creates-labels~1]
        args.push(format!("--label=name={workload_name}"));
        args.push(format!("--label=agent={agent}"));
        args.push(workload_cfg.image);

        args.append(&mut workload_cfg.command_args);

        log::debug!("The args are: '{:?}'", args);
        let id = CliCommand::new(PODMAN_CMD)
            .args(&args.iter().map(|x| &**x).collect::<Vec<&str>>())
            .exec()
            .await?
            .trim()
            .to_string();
        Ok(id)
    }

    pub async fn list_states_by_id(workload_id: &str) -> Result<Vec<ExecutionState>, String> {
        let output = CliCommand::new(PODMAN_CMD)
            .args(&[
                "ps",
                "--all",
                "--filter",
                &format!("id={workload_id}"),
                "--format=json",
            ])
            .exec()
            .await?;

        let res: Vec<PodmanContainerInfo> = serde_json::from_str(&output)
            .map_err(|err| format!("Could not parse podman output:{}", err))?;

        Ok(res.into_iter().map(|x| x.into()).collect())
    }

    pub async fn list_states_from_pods(pods: &[String]) -> Result<Vec<ContainerState>, String> {
        let mut args = vec!["ps", "--all", "--format=json"];
        let filters: Vec<String> = pods.iter().map(|p| format!("--filter=pod={}", p)).collect();
        args.extend(filters.iter().map(|x| x as &str));

        let output = CliCommand::new(PODMAN_CMD).args(&args).exec().await?;

        let res: Vec<PodmanContainerInfo> = serde_json::from_str(&output)
            .map_err(|err| format!("Could not parse podman output:{}", err))?;

        Ok(res.into_iter().map(|x| x.into()).collect())
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
                &res.get(0)
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
        let args = vec!["rm", "--force", workload_id];
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PodmanContainerInfo {
    state: PodmanContainerState,
    exit_code: u8,
    labels: HashMap<String, String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum PodmanContainerState {
    Created,
    Exited,
    Paused,
    Running,
    #[serde(other)]
    Unknown,
}
//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
// [utest->swdd~podman-uses-podman-cli~1]
#[cfg(test)]
mod tests {
    use super::{ContainerState, PodmanCli, PodmanContainerInfo, PodmanContainerState};

    use crate::test_helper::MOCKALL_CONTEXT_SYNC;
    use common::objects::ExecutionState;
    use std::collections::HashMap;

    const SAMPLE_ERROR_MESSAGE: &str = "error message";

    #[test]
    fn utest_container_state_from_podman_container_info_created() {
        let container_state: ContainerState = PodmanContainerInfo {
            state: PodmanContainerState::Created,
            exit_code: 0,
            labels: Default::default(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Created));
    }

    #[test]
    fn utest_container_state_from_podman_container_info_exited() {
        let container_state: ContainerState = PodmanContainerInfo {
            state: PodmanContainerState::Exited,
            exit_code: 23,
            labels: Default::default(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Exited(23)));
    }

    #[test]
    fn utest_container_state_from_podman_container_info_paused() {
        let container_state: ContainerState = PodmanContainerInfo {
            state: PodmanContainerState::Paused,
            exit_code: 0,
            labels: Default::default(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Paused));
    }

    #[test]
    fn utest_container_state_from_podman_container_info_running() {
        let container_state: ContainerState = PodmanContainerInfo {
            state: PodmanContainerState::Running,
            exit_code: 0,
            labels: Default::default(),
        }
        .into();

        assert!(matches!(container_state, ContainerState::Running));
    }

    #[test]
    fn utest_container_state_from_podman_container_info_unkown() {
        let container_state: ContainerState = PodmanContainerInfo {
            state: PodmanContainerState::Unknown,
            exit_code: 0,
            labels: Default::default(),
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
                    "--format={{.ID}}",
                ])
                .exec_returns(Ok("result1\nresult2\n".into())),
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
                    "--format={{.ID}}",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = PodmanCli::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
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
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Running,
                    0,
                    "workload_name".into(),
                ))),
        );

        let res = PodmanCli::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec!["workload_name".into()]));
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

    // [utest->swdd~podman-create-workload-creates-labels~1]
    // [utest->swdd~podman-create-workload-sets-optionally-container-name~1]
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

        let workload_cfg = crate::podman::podman_runtime_config::PodmanRuntimeConfig {
            general_options: Vec::new(),
            command_options: Vec::new(),
            image: "alpine:latest".into(),
            command_args: Vec::new(),
        };
        let res =
            PodmanCli::run_workload(workload_cfg, "test_workload_name", "test_agent", None).await;
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

        let workload_cfg = crate::podman::podman_runtime_config::PodmanRuntimeConfig {
            general_options: Vec::new(),
            command_options: Vec::new(),
            image: "alpine:latest".into(),
            command_args: Vec::new(),
        };
        let res =
            PodmanCli::run_workload(workload_cfg, "test_workload_name", "test_agent", None).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    // [utest->swdd~podman-create-workload-sets-optionally-container-name~1]
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

        let workload_cfg = crate::podman::podman_runtime_config::PodmanRuntimeConfig {
            general_options: vec!["--remote".into()],
            command_options: vec!["--network=host".into(), "--name".into(), "myCont".into()],
            image: "alpine:latest".into(),
            command_args: vec!["sh".into()],
        };
        let res = PodmanCli::run_workload(
            workload_cfg,
            "test_workload_name",
            "test_agent",
            Some("/test/path".into()),
        )
        .await;
        assert_eq!(res, Ok("test_id".to_string()));
    }

    // [utest->swdd~podman-state-getter-maps-state~1]
    #[tokio::test]
    async fn utest_list_states_by_id_pending() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Created,
                    0,
                    "workload_name".into(),
                ))),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecPending]));
    }

    // [utest->swdd~podman-state-getter-maps-state~1]
    #[tokio::test]
    async fn utest_list_states_by_id_succeeded() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Exited,
                    0,
                    "workload_name".into(),
                ))),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecSucceeded]));
    }

    // [utest->swdd~podman-state-getter-maps-state~1]
    #[tokio::test]
    async fn utest_list_states_by_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Exited,
                    1,
                    "workload_name".into(),
                ))),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecFailed]));
    }

    // [utest->swdd~podman-state-getter-maps-state~1]
    #[tokio::test]
    async fn utest_list_states_by_id_running() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Running,
                    0,
                    "workload_name".into(),
                ))),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecRunning]));
    }

    // [utest->swdd~podman-state-getter-maps-state~1]
    #[tokio::test]
    async fn utest_list_states_by_id_unknown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Paused,
                    0,
                    "workload_name".into(),
                ))),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecUnknown]));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_podman_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Err("simulated error".to_string())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok("non-json response from podman".to_string())),
        );

        let res = PodmanCli::list_states_by_id("test_id").await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output") ));
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--format=json",
                    "--filter=pod=pod1",
                    "--filter=pod=pod2",
                    "--filter=pod=pod3",
                ])
                .exec_returns(Ok(concat!(
                    r#"[{"State": "running", "ExitCode": 0, "Labels": {}},"#,
                    r#" {"State": "exited", "ExitCode": 42, "Labels": {}},"#,
                    r#" {"State": "", "ExitCode": 0, "Labels": {}}]"#,
                )
                .into())),
        );

        let res =
            PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;
        assert!(
            matches!(res, Ok(states) if states == [ContainerState::Running, ContainerState::Exited(42), ContainerState::Unknown] )
        );
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_command_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--format=json",
                    "--filter=pod=pod1",
                    "--filter=pod=pod2",
                    "--filter=pod=pod3",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res =
            PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;

        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE ));
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_result_not_json() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--all",
                    "--format=json",
                    "--filter=pod=pod1",
                    "--filter=pod=pod2",
                    "--filter=pod=pod3",
                ])
                .exec_returns(Ok("{".into())),
        );

        let res =
            PodmanCli::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;

        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output:") ));
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
    async fn utest_remove_workloads_by_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;
        super::CliCommand::reset();

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["rm", "--force", "test_id"])
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
                .expect_args(&["rm", "--force", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        let res = PodmanCli::remove_workloads_by_id("test_id").await;
        assert_eq!(res, Ok(()));
    }

    fn generate_container_info_json(
        state: PodmanContainerState,
        exit_code: u8,
        workload_name: String,
    ) -> String {
        serde_json::to_string(&vec![PodmanContainerInfo {
            state,
            exit_code,
            labels: HashMap::from([("name".to_string(), workload_name)]),
        }])
        .unwrap()
    }
}

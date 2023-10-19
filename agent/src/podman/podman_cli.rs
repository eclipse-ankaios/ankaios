use common::objects::ExecutionState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg_attr(test, mockall_double::double)]
use super::cli_command::CliCommand;
use super::podman_runtime_config::PodmanRuntimeConfigCli;

const PODMAN_CMD: &str = "podman";
const API_PIPES_MOUNT_POINT: &str = "/run/ankaios/control_interface";

#[derive(Debug)]
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

pub async fn play_kube(kube_yml: &[u8]) -> Result<String, String> {
    let result = CliCommand::new(PODMAN_CMD)
        .args(&["kube", "play", "-"])
        .stdin(kube_yml)
        .exec()
        .await?;
    Ok(result)
}

pub async fn list_all_workloads_by_label(
    key: &str,
    value: &str,
    result_format: &str,
) -> Result<Vec<String>, String> {
    log::debug!(
        "Listing all workloads for: {}='{}' with format '{}'",
        key,
        value,
        result_format
    );
    let output = CliCommand::new(PODMAN_CMD)
        .args(&[
            "ps",
            "-a",
            "--filter",
            &format!("label={key}={value}"),
            &format!("--format={}", result_format),
        ])
        .exec()
        .await?;
    Ok(output
        .split('\n')
        .map(|x| x.trim().into())
        .filter(|x: &String| !x.is_empty())
        .collect())
}

pub async fn run_workload(
    workload_cfg: PodmanRuntimeConfigCli,
    workload_name: &str,
    agent: &str,
    control_interface_path: Option<PathBuf>,
) -> Result<String, String> {
    log::debug!("Creating the workload: '{}'", workload_cfg.image);

    let mut args = if let Some(opts) = workload_cfg.general_options {
        opts
    } else {
        Vec::new()
    };

    args.push("run".into());
    args.push("-d".into());

    // Setting "--name" flag is intentionally here before reading "command_options".
    // We want to give the user chance to set own container name.
    // In other words the user can overwrite our container name.
    // We store workload name as a label (an use them from there).
    // Therefore we do insist on container names in particular format.
    args.append(&mut vec!["--name".into(), workload_name.to_string()]);

    if let Some(mut x) = workload_cfg.command_options {
        args.append(&mut x);
    }

    if let Some(path) = control_interface_path {
        args.push(
            vec![
                "--mount=type=bind,source=",
                &path.to_string_lossy(),
                ",destination=",
                API_PIPES_MOUNT_POINT,
            ]
            .concat(),
        );
    }

    args.push(format!("--label=name={workload_name}"));
    args.push(format!("--label=agent={agent}"));
    args.push(workload_cfg.image);

    if let Some(mut x) = workload_cfg.command_args {
        args.append(&mut x);
    }

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

pub async fn list_states_by_label(key: &str, value: &str) -> Result<Vec<ContainerState>, String> {
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
        .map_err(|err| format!("Could not parse podman output:{}", err))?;

    Ok(res.into_iter().map(|x| x.into()).collect())
}

pub async fn list_pods_by_label(key: &str, value: &str) -> Result<Vec<String>, String> {
    let output = CliCommand::new(PODMAN_CMD)
        .args(&[
            "pod",
            "ps",
            "--filter",
            &format!("label={key}={value}"),
            "--format={{.Id}}",
        ])
        .exec()
        .await?;
    Ok(output
        .split('\n')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect())
}

pub async fn stop_pods(pods: &[String]) -> Result<(), String> {
    let mut args = vec!["pod", "stop", "--"];
    args.extend(pods.iter().map(|x| x.as_str()));

    CliCommand::new(PODMAN_CMD).args(&args).exec().await?;
    Ok(())
}

pub async fn rm_pods(pods: &[String]) -> Result<(), String> {
    let mut args = vec!["pod", "rm", "--"];
    args.extend(pods.iter().map(|x| x.as_str()));

    CliCommand::new(PODMAN_CMD).args(&args).exec().await?;
    Ok(())
}

pub async fn list_volumes_by_label(key: &str, value: &str) -> Result<Vec<String>, String> {
    let output = CliCommand::new(PODMAN_CMD)
        .args(&[
            "volume",
            "ls",
            "--filter",
            &format!("label={key}={value}"),
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

pub async fn remove_workloads_by_id(workload_id: &str) -> Result<(), String> {
    let args = vec!["rm", "-f", workload_id];
    CliCommand::new(PODMAN_CMD).args(&args).exec().await?;
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PodmanContainerInfo {
    state: PodmanContainerState,
    exit_code: u8,
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
#[cfg(test)]
mod tests {
    use common::objects::ExecutionState;

    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

    use super::{PodmanContainerInfo, PodmanContainerState};

    const SAMPLE_ERROR_MESSAGE: &str = "error message";

    #[tokio::test]
    async fn utest_play_kube_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "play", "-"])
                .expect_stdin(sample_input)
                .exec_returns(Ok("".into())),
        );

        let res = super::play_kube(sample_input.as_bytes()).await;
        assert!(matches!(res, Ok(..)));
    }

    #[tokio::test]
    async fn utest_play_kube_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "play", "-"])
                .expect_stdin(sample_input)
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = super::play_kube(sample_input.as_bytes()).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_list_all_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "-a",
                    "--filter",
                    "label=name=test_agent",
                    "--format={{.ID}}",
                ])
                .exec_returns(Ok("result1\nresult2\n".into())),
        );

        let res = super::list_all_workloads_by_label("name", "test_agent", r"{{.ID}}").await;
        assert!(matches!(res, Ok(res) if res == vec!["result1", "result2"]));
    }

    #[tokio::test]
    async fn utest_list_all_workloads_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "-a",
                    "--filter",
                    "label=name=test_agent",
                    "--format={{.ID}}",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = super::list_all_workloads_by_label("name", "test_agent", r"{{.ID}}").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_run_container_success_no_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "run",
                    "-d",
                    "--name",
                    "test_workload_name",
                    "--label=name=test_workload_name",
                    "--label=agent=test_agent",
                    "alpine:latest",
                ])
                .exec_returns(Ok("test_id".to_string())),
        );

        let workload_cfg = crate::podman::podman_runtime_config::PodmanRuntimeConfigCli {
            general_options: None,
            command_options: None,
            image: "alpine:latest".into(),
            command_args: None,
        };
        let res = super::run_workload(workload_cfg, "test_workload_name", "test_agent", None).await;
        assert_eq!(res, Ok("test_id".to_string()));
    }

    #[tokio::test]
    async fn utest_run_container_fail_no_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "run",
                    "-d",
                    "--name",
                    "test_workload_name",
                    "--label=name=test_workload_name",
                    "--label=agent=test_agent",
                    "alpine:latest",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let workload_cfg = crate::podman::podman_runtime_config::PodmanRuntimeConfigCli {
            general_options: None,
            command_options: None,
            image: "alpine:latest".into(),
            command_args: None,
        };
        let res = super::run_workload(workload_cfg, "test_workload_name", "test_agent", None).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_run_container_success_with_options() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "--remote",
                    "run",
                    "-d",
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

        let workload_cfg = crate::podman::podman_runtime_config::PodmanRuntimeConfigCli {
            general_options: Some(vec!["--remote".into()]),
            command_options: Some(vec![
                "--network=host".into(),
                "--name".into(),
                "myCont".into(),
            ]),
            image: "alpine:latest".into(),
            command_args: Some(vec!["sh".into()]),
        };
        let res = super::run_workload(
            workload_cfg,
            "test_workload_name",
            "test_agent",
            Some("/test/path".into()),
        )
        .await;
        assert_eq!(res, Ok("test_id".to_string()));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_pending() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Created,
                    0,
                ))),
        );

        let res = super::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecPending]));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_succeeded() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Exited,
                    0,
                ))),
        );

        let res = super::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecSucceeded]));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Exited,
                    1,
                ))),
        );

        let res = super::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecFailed]));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_running() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Running,
                    0,
                ))),
        );

        let res = super::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecRunning]));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_unknown() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok(generate_container_info_json(
                    PodmanContainerState::Paused,
                    0,
                ))),
        );

        let res = super::list_states_by_id("test_id").await;
        assert_eq!(res, Ok(vec![ExecutionState::ExecUnknown]));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_podman_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Err("simulated error".to_string())),
        );

        let res = super::list_states_by_id("test_id").await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_states_by_id_broken_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--all", "--filter", "id=test_id", "--format=json"])
                .exec_returns(Ok("non-json response from podman".to_string())),
        );

        let res = super::list_states_by_id("test_id").await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output") ));
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_failed() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["rm", "-f", "test_id"])
                .exec_returns(Err("simulated error".to_string())),
        );

        assert_eq!(
            super::remove_workloads_by_id("test_id").await,
            Err("simulated error".to_string())
        );
    }

    #[tokio::test]
    async fn utest_remove_workloads_by_id_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["rm", "-f", "test_id"])
                .exec_returns(Ok("".to_string())),
        );

        let res = super::remove_workloads_by_id("test_id").await;
        assert_eq!(res, Ok(()));
    }

    fn generate_container_info_json(state: PodmanContainerState, exit_code: u8) -> String {
        serde_json::to_string(&vec![PodmanContainerInfo { state, exit_code }]).unwrap()
    }
}

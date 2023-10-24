use base64::Engine;
use common::objects::ExecutionState;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[cfg_attr(test, mockall_double::double)]
use super::cli_command::CliCommand;
use super::podman_runtime_config::PodmanRuntimeConfigCli;

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

pub async fn play_kube(kube_yml: &[u8]) -> Result<Vec<String>, String> {
    let result = CliCommand::new(PODMAN_CMD)
        .args(&["kube", "play", "--quiet", "-"])
        .stdin(kube_yml)
        .exec()
        .await?;
    Ok(parse_pods_from_output(result))
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

pub async fn down_kube(kube_yml: &[u8]) -> Result<(), String> {
    CliCommand::new(PODMAN_CMD)
        .args(&["kube", "down", "--force", "-"])
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
            "-a",
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

pub async fn list_workload_names_by_label(key: &str, value: &str) -> Result<Vec<String>, String> {
    log::debug!("Listing workload names for: {}='{}'", key, value,);
    let output = CliCommand::new(PODMAN_CMD)
        .args(&[
            "ps",
            "-a",
            "--filter",
            &format!("label={key}={value}"),
            "--format=json",
        ])
        .exec()
        .await?;

    let res: Vec<PodmanContainerInfo> = serde_json::from_str(&output)
        .map_err(|err| format!("Could not parse podman output: {}", err))?;

    let mut names: Vec<String> = Vec::new();
    for mut podman_info in res {
        if let Some(name_val) = podman_info.labels.get_mut("name") {
            names.push(name_val.to_string());
        }
    }
    Ok(names)
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
            [
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

pub async fn list_states_from_pods(pods: &[String]) -> Result<Vec<ContainerState>, String> {
    let mut args = vec!["ps", "--all", "--format=json"];
    let filters: Vec<String> = pods.iter().map(|p| format!("--filter=pod={}", p)).collect();
    args.extend(filters.iter().map(|x| x as &str));

    let output = CliCommand::new(PODMAN_CMD).args(&args).exec().await?;

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

pub async fn stop_pods(pods: &[String]) -> Result<(), String> {
    let mut args = vec!["pod", "stop", "--"];
    args.extend(pods.iter().map(|x| x.as_str()));

    CliCommand::new(PODMAN_CMD).args(&args).exec().await?;
    Ok(())
}

pub async fn store_data_as_volume(volume_name: &str, data: &str) -> Result<(), String> {
    remove_volume(volume_name).await?;

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

    let res: Vec<Volume> = serde_json::from_str(&result).unwrap();
    let res = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&res[0].labels.data)
        .map_err(|err| format!("Could not base64 decoded volume's data label: {}", err))?;
    let res = String::from_utf8(res)
        .map_err(|err| format!("Could not decode data stored in volume: {:?}", err))?;

    Ok(res)
}

pub async fn remove_volume(volume_name: &str) -> Result<(), String> {
    let _ = CliCommand::new(PODMAN_CMD)
        .args(&["volume", "rm", volume_name])
        .exec()
        .await;
    Ok(())
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
#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use common::objects::ExecutionState;

    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

    use super::{ContainerState, PodmanContainerInfo, PodmanContainerState};

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

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "play", "--quiet", "-"])
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

        let res = super::play_kube(sample_input.as_bytes()).await;
        assert!(
            matches!(res, Ok(pods) if pods == ["3".to_string(), "5".to_string(), "6".to_string()])
        );
    }

    #[tokio::test]
    async fn utest_play_kube_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "play", "--quiet", "-"])
                .expect_stdin(sample_input)
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = super::play_kube(sample_input.as_bytes()).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_down_kube_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "down", "--force", "-"])
                .expect_stdin(sample_input)
                .exec_returns(Ok("".into())),
        );

        let res = super::down_kube(sample_input.as_bytes()).await;
        assert!(matches!(res, Ok(..)));
    }

    #[tokio::test]
    async fn utest_down_kube_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sample_input = "sample input";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["kube", "down", "--force", "-"])
                .expect_stdin(sample_input)
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = super::down_kube(sample_input.as_bytes()).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_success() {
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

        let res = super::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Ok(res) if res == vec!["result1", "result2"]));
    }

    #[tokio::test]
    async fn utest_list_workload_ids_fail() {
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

        let res = super::list_workload_ids_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_list_workload_names_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "-a",
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

        let res = super::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Ok(vec!["workload_name".into()]));
    }

    #[tokio::test]
    async fn utest_list_workload_names_podman_error() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "-a",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Err("simulated error".to_string())),
        );

        let res = super::list_workload_names_by_label("name", "test_agent").await;
        assert_eq!(res, Err("simulated error".to_string()));
    }

    #[tokio::test]
    async fn utest_list_workload_names_broken_response() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "-a",
                    "--filter",
                    "label=name=test_agent",
                    "--format=json",
                ])
                .exec_returns(Ok("non-json response from podman".to_string())),
        );

        let res = super::list_workload_names_by_label("name", "test_agent").await;
        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output") ));
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
                    "workload_name".into(),
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
                    "workload_name".into(),
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
                    "workload_name".into(),
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
                    "workload_name".into(),
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
                    "workload_name".into(),
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
    async fn utest_list_states_from_pods_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

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
            super::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;
        assert!(
            matches!(res, Ok(states) if states == [ContainerState::Running, ContainerState::Exited(42), ContainerState::Unknown] )
        );
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_command_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

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
            super::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;

        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE ));
    }

    #[tokio::test]
    async fn utest_list_states_from_pods_result_not_json() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

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
            super::list_states_from_pods(&["pod1".into(), "pod2".into(), "pod3".into()]).await;

        assert!(matches!(res, Err(msg) if msg.starts_with("Could not parse podman output:") ));
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

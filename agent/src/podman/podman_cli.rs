use std::process::Stdio;
use tokio::io::AsyncWriteExt;

use tokio::process::Command;

use super::cli_command::CliCommand;

const PODMAN_CMD: &str = "podman";

pub async fn play_kube(kube_yml: &str) -> Result<String, String> {
    let result = CliCommand::new(PODMAN_CMD)
        .args(&["kube", "play", "-"])
        .stdin(kube_yml.as_bytes())
        .exec()
        .await?;
    Ok(result)
}

pub async fn list_workloads(regex: &str) -> Result<Vec<String>, String> {
    let output = CliCommand::new(PODMAN_CMD)
        .args(&[
            "ps",
            "--filter",
            &format!("name={}", regex),
            "--format={{.Names}}",
        ])
        .exec()
        .await?;
    Ok(output
        .split('\n')
        .map(|x| x.trim().into())
        .filter(|x: &String| !x.is_empty())
        .collect())
}

pub async fn has_image(image_name: &str) -> Result<bool, String> {
    let output = CliCommand::new(PODMAN_CMD)
        .args(&[
            "images",
            "--filter",
            &format!("reference={}", image_name),
            "--format={{.Repository}}:{{.Tag}}",
        ])
        .exec()
        .await?;
    Ok(output
        .split('\n')
        .map(|x| x.trim().into())
        .any(|x: String| x == *image_name))
}

pub async fn pull_image(image: &String) -> Result<(), String> {
    log::debug!("Pulling the image: {}", image);
    let result = CliCommand::new(PODMAN_CMD)
        .args(&["pull", image])
        .exec()
        .await?;
    Ok(())
}

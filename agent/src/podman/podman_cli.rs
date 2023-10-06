use std::process::Stdio;
use tokio::io::AsyncWriteExt;

use tokio::process::Command;

const PODMAN_CMD: &str = "podman";

pub async fn play_kube(kube_yml: &str) -> Result<String, String> {
    let result = PodmanCliCommand::new()
        .args(vec!["kube".into(), "play".into(), "-".into()])
        .stdin(kube_yml.as_bytes())
        .exec()
        .await?;
    Ok(result)
}

pub async fn list_workloads(regex: &str) -> Result<Vec<String>, String> {
    let output = PodmanCliCommand::new()
        .args(vec![
            "ps".into(),
            "--filter".into(),
            format!("name={}", regex),
            "--format={{.Names}}".into(),
        ])
        .exec()
        .await?;
    Ok(output
        .split('\n')
        .map(|x| x.trim().into())
        .filter(|x: &String| !x.is_empty())
        .collect())
}

struct PodmanCliCommand<'a> {
    command: Command,
    stdin: Option<&'a [u8]>,
}

impl<'a> PodmanCliCommand<'a> {
    fn new() -> Self {
        let mut command = Command::new(PODMAN_CMD);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());
        Self {
            command,
            stdin: None,
        }
    }

    fn args(&mut self, args: Vec<String>) -> &mut Self {
        self.command.args(args);
        self
    }

    fn stdin(&mut self, stdin: &'a [u8]) -> &mut Self {
        self.stdin = Some(stdin);
        self
    }

    async fn exec(&mut self) -> Result<String, String> {
        let mut child = self
            .command
            .spawn()
            .map_err(|err| format!("Could not execute podman command: {}", err))?;

        if let Some(stdin) = self.stdin {
            child
                .stdin
                .as_mut()
                .ok_or_else(|| "Could not access podman stdin".to_string())?
                .write_all(stdin)
                .await
                .map_err(|err| format!("Could write data to podman command: {}", err))?;
        }
        let result = child.wait_with_output().await.unwrap();
        if result.status.success() {
            String::from_utf8(result.stdout)
                .map_err(|err| format!("Could not decode podman output as UTF8: {}", err))
        } else {
            let stderr = String::from_utf8(result.stderr)
                .unwrap_or_else(|err| format!("Could not decode podman stderr as UTF8: {}", err));
            Err(format!("Execution of podman failed: {}", stderr))
        }
    }
}

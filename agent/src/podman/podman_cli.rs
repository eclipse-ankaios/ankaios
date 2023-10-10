use std::process::Stdio;
use tokio::io::AsyncWriteExt;

use tokio::process::Command;

#[cfg_attr(test, mockall_double::double)]
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

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use crate::test_helper::MOCKALL_CONTEXT_SYNC;

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

        let res = super::play_kube(sample_input).await;
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

        let res = super::play_kube(sample_input).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_list_workloads_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        let sample_regex = "sample regex";

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "ps",
                    "--filter",
                    &format!("name={}", sample_regex),
                    "--format={{.Names}}",
                ])
                .exec_returns(Ok("result1\nresult2\n".into())),
        );

        let res = super::list_workloads(sample_regex).await;
        assert!(matches!(res, Ok(res) if res == vec!["result1", "result2"]));
    }

    #[tokio::test]
    async fn utest_list_workloads_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["ps", "--filter", "name=sample regex", "--format={{.Names}}"])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = super::list_workloads("sample regex").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_has_image_true() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "images",
                    "--filter",
                    "reference=sample_image:latest",
                    "--format={{.Repository}}:{{.Tag}}",
                ])
                .exec_returns(Ok(
                    "some_image:1\nsample_image:latest\nanother_image:2\n".into()
                )),
        );

        let res = super::has_image("sample_image:latest").await;
        assert!(matches!(res, Ok(res) if res));
    }

    #[tokio::test]
    async fn utest_has_image_false() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "images",
                    "--filter",
                    "reference=sample_image:latest",
                    "--format={{.Repository}}:{{.Tag}}",
                ])
                .exec_returns(Ok("some_image:1\nanother_image:2\n".into())),
        );

        let res = super::has_image("sample_image:latest").await;
        assert!(matches!(res, Ok(res) if !res));
    }

    #[tokio::test]
    async fn utest_has_image_fails() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&[
                    "images",
                    "--filter",
                    "reference=sample_image:latest",
                    "--format={{.Repository}}:{{.Tag}}",
                ])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = super::has_image("sample_image:latest").await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }

    #[tokio::test]
    async fn utest_pull_image_success() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["pull", "sample_image:latest"])
                .exec_returns(Ok("".into())),
        );

        let res = super::pull_image(&"sample_image:latest".into()).await;
        assert!(matches!(res, Ok(..)));
    }

    #[tokio::test]
    async fn utest_pull_image_fail() {
        let _guard = MOCKALL_CONTEXT_SYNC.get_lock_async().await;

        super::CliCommand::new_expect(
            "podman",
            super::CliCommand::default()
                .expect_args(&["pull", "sample_image:latest"])
                .exec_returns(Err(SAMPLE_ERROR_MESSAGE.into())),
        );

        let res = super::pull_image(&"sample_image:latest".into()).await;
        assert!(matches!(res, Err(msg) if msg == SAMPLE_ERROR_MESSAGE));
    }
}

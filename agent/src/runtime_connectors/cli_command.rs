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

use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[cfg(test)]
pub use tests::MockCliCommand;

pub struct CliCommand<'a> {
    command: Command,
    stdin: Option<&'a [u8]>,
    program: String,
    args: Vec<String>,
}

impl<'a> CliCommand<'a> {
    pub fn new(program: &str) -> Self {
        let mut command = Command::new(program);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());
        Self {
            command,
            stdin: None,
            program: program.to_owned(),
            args: Vec::new(),
        }
    }

    pub fn args(&mut self, args: &[&str]) -> &mut Self {
        self.command.args(args);
        self.args = args.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn stdin(&mut self, stdin: &'a [u8]) -> &mut Self {
        self.stdin = Some(stdin);
        self
    }

    pub async fn exec(&mut self) -> Result<String, String> {
        let mut child = self.command.spawn().map_err(|err| {
            format!(
                "Error: '{}'. Could not spawn command '{:?}'.",
                err, self.command
            )
        })?;

        if let Some(stdin) = self.stdin {
            child
                .stdin
                .as_mut()
                .ok_or_else(|| "Could not access commands stdin".to_string())?
                .write_all(stdin)
                .await
                .map_err(|err| format!("Could write stdin data to command: '{}'", err))?;
        }
        let result = child.wait_with_output().await.unwrap();
        if result.status.success() {
            String::from_utf8(result.stdout)
                .map_err(|err| format!("Could not decode command's output as UTF8: '{}'", err))
        } else {
            let stderr = String::from_utf8(result.stderr).unwrap_or_else(|err| {
                format!("Could not decode command's stderr as UTF8: '{}'", err)
            });

            let args_with_quotes = self.get_quoted_args(); // quoted args for easy debugging of the user

            Err(format!(
                "{}. Execution of '{} {}'",
                stderr.trim(),
                self.program,
                args_with_quotes,
            ))
        }
    }

    fn get_quoted_args(&self) -> String {
        self.args
            .iter()
            .map(|arg| format!("\"{}\"", arg))
            .collect::<Vec<String>>()
            .join(" ")
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
    use std::{
        collections::{HashMap, VecDeque},
        sync::Mutex,
    };

    use super::CliCommand;

    #[tokio::test]
    async fn utest_cli_command_simple_output() {
        let result = CliCommand::new("echo")
            .args(&["Hello", "World"])
            .exec()
            .await;
        assert!(matches!(result, Ok(x) if x.eq("Hello World\n")))
    }

    #[tokio::test]
    async fn utest_cli_command_simple_output_multiple_args() {
        let result = CliCommand::new("echo")
            .args(&["Hello"])
            .args(&["World"])
            .exec()
            .await;
        assert!(matches!(result, Ok(x) if x.eq("Hello World\n")));
    }

    #[tokio::test]
    async fn utest_cli_command_fail_on_not_existing_command() {
        let result = CliCommand::new("non_existing_command").exec().await;
        assert!(matches!(result, Err(x) if x.contains("Could not spawn command")));
    }

    #[tokio::test]
    async fn utest_cli_command_simple_input_output() {
        let result = CliCommand::new("tr")
            .args(&["[:lower:]", "[:upper:]"])
            .stdin("Hello World".as_bytes())
            .exec()
            .await;
        assert!(matches!(result, Ok(x) if x.eq("HELLO WORLD")));
    }

    #[tokio::test]
    async fn utest_cli_command_only_forward_stdout() {
        let result = CliCommand::new("bash")
            .args(&["-c", "echo output;echo error >&2"])
            .exec()
            .await;

        assert!(matches!(result, Ok(x) if x== "output\n"));
    }

    #[tokio::test]
    async fn utest_cli_command_on_fail_only_forward_stderr() {
        let result = CliCommand::new("bash")
            .args(&["-c", "echo output;echo error >&2; false"])
            .exec()
            .await;

        assert_eq!(
            result,
            Err(
                "error. Execution of 'bash \"-c\" \"echo output;echo error >&2; false\"'"
                    .to_string()
            )
        );
    }

    #[tokio::test]
    async fn utest_cli_command_stdout_not_utf8() {
        let result = CliCommand::new("bash")
            .args(&["-c", "echo wAA= | base64 -d"])
            .exec()
            .await;
        assert!(
            matches!(result, Err(x) if x.starts_with("Could not decode command's output as UTF8"))
        );
    }

    #[tokio::test]
    async fn utest_cli_command_stderr_not_utf8() {
        let result = CliCommand::new("bash")
            .args(&["-c", "echo wAA= | base64 -d >&2; false"])
            .exec()
            .await;

        assert!(
            matches!(result, Err(x) if x.contains("Could not decode command's stderr as UTF8"))
        );
    }

    lazy_static::lazy_static! {
        static ref MOCK_CLI_COMMANDS: Mutex<HashMap<String, VecDeque<MockCliCommand>>> =
            Default::default();
    }

    #[derive(Default, Clone)]
    pub struct MockCliCommand {
        args: VecDeque<String>,
        stdin: Option<String>,
        result: Option<Result<String, String>>,
    }

    impl MockCliCommand {
        pub fn reset() {
            *MOCK_CLI_COMMANDS.lock().unwrap() = HashMap::new();
        }

        pub fn new_expect(program: &str, mock_cli_command: MockCliCommand) {
            MOCK_CLI_COMMANDS
                .lock()
                .unwrap()
                .entry(program.into())
                .or_default()
                .push_back(mock_cli_command);
        }

        pub fn expect_args(mut self, args: &[&str]) -> Self {
            self.args = args.iter().map(|s| s.to_string()).collect();
            self
        }

        pub fn expect_stdin(mut self, stdin: &str) -> Self {
            self.stdin = Some(stdin.into());
            self
        }

        pub fn exec_returns(mut self, result: Result<String, String>) -> Self {
            self.result = Some(result);
            self
        }

        pub fn new(program: &str) -> Self {
            MOCK_CLI_COMMANDS
                .lock()
                .unwrap()
                .get_mut(program)
                .unwrap()
                .pop_front()
                .unwrap()
        }

        pub fn args(&mut self, args: &[&str]) -> &mut Self {
            for actual in args {
                let expected = self.args.pop_front().unwrap();
                assert_eq!(actual, &expected)
            }

            self
        }

        pub fn stdin(&mut self, stdin: &[u8]) -> &mut Self {
            let expected = self.stdin.take().unwrap();
            assert_eq!(stdin, expected.as_bytes());
            self
        }

        pub async fn exec(&mut self) -> Result<String, String> {
            assert!(self.args.is_empty());
            assert_eq!(self.stdin, None);

            self.result.take().unwrap()
        }
    }
}

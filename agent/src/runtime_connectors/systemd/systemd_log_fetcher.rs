// Copyright (c) 2024 Elektrobit Automotive GmbH
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

use super::super::log_fetcher::{GetOutputStreams, StreamTrait};
use crate::runtime_connectors::{runtime_connector::LogRequestOptions, RuntimeError, RuntimeWorkloadId};

use std::process::Stdio;
#[cfg(test)]
use tests::{MockChild as Child, MockCommand as Command};
#[cfg(not(test))]
use tokio::process::{Child, Command};

#[derive(Debug)]
pub struct SystemdLogFetcher {
    child: Option<Child>,
    #[cfg(test)]
    pub stdout: Option<Box<dyn StreamTrait>>,
    #[cfg(test)]
    pub stderr: Option<Box<dyn StreamTrait>>,
}

impl SystemdLogFetcher {
    pub fn new(
        workload_id: &RuntimeWorkloadId,
        options: &LogRequestOptions,
    ) -> Result<Self, RuntimeError> {
        let unit_name = workload_id.as_ref();
        let mut args = Vec::with_capacity(9);
        args.push("-u");
        args.push(unit_name);

        if options.follow {
            args.push("-f");
        }
        if let Some(since) = &options.since {
            args.push("--since");
            args.push(since);
        }
        if let Some(until) = &options.until {
            args.push("--until");
            args.push(until);
        }
        let tail_string;
        if let Some(tail) = options.tail {
            tail_string = tail.to_string();
            args.push("-n");
            args.push(&tail_string);
        }

        let cmd = Command::new("journalctl")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let cmd = match cmd {
            Ok(cmd) => Some(cmd),
            Err(err) => {
                log::warn!("Cannot collect logs for '{}': '{}'", workload_id, err);
                return Err(RuntimeError::CollectLog(format!(
                    "Failed to spawn journalctl: {}",
                    err
                )));
            }
        };

        #[cfg(not(test))]
        return Ok(Self { child: cmd });
        #[cfg(test)]
        Ok(Self {
            child: cmd,
            stdout: None,
            stderr: None,
        })
    }

}

impl Drop for SystemdLogFetcher {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child
            && let Err(err) = child.start_kill()
        {
            log::warn!("Could not stop log collection: '{}'", err);
        }
    }
}

impl GetOutputStreams for SystemdLogFetcher {
    type OutputStream = Box<dyn StreamTrait>;
    type ErrStream = Box<dyn StreamTrait>;

    fn get_output_streams(&mut self) -> (Option<Self::OutputStream>, Option<Self::ErrStream>) {
        #[cfg(not(test))]
        {
            if let Some(child) = &mut self.child {
                return (
                    child
                        .stdout
                        .take()
                        .map(|stdout| Box::new(stdout) as Box<dyn StreamTrait>),
                    child
                        .stderr
                        .take()
                        .map(|stderr| Box::new(stderr) as Box<dyn StreamTrait>),
                );
            }
            (None, None)
        }
        #[cfg(test)]
        return (self.stdout.take(), self.stderr.take());
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
    use super::SystemdLogFetcher;
    use crate::runtime_connectors::{
        log_fetcher::GetOutputStreams, LogRequestOptions, RuntimeWorkloadId,
    };

    use std::{process::Stdio, sync::Mutex};
    use tokio::io::Empty;

    static CAN_SPAWN: Mutex<bool> = Mutex::new(true);
    static CAN_KILL: Mutex<bool> = Mutex::new(true);
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static WAS_KILLED: Mutex<bool> = Mutex::new(false);

    #[derive(Debug)]
    pub struct MockChild {
        pub _stdout: Option<Empty>,
        cmd: String,
        args: Vec<String>,
        stdout_option: Option<Stdio>,
        stderr_option: Option<Stdio>,
    }
    impl MockChild {
        pub(crate) fn start_kill(&self) -> Result<(), String> {
            *WAS_KILLED.lock().unwrap() = true;
            if *CAN_KILL.lock().unwrap() {
                Ok(())
            } else {
                Err("MockChild: Could not kill child".to_string())
            }
        }
    }

    #[derive(Default)]
    pub struct MockCommand {
        cmd: String,
        args: Vec<String>,
        stdout: Option<Stdio>,
        stderr: Option<Stdio>,
    }

    impl MockCommand {
        pub fn new(cmd: &str) -> Self {
            Self {
                cmd: cmd.to_owned(),
                ..Default::default()
            }
        }

        pub(crate) fn args(&mut self, args: Vec<&str>) -> &mut Self {
            self.args = args.into_iter().map(ToOwned::to_owned).collect();
            self
        }

        pub(crate) fn stdout(&mut self, piped: Stdio) -> &mut Self {
            self.stdout = Some(piped);
            self
        }

        pub(crate) fn stderr(&mut self, piped: Stdio) -> &mut Self {
            self.stderr = Some(piped);
            self
        }

        pub(crate) fn spawn(&mut self) -> Result<MockChild, String> {
            if *CAN_SPAWN.lock().unwrap() {
                Ok(MockChild {
                    _stdout: None,
                    cmd: self.cmd.clone(),
                    args: self.args.clone(),
                    stdout_option: self.stdout.take(),
                    stderr_option: self.stderr.take(),
                })
            } else {
                Err("MockCommand: Could not spawn child".to_string())
            }
        }
    }

    #[test]
    fn utest_new_with_no_parameters() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CAN_SPAWN.lock().unwrap() = true;
        let mut log_fetcher = SystemdLogFetcher::new(
            &RuntimeWorkloadId::from("test.service"),
            &LogRequestOptions {
                follow: false,
                tail: None,
                since: None,
                until: None,
            },
        )
        .unwrap();

        assert!(matches!(
            &log_fetcher.child,
            Some(MockChild {
                _stdout: _,
                cmd,
                args,
                stdout_option: Some(_),
                stderr_option: Some(_)

            }) if cmd == "journalctl" && *args == vec!["-u".to_string(), "test.service".to_string()]
        ));
        let (child_stdout, child_stderr) = log_fetcher.get_output_streams();
        assert!(child_stdout.is_none());
        assert!(child_stderr.is_none());
    }

    #[test]
    fn utest_new_with_with_parameters() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CAN_SPAWN.lock().unwrap() = true;
        let mut log_fetcher = SystemdLogFetcher::new(
            &RuntimeWorkloadId::from("nginx.service"),
            &LogRequestOptions {
                follow: true,
                tail: Some(10),
                since: Some("since".to_string()),
                until: Some("until".to_string()),
            },
        )
        .unwrap();

        assert!(matches!(
            &log_fetcher.child,
            Some(MockChild {
                _stdout: _,
                cmd,
                args,
                stdout_option: Some(_),
                stderr_option: Some(_),
            }) if cmd == "journalctl" && *args == vec!["-u".to_string(), "nginx.service".to_string(), "-f".to_string(), "--since".to_string(), "since".to_string(), "--until".to_string(), "until".to_string(), "-n".to_string(), "10".to_string()]
        ));
        let (child_stdout, child_stderr) = log_fetcher.get_output_streams();
        assert!(child_stdout.is_none());
        assert!(child_stderr.is_none());
    }

    #[test]
    fn utest_new_spawn_fails() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CAN_SPAWN.lock().unwrap() = false;
        let result = SystemdLogFetcher::new(
            &RuntimeWorkloadId::from("test.service"),
            &LogRequestOptions {
                follow: false,
                tail: None,
                since: None,
                until: None,
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn utest_dropped_child_kills_cmd() {
        let _guard = TEST_LOCK.lock().unwrap();
        *WAS_KILLED.lock().unwrap() = false;
        *CAN_SPAWN.lock().unwrap() = true;

        *CAN_KILL.lock().unwrap() = true;
        let log_fetcher = SystemdLogFetcher::new(
            &RuntimeWorkloadId::from("test.service"),
            &LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            },
        )
        .unwrap();

        assert!(!*WAS_KILLED.lock().unwrap());
        drop(log_fetcher);
        assert!(*WAS_KILLED.lock().unwrap());
    }

    #[test]
    fn utest_dropped_child_handles_kills_error() {
        let _guard = TEST_LOCK.lock().unwrap();
        *WAS_KILLED.lock().unwrap() = false;
        *CAN_SPAWN.lock().unwrap() = true;
        *CAN_KILL.lock().unwrap() = false;
        let log_fetcher = SystemdLogFetcher::new(
            &RuntimeWorkloadId::from("test.service"),
            &LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            },
        )
        .unwrap();

        assert!(!*WAS_KILLED.lock().unwrap());
        drop(log_fetcher);
        assert!(*WAS_KILLED.lock().unwrap());
    }
}

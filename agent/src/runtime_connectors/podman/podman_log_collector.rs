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

use std::pin::Pin;
use std::process::Stdio;
#[cfg(test)]
use tests::MockChild as Child;
#[cfg(test)]
use tests::MockCommand as Command;
#[cfg(not(test))]
use tokio::process::{Child, Command};

use tokio::io::AsyncRead;

use crate::runtime_connectors::runtime_connector::LogRequestOptions;

use super::PodmanWorkloadId;

#[derive(Debug)]
pub struct PodmanLogCollector {
    child: Option<Child>,
}

impl PodmanLogCollector {
    pub fn new(workload_id: &PodmanWorkloadId, options: &LogRequestOptions) -> Self {
        let mut args = Vec::with_capacity(8);
        args.push("logs");
        if options.follow {
            args.push("-f")
        }
        if let Some(since) = &options.since {
            args.push("--since");
            args.push(since);
        }
        if let Some(until) = &options.until {
            args.push("--until");
            args.push(until);
        }
        let mut _tail = String::new();
        if let Some(tail2) = options.tail {
            _tail = tail2.to_string();
            args.push("--tail");
            args.push(_tail.as_str());
        }
        args.push(&workload_id.id);
        let cmd = Command::new("podman")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let cmd = match cmd {
            Ok(cmd) => Some(cmd),
            Err(err) => {
                log::warn!("Can not collect logs for '{}': '{}'", workload_id, err);
                None
            }
        };
        Self { child: cmd }
    }
}

impl AsyncRead for PodmanLogCollector {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.child {
            Some(child) => {
                if let Some(stdout) = child.stdout.as_mut() {
                    let x = Pin::new(stdout);
                    x.poll_read(cx, buf)
                } else {
                    log::warn!("Could not access stdout of log collecting service.");
                    std::task::Poll::Ready(std::io::Result::Ok(()))
                }
            }
            None => std::task::Poll::Ready(std::io::Result::Ok(())),
        }
    }
}

impl Drop for PodmanLogCollector {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            if let Err(err) = child.start_kill() {
                log::warn!("Could not stop log collection: '{}'", err);
            }
        }
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

// [utest->swdd~functions-required-by-runtime-connector~1]
#[cfg(test)]
mod tests {

    use std::sync::Mutex;

    use tokio::io::Empty;

    use crate::runtime_connectors::{podman::PodmanWorkloadId, LogRequestOptions};

    use super::PodmanLogCollector;

    const WORKLOAD_ID: &str = "workload_id";
    static CAN_SPAWN: Mutex<bool> = Mutex::new(true);
    static CAN_KILL: Mutex<bool> = Mutex::new(true);
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static WAS_KILLED: Mutex<bool> = Mutex::new(false);

    #[derive(Debug)]
    pub struct MockChild {
        pub stdout: Option<Empty>,
        cmd: String,
        args: Vec<String>,
        stdout_option: Option<std::process::Stdio>,
        stderr_option: Option<std::process::Stdio>,
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
        stdout: Option<std::process::Stdio>,
        stderr: Option<std::process::Stdio>,
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

        pub(crate) fn stdout(&mut self, piped: std::process::Stdio) -> &mut Self {
            self.stdout = Some(piped);
            self
        }

        pub(crate) fn stderr(&mut self, piped: std::process::Stdio) -> &mut Self {
            self.stderr = Some(piped);
            self
        }

        pub(crate) fn spawn(&mut self) -> Result<MockChild, String> {
            if *CAN_SPAWN.lock().unwrap() {
                Ok(MockChild {
                    stdout: None,
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
        let log_collector = PodmanLogCollector::new(
            &PodmanWorkloadId {
                id: WORKLOAD_ID.into(),
            },
            &LogRequestOptions {
                follow: false,
                tail: None,
                since: None,
                until: None,
            },
        );

        assert!(matches!(
            &log_collector.child,
            Some(MockChild {
                stdout: _,
                cmd,
                args,
                stdout_option: Some(_),
                stderr_option: Some(_)

            }) if cmd == "podman" && *args == vec!["logs".to_string(), WORKLOAD_ID.to_string()]
        ));
    }

    #[test]
    fn utest_new_with_with_parameters() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CAN_SPAWN.lock().unwrap() = true;
        let log_collector = PodmanLogCollector::new(
            &PodmanWorkloadId {
                id: WORKLOAD_ID.into(),
            },
            &LogRequestOptions {
                follow: true,
                tail: Some(10),
                since: Some("since".to_string()),
                until: Some("until".to_string()),
            },
        );

        assert!(matches!(
            &log_collector.child,
            Some(MockChild {
                stdout: _,
                cmd,
                args,
                stdout_option: Some(_),
                stderr_option: Some(_),
            }) if cmd == "podman" && *args == vec!["logs".to_string(), "-f".to_string(), "--since".to_string(), "since".to_string(), "--until".to_string(), "until".to_string(), "--tail".to_string(), "10".to_string(), WORKLOAD_ID.to_string(), ]
        ));
    }

    #[test]
    fn utest_new_spawn_fails() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CAN_SPAWN.lock().unwrap() = false;
        let log_collector = PodmanLogCollector::new(
            &PodmanWorkloadId {
                id: WORKLOAD_ID.into(),
            },
            &LogRequestOptions {
                follow: false,
                tail: None,
                since: None,
                until: None,
            },
        );

        assert!(&log_collector.child.is_none())
    }

    #[test]
    fn utest_dropped_child_kills_cmd() {
        let _guard = TEST_LOCK.lock().unwrap();
        *WAS_KILLED.lock().unwrap() = false;
        *CAN_SPAWN.lock().unwrap() = true;

        *CAN_KILL.lock().unwrap() = true;
        let log_collector = PodmanLogCollector::new(
            &PodmanWorkloadId {
                id: WORKLOAD_ID.into(),
            },
            &LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            },
        );

        assert!(!*WAS_KILLED.lock().unwrap());
        drop(log_collector);
        assert!(*WAS_KILLED.lock().unwrap());
    }

    #[test]
    fn utest_dropped_child_handles_kills_error() {
        let _guard = TEST_LOCK.lock().unwrap();
        *WAS_KILLED.lock().unwrap() = false;
        *CAN_SPAWN.lock().unwrap() = true;
        *CAN_KILL.lock().unwrap() = false;
        let log_collector = PodmanLogCollector::new(
            &PodmanWorkloadId {
                id: WORKLOAD_ID.into(),
            },
            &LogRequestOptions {
                follow: true,
                tail: None,
                since: None,
                until: None,
            },
        );

        assert!(!*WAS_KILLED.lock().unwrap());
        drop(log_collector);
        assert!(*WAS_KILLED.lock().unwrap());
    }
}

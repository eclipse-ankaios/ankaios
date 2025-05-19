use std::process::Stdio;
use tokio::io::AsyncRead;
use tokio::process::{ChildStderr, ChildStdout};

#[cfg(not(test))]
use tokio::process::{Child, Command};

#[cfg(test)]
use test::{MockChild as Child, MockCommand as Command};

use crate::runtime_connectors::runtime_connector::LogRequestOptions;

use super::super::log_collector::{StreamTrait, GetOutputStreams};
use super::PodmanWorkloadId;

#[derive(Debug)]
pub struct PodmanLogCollector {
    child: Option<Child>,
    #[cfg(test)]
    pub stdout: Option<Box<dyn StreamTrait>>,
    #[cfg(test)]
    pub stderr: Option<Box<dyn StreamTrait>>,
}

impl PodmanLogCollector {
    pub fn new(workload_id: &PodmanWorkloadId, options: &LogRequestOptions) -> Self {
        let mut args = Vec::with_capacity(9);
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
            .stderr(Stdio::piped())
            .spawn();
        let cmd = match cmd {
            Ok(cmd) => Some(cmd),
            Err(err) => {
                log::warn!("Can not collect logs for '{}': '{}'", workload_id, err);
                None
            }
        };
        #[cfg(not(test))]
        return Self { child: cmd };
        #[cfg(test)]
        Self {
            child: cmd,
            stdout: None,
            stderr: None,
        }
    }

    #[cfg(test)]
    pub fn set_stdout(&mut self, stdout: Option<Box<dyn StreamTrait>>) {
        self.stdout = stdout;
    }

    #[cfg(test)]
    pub fn set_stderr(&mut self, stderr: Option<Box<dyn StreamTrait>>) {
        self.stderr = stderr;
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

impl GetOutputStreams for PodmanLogCollector {
    type OutputStream = Box<dyn StreamTrait>;
    type ErrStream = Box<dyn StreamTrait>;

    fn get_output_stream(&mut self) -> (Option<Self::OutputStream>, Option<Self::ErrStream>) {
        #[cfg(not(test))]
        {
            if let Some(child) = &mut self.child {
                return (
                    child.stdout.take().map(|stdout| Box::new(stdout) as Box<dyn StreamTrait>),
                    child.stderr.take().map(|stderr| Box::new(stderr) as Box<dyn StreamTrait>),
                );
            }
            (None, None)
        }
        #[cfg(test)]
        return (
            self.stdout.take(),
            self.stderr.take(),
        );
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
pub mod test {
    use super::*;
    use std::io;

    pub struct MockCommand;

    impl MockCommand {
        pub fn new(bin: &str) -> Self {
            assert_eq!(bin, "podman");
            Self {}
        }
        pub fn args(self, args: Vec<&str>) -> Self {
            assert_eq!(args[0], "logs");
            assert_eq!(args[1], "-f");
            assert_eq!(args[2], "--since");
            assert_eq!(args[3], "test_since");
            assert_eq!(args[4], "--until");
            assert_eq!(args[5], "test_until");
            assert_eq!(args[6], "--tail");
            assert_eq!(args[7], "10");
            assert_eq!(args[8], "test");
            self
        }
        pub fn stdout(self, _stdio: Stdio) -> Self {
            self
        }
        pub fn stderr(self, _stdio: Stdio) -> Self {
            self
        }
        pub fn spawn(self) -> Result<MockChild, io::Error> {
            Ok(MockChild {
                stdout: None,
                stderr: None,
            })
        }
    }

    #[derive(Debug)]
    pub struct MockChild {
        pub stdout: Option<ChildStdout>,
        pub stderr: Option<ChildStderr>,
    }

    impl MockChild {
        pub fn start_kill(&mut self) -> Result<(), io::Error> {
            Ok(())
        }
    }

    #[test]
    fn test_podman_log_collector() {
        let workload_id = PodmanWorkloadId {
            id: "test".to_string(),
        };
        let options = LogRequestOptions {
            follow: true,
            since: Some("test_since".to_string()),
            until: Some("test_until".to_string()),
            tail: Some(10),
        };
        let mut collector = PodmanLogCollector::new(&workload_id, &options);
        assert!(collector.child.is_some());

        let (child_stdout, child_stderr) = collector.get_output_stream();
        assert!(child_stdout.is_none());
        assert!(child_stderr.is_none());
    }
}

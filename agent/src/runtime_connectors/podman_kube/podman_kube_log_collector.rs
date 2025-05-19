use std::pin::Pin;
use std::process::Stdio;
use tokio::io::AsyncRead;
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

use super::super::log_collector::GetOutputStreams;
use super::PodmanKubeWorkloadId;
use crate::runtime_connectors::runtime_connector::LogRequestOptions;

#[derive(Debug)]
pub struct PodmanLogCollector {
    child: Option<Child>,
}

// TODO improve this
impl PodmanLogCollector {
    pub fn new(workload_id: &PodmanKubeWorkloadId, options: &LogRequestOptions) -> Self {
        let pod_name = if let Some(pods) = &workload_id.pods {
            pods[0].as_str() // We collect the logs of only the first pod
        } else {
            log::warn!("No pod name found for workload id '{}'", workload_id);
            return Self { child: None };
        };
        let mut args = Vec::with_capacity(8);
        args.push("logs");
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
        let mut _tail = String::new();
        if let Some(tail2) = options.tail {
            _tail = tail2.to_string();
            args.push("--tail");
            args.push(_tail.as_str());
        }
        args.push(pod_name);

        let cmd = Command::new("podman")
            .args(&args)
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
        Self { child: cmd }
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
    type OutputStream = ChildStdout;
    type ErrStream = ChildStderr;

    fn get_output_stream(&mut self) -> (Option<Self::OutputStream>, Option<Self::ErrStream>) {
        (None, None)
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
pub mod test {}

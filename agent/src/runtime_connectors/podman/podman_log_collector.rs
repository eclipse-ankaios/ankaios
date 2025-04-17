use futures_util::stream::FuturesUnordered;
use futures_util::task::AtomicWaker;
use std::pin::{pin, Pin};
use std::process::Stdio;
use tokio::process::{Child, Command};

use tokio::io::AsyncRead;

use crate::runtime_connectors::{
    log_channel, log_collector::LogCollector, runtime_connector::LogRequestOptions,
};

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

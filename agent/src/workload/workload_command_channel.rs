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
use crate::{
    control_interface::ControlInterfacePath,
    runtime_connectors::{LogRequestOptions, log_fetcher::LogFetcher},
    workload::WorkloadCommand,
};
use api::ank_base::{WorkloadInstanceNameInternal, WorkloadNamed};
#[cfg(test)]
use mockall_double::double;
use tokio::sync::{mpsc, oneshot};

#[cfg_attr(test, double)]
use super::retry_manager::RetryToken;
static COMMAND_BUFFER_SIZE: usize = 5;

pub type WorkloadCommandReceiver = mpsc::Receiver<WorkloadCommand>;

#[derive(Clone)]
pub struct WorkloadCommandSender {
    sender: mpsc::Sender<WorkloadCommand>,
}

impl WorkloadCommandSender {
    pub fn new() -> (Self, WorkloadCommandReceiver) {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);
        (
            WorkloadCommandSender {
                sender: command_sender,
            },
            command_receiver,
        )
    }

    pub async fn create(&self) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender.send(WorkloadCommand::Create).await
    }

    pub async fn retry(
        &self,
        instance_name: WorkloadInstanceNameInternal,
        retry_token: RetryToken,
    ) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        let sender = self.sender.clone();

        // [impl->swdd~agent-workload-control-loop-exponential-backoff-retries~1]
        tokio::spawn(retry_token.call_with_backoff(|retry_token| async move {
            if sender
                .send(WorkloadCommand::Retry(Box::new(instance_name), retry_token))
                .await
                .is_err()
            {
                log::debug!("Could not send retry command");
            };
        }));

        Ok(())
    }

    pub async fn update(
        &self,
        workload_named: Option<WorkloadNamed>,
        control_interface_path: Option<ControlInterfacePath>,
    ) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender
            .send(WorkloadCommand::Update(
                workload_named.map(Box::new),
                control_interface_path,
            ))
            .await
    }

    pub fn resume(&self) -> Result<(), mpsc::error::TrySendError<WorkloadCommand>> {
        self.sender.try_send(WorkloadCommand::Resume)
    }

    pub async fn delete(self) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender.send(WorkloadCommand::Delete).await
    }

    // [impl->swdd~agent-workload-obj-start-log-fetcher-command~1]
    pub async fn start_collecting_logs(
        &self,
        log_request_options: LogRequestOptions,
    ) -> Result<Box<dyn LogFetcher>, Box<dyn std::error::Error>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(WorkloadCommand::StartLogFetcher(
                log_request_options,
                sender,
            ))
            .await?;
        Ok(receiver.await?)
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
    use crate::{
        runtime_connectors::{LogRequestOptions, log_fetcher::MockLogFetcher},
        workload::retry_manager::MockRetryToken,
    };
    use api::ank_base::WorkloadNamed;
    use api::test_utils::generate_test_workload;

    use super::{ControlInterfacePath, WorkloadCommand, WorkloadCommandSender};
    use std::path::PathBuf;
    use tokio::sync::mpsc::Receiver;

    use mockall::lazy_static;

    const PIPES_LOCATION: &str = "/some/path";
    const LOG_REQUEST_OPTIONS: LogRequestOptions = LogRequestOptions {
        follow: true,
        tail: Some(100),
        since: None,
        until: None,
    };

    lazy_static! {
        pub static ref WORKLOAD_SPEC: WorkloadNamed = generate_test_workload();
        pub static ref CONTROL_INTERFACE_PATH: Option<ControlInterfacePath> =
            Some(ControlInterfacePath::new(PathBuf::from(PIPES_LOCATION)));
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    #[tokio::test]
    async fn utest_send_create() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        workload_command_sender.create().await.unwrap();

        let workload_command = workload_command_receiver.recv().await.unwrap();

        assert_eq!(workload_command, WorkloadCommand::Create);
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    // [utest->swdd~agent-workload-control-loop-exponential-backoff-retries~1]
    #[tokio::test]
    async fn utest_send_retry() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();
        let retry_token = MockRetryToken {
            valid: true,
            has_been_called: false,
        };

        workload_command_sender
            .retry(WORKLOAD_SPEC.instance_name.clone(), retry_token)
            .await
            .unwrap();

        let workload_command = workload_command_receiver.recv().await.unwrap();

        let WorkloadCommand::Retry(received_instance_name, received_retry_token) = workload_command
        else {
            panic!("Expected WorkloadCommand::Retry");
        };

        assert_eq!(*received_instance_name, WORKLOAD_SPEC.instance_name);
        assert!(received_retry_token.has_been_called);
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    #[tokio::test]
    async fn utest_send_update() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        let workload = WORKLOAD_SPEC.clone();
        let control_interface_path = CONTROL_INTERFACE_PATH.clone();

        workload_command_sender
            .update(Some(workload.clone()), control_interface_path.clone())
            .await
            .unwrap();

        let workload_command = workload_command_receiver.recv().await.unwrap();

        assert_eq!(
            WorkloadCommand::Update(Some(Box::new(workload)), control_interface_path.clone()),
            workload_command
        );
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~4]
    #[tokio::test]
    async fn utest_send_delete() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        workload_command_sender.delete().await.unwrap();

        let workload_command = workload_command_receiver.recv().await.unwrap();

        assert!(matches!(workload_command, WorkloadCommand::Delete));
    }

    #[tokio::test]
    async fn utest_send_resume() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        workload_command_sender.resume().unwrap();

        let workload_command = workload_command_receiver.recv().await;

        assert_eq!(Some(WorkloadCommand::Resume), workload_command);
    }

    #[tokio::test]
    async fn utest_send_resume_error() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        // close the channel to simulate an error
        workload_command_receiver.close();

        assert!(workload_command_sender.resume().is_err());
    }

    // [utest->swdd~agent-workload-obj-start-log-fetcher-command~1]
    #[tokio::test]
    async fn utest_start_collecting_logs_success() {
        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();

        let jh =
            listen_for_start_log_fetcher(workload_command_receiver, Some(MockLogFetcher::new()));

        let res = workload_command_sender
            .start_collecting_logs(LOG_REQUEST_OPTIONS.clone())
            .await;

        assert!(res.is_ok());
        assert!(jh.await.is_ok());
    }

    // [utest->swdd~agent-workload-obj-start-log-fetcher-command~1]
    #[tokio::test]
    async fn utest_start_collecting_logs_no_result() {
        let (workload_command_sender, workload_command_receiver) = WorkloadCommandSender::new();

        let jh = listen_for_start_log_fetcher(workload_command_receiver, None);

        let res = workload_command_sender
            .start_collecting_logs(LOG_REQUEST_OPTIONS.clone())
            .await;

        assert!(res.is_err());
        assert!(jh.await.is_ok());
    }

    // [utest->swdd~agent-workload-obj-start-log-fetcher-command~1]
    #[tokio::test]
    async fn utest_start_collecting_logs_receiver_gone() {
        let (workload_command_sender, _) = WorkloadCommandSender::new();

        let res = workload_command_sender
            .start_collecting_logs(LOG_REQUEST_OPTIONS.clone())
            .await;

        assert!(res.is_err());
    }

    // [utest->swdd~agent-workload-obj-start-log-fetcher-command~1]
    fn listen_for_start_log_fetcher(
        mut receiver: Receiver<WorkloadCommand>,
        res: Option<MockLogFetcher>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let command = receiver.recv().await.unwrap();
            let WorkloadCommand::StartLogFetcher(options, result_sink) = command else {
                panic!("Expected WorkloadCommand::StartLogFetcher")
            };
            assert_eq!(options, LOG_REQUEST_OPTIONS);
            if let Some(res) = res {
                result_sink.send(Box::new(res)).unwrap();
            };
        })
    }
}

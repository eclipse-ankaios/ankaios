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
use crate::workload::WorkloadCommand;
use common::objects::{WorkloadInstanceName, WorkloadSpec};
use std::path::PathBuf;
use tokio::sync::mpsc;

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
        instance_name: WorkloadInstanceName,
    ) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender
            .send(WorkloadCommand::Retry(Box::new(instance_name)))
            .await
    }

    pub async fn update(
        &self,
        workload_spec: Option<WorkloadSpec>,
        control_interface_path: Option<PathBuf>,
    ) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender
            .send(WorkloadCommand::Update(
                workload_spec.map(Box::new),
                control_interface_path,
            ))
            .await
    }

    pub async fn resume(&self) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender.send(WorkloadCommand::Resume).await
    }

    pub async fn delete(self) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender.send(WorkloadCommand::Delete).await
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
    use super::*;
    use common::objects::generate_test_workload_spec;
    const PIPES_LOCATION: &str = "/some/path";

    use mockall::lazy_static;

    lazy_static! {
        pub static ref WORKLOAD_SPEC: WorkloadSpec = generate_test_workload_spec();
        pub static ref CONTROL_INTERFACE_PATH: Option<PathBuf> =
            Some(PathBuf::from(PIPES_LOCATION));
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~2]
    #[tokio::test]
    async fn utest_send_create() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        workload_command_sender.create().await.unwrap();

        let workload_command = workload_command_receiver.recv().await.unwrap();

        assert_eq!(workload_command, WorkloadCommand::Create);
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~2]
    #[tokio::test]
    async fn utest_send_retry() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        workload_command_sender
            .retry(WORKLOAD_SPEC.instance_name.clone())
            .await
            .unwrap();

        let workload_command = workload_command_receiver.recv().await.unwrap();

        assert!(
            matches!(workload_command, WorkloadCommand::Retry(received_instance_name) if *received_instance_name == WORKLOAD_SPEC.instance_name)
        );
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~2]
    #[tokio::test]
    async fn utest_send_update() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        let workload_spec = WORKLOAD_SPEC.clone();
        let control_interface = CONTROL_INTERFACE_PATH.clone();

        workload_command_sender
            .update(Some(workload_spec.clone()), control_interface.clone())
            .await
            .unwrap();

        let workload_command = workload_command_receiver.recv().await.unwrap();

        assert_eq!(
            WorkloadCommand::Update(Some(Box::new(workload_spec)), control_interface.clone()),
            workload_command
        );
    }

    // [utest->swdd~agent-workload-control-loop-executes-create~2]
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

        workload_command_sender.resume().await.unwrap();

        let workload_command = workload_command_receiver.recv().await;

        assert_eq!(Some(WorkloadCommand::Resume), workload_command);
    }

    #[tokio::test]
    async fn utest_send_resume_error() {
        let (workload_command_sender, mut workload_command_receiver) = WorkloadCommandSender::new();

        // close the channel to simulate an error
        workload_command_receiver.close();

        assert!(workload_command_sender.resume().await.is_err());
    }
}

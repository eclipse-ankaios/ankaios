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
use common::objects::WorkloadSpec;
use std::path::PathBuf;
use tokio::sync::mpsc;

static COMMAND_BUFFER_SIZE: usize = 5;

pub type WorkloadCommandSender = mpsc::Sender<WorkloadCommand>;
pub type WorkloadCommandReceiver = mpsc::Receiver<WorkloadCommand>;

#[derive(Clone)]
pub struct WorkloadCommandChannel {
    sender: WorkloadCommandSender,
}

impl WorkloadCommandChannel {
    pub fn new() -> (Self, WorkloadCommandReceiver) {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_BUFFER_SIZE);
        (
            WorkloadCommandChannel {
                sender: command_sender,
            },
            command_receiver,
        )
    }

    pub async fn restart(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
    ) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender
            .send(WorkloadCommand::Restart(
                Box::new(workload_spec),
                control_interface_path,
            ))
            .await
    }

    pub async fn update(
        &self,
        workload_spec: WorkloadSpec,
        control_interface_path: Option<PathBuf>,
    ) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender
            .send(WorkloadCommand::Update(
                Box::new(workload_spec),
                control_interface_path,
            ))
            .await
    }

    pub async fn delete(self) -> Result<(), mpsc::error::SendError<WorkloadCommand>> {
        self.sender.send(WorkloadCommand::Delete).await
    }
}

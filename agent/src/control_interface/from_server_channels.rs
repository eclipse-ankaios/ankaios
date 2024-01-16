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

use common::execution_interface::{FromServerReceiver, FromServerSender};
use tokio::sync::mpsc;

pub struct FromServerChannels {
    sender: FromServerSender,
    receiver: FromServerReceiver,
}

#[cfg_attr(test, mockall::automock)]
impl FromServerChannels {
    pub fn new(buf_size: usize) -> Self {
        let (sender, receiver) = mpsc::channel(buf_size);
        Self { sender, receiver }
    }
    pub fn get_sender(&self) -> FromServerSender {
        self.sender.clone()
    }
    pub fn move_receiver(self) -> FromServerReceiver {
        self.receiver
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

    #[tokio::test]
    async fn utest_execution_command_channels_new() {
        let test_channels = FromServerChannels::new(1024);

        let _ = test_channels
            .get_sender()
            .send(common::execution_interface::FromServer::Stop(
                common::commands::Stop {},
            ))
            .await;
        let mut receiver = test_channels.move_receiver();
        assert!(matches!(
            receiver.recv().await,
            Some(common::execution_interface::FromServer::Stop(
                common::commands::Stop {}
            ))
        ))
    }
}

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

use tokio::sync::{mpsc, watch};

#[cfg(test)]
use mockall::automock;

pub struct Receiver {
    log_line_receiver: mpsc::Receiver<Vec<String>>,
    receiver_dropped_sink: watch::Sender<bool>,
}

#[cfg_attr(test, automock)]
impl Receiver {
    pub async fn read_log_lines(&mut self) -> Option<Vec<String>> {
        self.log_line_receiver.recv().await
    }

    #[cfg(test)]
    pub fn take_log_line_receiver(&mut self) -> mpsc::Receiver<Vec<String>> {
        let (_, new_receiver) = mpsc::channel(1);
        std::mem::replace(&mut self.log_line_receiver, new_receiver)
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        _ = self.receiver_dropped_sink.send(true);
    }
}

pub struct Sender {
    log_line_sender: mpsc::Sender<Vec<String>>,
    receiver_dropped: watch::Receiver<bool>,
}

impl Sender {
    pub async fn send_log_lines(
        &self,
        log_lines: Vec<String>,
    ) -> Result<(), mpsc::error::SendError<Vec<String>>> {
        self.log_line_sender.send(log_lines).await
    }

    pub async fn send_stop(&self) -> Result<(), mpsc::error::SendError<Vec<String>>> {
        self.log_line_sender.send(Default::default()).await
    }

    pub async fn wait_for_receiver_dropped(&mut self) {
        // Errors can be ignores, as `wait_for` only return an error if the channel is closed, in which case the sender is also dropped
        _ = self.receiver_dropped.wait_for(|x| *x).await;
    }
}

pub fn channel() -> (Sender, Receiver) {
    let (log_line_sender, log_line_receiver) = mpsc::channel(1);
    let (receiver_dropped_sink, receiver_dropped) = watch::channel(false);
    (
        Sender {
            log_line_sender,
            receiver_dropped,
        },
        Receiver {
            log_line_receiver,
            receiver_dropped_sink,
        },
    )
}

/////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::channel;

    const LINE1: [&str; 2] = ["line 1", "line 2"];
    const LINE2: [&str; 1] = ["line 3"];
    const LINE3: [&str; 3] = ["line 4", "line 5", "line 6"];

    #[tokio::test]
    async fn utest_sender_dropped() {
        let (sender, mut receiver) = channel();
        let jh = tokio::spawn(async move {
            sender.send_log_lines(into_vec(LINE1)).await.unwrap();
            sender.send_log_lines(into_vec(LINE2)).await.unwrap();
            sender.send_log_lines(into_vec(LINE3)).await.unwrap();
        });

        assert_eq!(receiver.read_log_lines().await, Some(into_vec(LINE1)));
        assert_eq!(receiver.read_log_lines().await, Some(into_vec(LINE2)));
        assert_eq!(receiver.read_log_lines().await, Some(into_vec(LINE3)));
        assert_eq!(receiver.read_log_lines().await, None);

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn utest_receiver_dropped() {
        let (mut sender, mut receiver) = channel();
        let jh = tokio::spawn(async move {
            assert_eq!(receiver.read_log_lines().await, Some(into_vec(LINE1)));
            assert_eq!(receiver.read_log_lines().await, Some(into_vec(LINE2)));
            assert_eq!(receiver.read_log_lines().await, Some(into_vec(LINE3)));
        });

        sender.send_log_lines(into_vec(LINE1)).await.unwrap();
        sender.send_log_lines(into_vec(LINE2)).await.unwrap();
        sender.send_log_lines(into_vec(LINE3)).await.unwrap();

        tokio::time::timeout(
            Duration::from_millis(10),
            sender.wait_for_receiver_dropped(),
        )
        .await
        .unwrap();

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn utest_receiver_dropped_before_wait() {
        let (mut sender, receiver) = channel();
        drop(receiver);

        tokio::time::timeout(
            Duration::from_millis(10),
            sender.wait_for_receiver_dropped(),
        )
        .await
        .unwrap();
    }

    fn into_vec<const N: usize>(array: [&str; N]) -> Vec<String> {
        array.into_iter().map(str::to_string).collect()
    }
}

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

use std::ops::DerefMut;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use tokio::select;

use super::log_channel;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait LogCollector: std::fmt::Debug + Send {
    async fn next_lines(&mut self) -> Option<Vec<String>>;
}

pub async fn run(
    mut log_collector: impl DerefMut<Target: LogCollector>,
    mut sender: log_channel::Sender,
) {
    loop {
        select! {
            lines = log_collector.next_lines() => {

                if let Some(lines) = lines {
                    let res = sender.send_log_lines(lines).await;

                    if let Err(e) = res {
                        log::error!("Failed to send log lines: {}", e);
                        break;
                    }
                } else {
                    let res = sender.send_stop().await;
                    if let Err(e) = res {
                        log::error!("Failed to send stop message: {}", e);
                    }
                    break;
                }

            }
            _ = sender.wait_for_receiver_dropped() => {
                break;
            }
        }
    }
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
    use std::{collections::VecDeque, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use tokio::{sync::Semaphore, time::timeout};

    use crate::runtime_connectors::log_channel;

    use super::LogCollector;

    const LINES_1: [&str; 3] = ["line 1 1", "line 1 2", "line 1 3"];
    const LINES_2: [&str; 2] = ["line 2 1", "line 2 2"];
    const LINES_3: [&str; 4] = ["line 3 1", "line 3 2", "line 3 3", "line 3 4"];

    const TIMEOUT: Duration = Duration::from_millis(10);

    #[derive(Debug)]
    struct MockLogCollector {
        mock_data: VecDeque<Vec<String>>,
        limited: bool,
        semaphore: Arc<Semaphore>,
    }

    impl MockLogCollector {
        fn new<'a>(data: &'a [&'a [&'a str]], limited: bool) -> Self {
            Self {
                mock_data: data
                    .iter()
                    .map(|x| x.iter().map(|y| y.to_string()).collect())
                    .collect(),
                semaphore: Arc::new(Semaphore::new(0)),
                limited,
            }
        }

        fn semaphore(&self) -> Arc<Semaphore> {
            self.semaphore.clone()
        }
    }

    #[async_trait]
    impl LogCollector for MockLogCollector {
        async fn next_lines(&mut self) -> Option<Vec<String>> {
            self.semaphore.acquire().await.unwrap().forget();
            if self.limited {
                self.mock_data.pop_front()
            } else {
                let res = self.mock_data.pop_front()?;
                self.mock_data.push_back(res.clone());
                Some(res)
            }
        }
    }

    #[tokio::test]
    async fn utest_log_collector_read_all_lines() {
        let log_collector = MockLogCollector::new(&[&LINES_1, &LINES_2, &LINES_3], true);
        let sem = log_collector.semaphore();
        sem.add_permits(4);

        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_collector), sender));

        assert_eq!(
            timeout(TIMEOUT, receiver.read_log_lines()).await,
            Ok(Some(LINES_1.iter().map(|&x| x.into()).collect()))
        );
        assert_eq!(
            timeout(TIMEOUT, receiver.read_log_lines()).await,
            Ok(Some(LINES_2.iter().map(|&x| x.into()).collect()))
        );
        assert_eq!(
            timeout(TIMEOUT, receiver.read_log_lines()).await,
            Ok(Some(LINES_3.iter().map(|&x| x.into()).collect()))
        );
        assert_eq!(timeout(TIMEOUT, receiver.read_log_lines()).await, Ok(None));
        timeout(TIMEOUT, jh).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn utest_log_collector_cannot_send_message() {
        let log_collector = MockLogCollector::new(&[&LINES_1, &LINES_2, &LINES_3], false);
        let sem = log_collector.semaphore();
        sem.add_permits(4);

        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_collector), sender));

        assert_eq!(
            timeout(TIMEOUT, receiver.read_log_lines()).await,
            Ok(Some(LINES_1.iter().map(|&x| x.into()).collect()))
        );
        assert_eq!(
            timeout(TIMEOUT, receiver.read_log_lines()).await,
            Ok(Some(LINES_2.iter().map(|&x| x.into()).collect()))
        );
        receiver.take_log_line_receiver();
        timeout(TIMEOUT, jh).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn utest_log_collector_informed_about_receiver_dropped() {
        let log_collector = MockLogCollector::new(&[&LINES_1, &LINES_2, &LINES_3], false);
        let sem = log_collector.semaphore();
        sem.add_permits(2);

        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_collector), sender));

        assert_eq!(
            timeout(TIMEOUT, receiver.read_log_lines()).await,
            Ok(Some(LINES_1.iter().map(|&x| x.into()).collect()))
        );
        assert_eq!(
            timeout(TIMEOUT, receiver.read_log_lines()).await,
            Ok(Some(LINES_2.iter().map(|&x| x.into()).collect()))
        );
        drop(receiver);
        timeout(TIMEOUT, jh).await.unwrap().unwrap();
    }
}

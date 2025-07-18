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

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use tokio::{io::AsyncRead, select};

use super::log_channel;

#[derive(Clone)]
pub enum NextLinesResult {
    Stdout(Vec<String>),
    Stderr(Vec<String>),
    EoF,
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait LogPicker: std::fmt::Debug + Send + 'static {
    async fn next_lines(&mut self) -> NextLinesResult;
}

pub trait StreamTrait: AsyncRead + std::fmt::Debug + Send + Unpin {}
impl<T: AsyncRead + std::fmt::Debug + Send + Unpin> StreamTrait for T {}

pub trait GetOutputStreams {
    type OutputStream: StreamTrait;
    type ErrStream: StreamTrait;
    fn get_output_streams(&mut self) -> (Option<Self::OutputStream>, Option<Self::ErrStream>);
}

pub async fn run(mut log_picker: Box<dyn LogPicker>, mut sender: log_channel::Sender) {
    loop {
        select! {
            lines = log_picker.next_lines() => {
                match lines{
                    NextLinesResult::Stdout(lines) => {
                        let res = sender.send_log_lines(lines).await;
                        if let Err(err) = res {
                            log::warn!("Could not forward stdout log lines: {:?}", err.0);
                            break;
                        }
                    }
                    NextLinesResult::Stderr(lines) => {
                        let res = sender.send_log_lines(lines).await;
                        if let Err(err) = res {
                            log::warn!("Could not forward stderr log lines: {:?}", err.0);
                            break;
                        }
                    }
                    NextLinesResult::EoF => {
                        log::debug!("Log picker returned no more log lines, stopping.");
                        drop(sender); // drop the non-cloneable log sender to indicate stop of log responses
                        break;
                    }
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

    use super::{LogPicker, NextLinesResult};

    const LINES_1: [&str; 3] = ["line 1 1", "line 1 2", "line 1 3"];
    const LINES_2: [&str; 2] = ["line 2 1", "line 2 2"];
    const LINES_3: [&str; 4] = ["line 3 1", "line 3 2", "line 3 3", "line 3 4"];

    const TIMEOUT: Duration = Duration::from_millis(10);

    #[derive(Debug)]
    enum NextLineType {
        Stdout,
        Stderr,
    }

    #[derive(Debug)]
    struct MockLogPicker {
        mock_data: VecDeque<Vec<String>>,
        limited: bool,
        semaphore: Arc<Semaphore>,
        line_type: NextLineType,
    }

    impl MockLogPicker {
        fn new<'a>(data: &'a [&'a [&'a str]], limited: bool, line_type: NextLineType) -> Self {
            Self {
                mock_data: data
                    .iter()
                    .map(|x| x.iter().map(|y| y.to_string()).collect())
                    .collect(),
                semaphore: Arc::new(Semaphore::new(0)),
                limited,
                line_type,
            }
        }

        fn semaphore(&self) -> Arc<Semaphore> {
            self.semaphore.clone()
        }
    }

    #[async_trait]
    impl LogPicker for MockLogPicker {
        async fn next_lines(&mut self) -> NextLinesResult {
            self.semaphore.acquire().await.unwrap().forget();
            if self.limited {
                match self.mock_data.pop_front() {
                    Some(res) => match self.line_type {
                        NextLineType::Stdout => NextLinesResult::Stdout(res),
                        NextLineType::Stderr => NextLinesResult::Stderr(res),
                    },
                    None => NextLinesResult::EoF,
                }
            } else {
                let res = self.mock_data.pop_front().unwrap();
                self.mock_data.push_back(res.clone());
                match self.line_type {
                    NextLineType::Stdout => NextLinesResult::Stdout(res),
                    NextLineType::Stderr => NextLinesResult::Stderr(res),
                }
            }
        }
    }

    #[tokio::test]
    async fn utest_log_picker_read_all_lines() {
        let log_picker =
            MockLogPicker::new(&[&LINES_1, &LINES_2, &LINES_3], true, NextLineType::Stdout);
        let sem = log_picker.semaphore();
        sem.add_permits(4);

        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_picker), sender));

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
    async fn utest_log_picker_cannot_send_message() {
        let log_picker =
            MockLogPicker::new(&[&LINES_1, &LINES_2, &LINES_3], false, NextLineType::Stdout);
        let sem = log_picker.semaphore();
        sem.add_permits(4);

        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_picker), sender));

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
    async fn utest_log_picker_informed_about_receiver_dropped() {
        let log_picker =
            MockLogPicker::new(&[&LINES_1, &LINES_2, &LINES_3], false, NextLineType::Stdout);
        let sem = log_picker.semaphore();
        sem.add_permits(2);

        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_picker), sender));

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

    #[tokio::test]
    async fn utest_log_picker_stderr_read_all_lines() {
        let log_picker =
            MockLogPicker::new(&[&LINES_1, &LINES_2, &LINES_3], true, NextLineType::Stderr);
        let sem = log_picker.semaphore();

        sem.add_permits(4);
        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_picker), sender));
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
    async fn utest_log_picker_stderr_cannot_send_message() {
        let log_picker =
            MockLogPicker::new(&[&LINES_1, &LINES_2, &LINES_3], false, NextLineType::Stderr);
        let sem = log_picker.semaphore();
        sem.add_permits(4);

        let (sender, mut receiver) = log_channel::channel();
        let jh = tokio::spawn(super::run(Box::new(log_picker), sender));

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
        assert_eq!(timeout(TIMEOUT, receiver.read_log_lines()).await, Ok(None));
    }
}

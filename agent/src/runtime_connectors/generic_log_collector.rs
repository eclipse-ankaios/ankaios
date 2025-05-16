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
use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::log_collector::LogCollector;

const LINE_FEED: u8 = 0x0A;

#[derive(Debug)]
pub struct GenericLogCollector<T: AsyncRead + std::fmt::Debug> {
    reader: T,
    read_data: BytesMut,
}

impl<T: AsyncRead + std::fmt::Debug> GenericLogCollector<T> {
    pub fn new(read: T) -> Self {
        Self {
            reader: read,
            read_data: BytesMut::new(),
        }
    }
}

#[async_trait]
impl<T: AsyncRead + std::fmt::Debug + std::marker::Unpin + std::marker::Send> LogCollector
    for GenericLogCollector<T>
{
    async fn next_lines(&mut self) -> Option<Vec<String>> {
        let mut start_byte = self.read_data.len();
        match self.reader.read_buf(&mut self.read_data).await {
            Ok(0) => {
                if start_byte == 0 {
                    return None;
                } else {
                    return Some(vec![convert_to_string(self.read_data.split())]);
                }
            }
            Err(err) => {
                log::warn!("Failed to read log lines: {:?}", err);
                return None;
            }
            _ => {}
        }

        let mut res = Vec::<String>::new();

        while let Some((pos, _)) = &(*self.read_data)[start_byte..]
            .iter()
            .enumerate()
            .find(|(_, value)| **value == LINE_FEED)
        {
            let line = self.read_data.split_to(start_byte + pos + 1);
            let mut line = convert_to_string(line);
            line.pop();
            res.push(line);
            start_byte = 0;
        }
        if res.is_empty() {
            self.next_lines().await
        } else {
            Some(res)
        }
    }
}

fn convert_to_string(vec: impl Into<Vec<u8>>) -> String {
    match String::from_utf8(vec.into()) {
        Ok(res) => res,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
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
    use std::{collections::VecDeque, vec};

    use tokio::io::AsyncRead;

    use crate::runtime_connectors::{
        generic_log_collector::GenericLogCollector, log_collector::LogCollector,
    };

    const LINE_1: &str = "first line";
    const LINE_2: &str = "second line";
    const LINE_3: &str = "third line";
    const LINE_4: &str = "forth line";
    const LINE_5: &str = "fifth line";

    #[derive(Debug)]
    struct MockRead {
        data: VecDeque<MockReadDataEntry>,
    }

    #[derive(Debug)]
    enum MockReadDataEntry {
        Data(Vec<u8>),
        Error(std::io::Error),
    }

    impl MockReadDataEntry {
        fn data(data: &str) -> Self {
            Self::Data(data.as_bytes().to_owned())
        }

        fn error() -> Self {
            Self::Error(std::io::Error::new(
                std::io::ErrorKind::Other,
                "".to_string(),
            ))
        }
    }

    impl AsyncRead for MockRead {
        fn poll_read(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            let element = self.data.pop_front();
            match element {
                Some(MockReadDataEntry::Data(data)) => {
                    buf.put_slice(&data);
                    std::task::Poll::Ready(std::io::Result::Ok(()))
                }
                Some(MockReadDataEntry::Error(err)) => {
                    std::task::Poll::Ready(std::io::Result::Err(err))
                }
                None => std::task::Poll::Ready(std::io::Result::Ok(())),
            }
        }
    }

    #[tokio::test]
    async fn utest_multiple_lines() {
        let read = MockRead {
            data: vec![
                MockReadDataEntry::data(&format!("{LINE_1}\n{LINE_2}\n{LINE_3}\n")),
                MockReadDataEntry::data(&format!("{LINE_4}\n{LINE_5}\n")),
            ]
            .into(),
        };

        let mut log_collector = GenericLogCollector::new(read);
        assert_eq!(
            log_collector.next_lines().await,
            Some(vec![LINE_1.into(), LINE_2.into(), LINE_3.into()])
        );
        assert_eq!(
            log_collector.next_lines().await,
            Some(vec![LINE_4.into(), LINE_5.into()])
        );
        assert_eq!(log_collector.next_lines().await, None);
    }

    #[tokio::test]
    async fn utest_last_newline_missing() {
        let read = MockRead {
            data: vec![
                MockReadDataEntry::data(&format!("{LINE_1}\n{LINE_2}\n{LINE_3}\n")),
                MockReadDataEntry::data(&format!("{LINE_4}\n{LINE_5}")),
            ]
            .into(),
        };

        let mut log_collector = GenericLogCollector::new(read);
        assert_eq!(
            log_collector.next_lines().await,
            Some(vec![LINE_1.into(), LINE_2.into(), LINE_3.into()])
        );
        assert_eq!(log_collector.next_lines().await, Some(vec![LINE_4.into()]));
        assert_eq!(log_collector.next_lines().await, Some(vec![LINE_5.into()]));
        assert_eq!(log_collector.next_lines().await, None);
    }

    #[tokio::test]
    async fn utest_line_split_multiple_times() {
        let read = MockRead {
            data: vec![
                MockReadDataEntry::data("first"),
                MockReadDataEntry::data(" "),
                MockReadDataEntry::data(&format!("line\n{LINE_2}\nthird ")),
                MockReadDataEntry::data("line\n"),
            ]
            .into(),
        };

        let mut log_collector = GenericLogCollector::new(read);
        assert_eq!(
            log_collector.next_lines().await,
            Some(vec![LINE_1.into(), LINE_2.into()])
        );
        assert_eq!(log_collector.next_lines().await, Some(vec![LINE_3.into()]));
        assert_eq!(log_collector.next_lines().await, None);
    }

    #[tokio::test]
    async fn utest_handle_non_utf8() {
        let read = MockRead {
            data: vec![MockReadDataEntry::Data(vec![
                0x6c, 0x69, 0x6e, 0x65, 0x90, 0x0A,
            ])]
            .into(),
        };

        let mut log_collector = GenericLogCollector::new(read);
        assert_eq!(log_collector.next_lines().await, Some(vec!["line�".into()]));
        assert_eq!(log_collector.next_lines().await, None);
    }

    #[tokio::test]
    async fn utest_read_fails() {
        let read = MockRead {
            data: vec![
                MockReadDataEntry::data(&format!("{LINE_1}\nsecond")),
                MockReadDataEntry::error(),
            ]
            .into(),
        };

        let mut log_collector = GenericLogCollector::new(read);
        assert_eq!(log_collector.next_lines().await, Some(vec![LINE_1.into()]));
        assert_eq!(log_collector.next_lines().await, None);
    }
}

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
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    select,
};

use super::log_picker::{GetOutputStreams, LogPicker, NextLinesResult};

const LINE_FEED: u8 = 0x0A;

#[derive(Debug)]
pub struct GenericSingleLogCollector<T: AsyncRead + std::fmt::Debug> {
    reader: T,
    read_data: BytesMut,
}

#[derive(Debug)]
pub struct GenericLogCollector<T>
where
    T: GetOutputStreams,
{
    stdout: Option<GenericSingleLogCollector<T::OutputStream>>,
    stderr: Option<GenericSingleLogCollector<T::ErrStream>>,
    _streams: T,
}

impl<T: GetOutputStreams> GenericLogCollector<T> {
    pub fn new(mut streams: T) -> Self {
        let (stdout, stderr) = streams.get_output_streams();
        Self {
            stdout: stdout.map(GenericSingleLogCollector::new),
            stderr: stderr.map(GenericSingleLogCollector::new),
            _streams: streams,
        }
    }
}

#[async_trait]
impl<T: GetOutputStreams + std::fmt::Debug + Send + 'static> LogPicker for GenericLogCollector<T> {
    async fn next_lines(&mut self) -> NextLinesResult {
        loop {
            match (&mut self.stdout, &mut self.stderr) {
                (Some(ref mut stdout), Some(ref mut stderr)) => {
                    select! {
                        lines = stdout.next_lines() => {
                            if let Some(lines) = lines {
                                return NextLinesResult::Stdout(lines);
                            } else {
                                self.stdout = None;
                            }
                        }
                        lines = stderr.next_lines() => {
                            if let Some(lines) = lines {
                                return NextLinesResult::Stderr(lines);
                            } else {
                                self.stderr = None;
                            }
                        }
                    }
                }
                (Some(ref mut stdout), None) => {
                    if let Some(lines) = stdout.next_lines().await {
                        return NextLinesResult::Stdout(lines);
                    } else {
                        return NextLinesResult::EoF;
                    }
                }
                (None, Some(ref mut stderr)) => {
                    if let Some(lines) = stderr.next_lines().await {
                        return NextLinesResult::Stderr(lines);
                    } else {
                        return NextLinesResult::EoF;
                    }
                }
                (None, None) => {
                    return NextLinesResult::EoF;
                }
            }
        }
    }
}

impl<T: AsyncRead + std::fmt::Debug + std::marker::Unpin> GenericSingleLogCollector<T> {
    pub fn new(read: T) -> Self {
        Self {
            reader: read,
            read_data: BytesMut::new(),
        }
    }

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
            Box::pin(self.next_lines()).await
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

    use super::NextLinesResult;
    use crate::runtime_connectors::{
        generic_log_collector::{GenericLogCollector, GenericSingleLogCollector},
        log_picker::{LogPicker, StreamTrait},
        podman::{podman_log_collector::PodmanLogCollector, PodmanWorkloadId},
        runtime_connector::LogRequestOptions,
    };

    const LINE_1: &str = "first line";
    const LINE_2: &str = "second line";
    const LINE_3: &str = "third line";
    const LINE_4: &str = "forth line";
    const LINE_5: &str = "fifth line";
    const STDOUT_LINE: &str = "line_from_stdout";
    const STDERR_LINE: &str = "line_from_stderr";

    #[derive(Debug)]
    pub(crate) struct MockRead {
        pub(crate) data: VecDeque<MockReadDataEntry>,
    }

    #[derive(Debug)]
    pub(crate) enum MockReadDataEntry {
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

        let mut log_collector = GenericSingleLogCollector::new(read);
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

        let mut log_collector = GenericSingleLogCollector::new(read);
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

        let mut log_collector = GenericSingleLogCollector::new(read);
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

        let mut log_collector = GenericSingleLogCollector::new(read);
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

        let mut log_collector = GenericSingleLogCollector::new(read);
        assert_eq!(log_collector.next_lines().await, Some(vec![LINE_1.into()]));
        assert_eq!(log_collector.next_lines().await, None);
    }

    #[tokio::test]
    async fn utest_generic_log_collector_none() {
        let mut generic_log_collector = create_generic_log_collector(None, None);
        assert!(matches!(
            generic_log_collector.next_lines().await,
            NextLinesResult::EoF
        ));
    }

    #[tokio::test]
    async fn utest_generic_log_collector_stdout() {
        let stdout = MockRead {
            data: vec![MockReadDataEntry::data(&format!("{STDOUT_LINE}\n"))].into(),
        };

        let mut generic_log_collector = create_generic_log_collector(Some(Box::new(stdout)), None);
        assert!(matches!(
            generic_log_collector.next_lines().await,
            NextLinesResult::Stdout(lines) if lines == vec![STDOUT_LINE.to_string()]
        ));
        assert!(matches!(
            generic_log_collector.next_lines().await,
            NextLinesResult::EoF
        ));
    }

    #[tokio::test]
    async fn utest_generic_log_collector_stderr() {
        let stderr = MockRead {
            data: vec![MockReadDataEntry::data(&format!("{STDERR_LINE}\n"))].into(),
        };

        let mut generic_log_collector = create_generic_log_collector(None, Some(Box::new(stderr)));
        assert!(matches!(
            generic_log_collector.next_lines().await,
            NextLinesResult::Stderr(lines) if lines == vec![STDERR_LINE.to_string()]
        ));
        assert!(matches!(
            generic_log_collector.next_lines().await,
            NextLinesResult::EoF
        ));
    }

    #[tokio::test]
    async fn utest_generic_log_collector_stdout_and_stderr() {
        let stdout = MockRead {
            data: vec![MockReadDataEntry::data(&format!("{STDOUT_LINE}\n"))].into(),
        };
        let stderr = MockRead {
            data: vec![MockReadDataEntry::data(&format!("{STDERR_LINE}\n"))].into(),
        };

        let mut generic_log_collector =
            create_generic_log_collector(Some(Box::new(stdout)), Some(Box::new(stderr)));
        let mut lines_from_stdout = 0;
        let mut lines_from_stderr = 0;

        for _ in 0..2 {
            let line = generic_log_collector.next_lines().await;
            match line {
                NextLinesResult::Stdout(lines) => {
                    assert_eq!(lines, vec![STDOUT_LINE.to_string()]);
                    lines_from_stdout += 1;
                }
                NextLinesResult::Stderr(lines) => {
                    assert_eq!(lines, vec![STDERR_LINE.to_string()]);
                    lines_from_stderr += 1;
                }
                NextLinesResult::EoF => {
                    panic!("Unexpected EoF");
                }
            }
        }

        assert_eq!(lines_from_stdout, 1);
        assert_eq!(lines_from_stderr, 1);

        // Last line must be EoF
        assert!(matches!(
            generic_log_collector.next_lines().await,
            NextLinesResult::EoF
        ));
    }

    fn create_generic_log_collector(
        stdout: Option<Box<dyn StreamTrait>>,
        stderr: Option<Box<dyn StreamTrait>>,
    ) -> GenericLogCollector<PodmanLogCollector> {
        let workload_id = PodmanWorkloadId {
            id: "test".to_string(),
        };
        let mut podman_log_collector = PodmanLogCollector::new(
            &workload_id,
            &LogRequestOptions {
                follow: true,
                since: Some("test_since".to_string()),
                until: Some("test_until".to_string()),
                tail: Some(10),
            },
        );
        podman_log_collector.set_stdout(stdout);
        podman_log_collector.set_stderr(stderr);

        GenericLogCollector::new(podman_log_collector)
    }
}

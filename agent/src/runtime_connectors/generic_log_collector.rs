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
    use std::collections::VecDeque;

    use tokio::io::{AsyncRead, AsyncReadExt};

    use crate::runtime_connectors::{
        generic_log_collector::GenericLogCollector,
        log_collector::{self, LogCollector},
    };

    #[derive(Debug)]
    struct MockRead {
        data: VecDeque<MockReadDataEntry>,
    }

    #[derive(Debug)]
    enum MockReadDataEntry {
        Data(String),
        Error(std::io::Error),
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
                    buf.put_slice(data.as_bytes());
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
    async fn utest_foobar() {
        let read = MockRead {
            data: vec![
                MockReadDataEntry::Data("first".into()),
                MockReadDataEntry::Data(" ".into()),
                MockReadDataEntry::Data("line\nsecond line\nlast ".into()),
                MockReadDataEntry::Data("bytes\n".into()),
            ]
            .into(),
        };

        let mut log_collector = GenericLogCollector::new(read);
        while let Some(lines) = log_collector.next_lines().await {
            println!("====================");
            println!("{:?}", lines);
        }
    }
}

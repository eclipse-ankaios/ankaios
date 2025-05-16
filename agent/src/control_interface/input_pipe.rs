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

use std::{
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

use tokio::{
    io::{self, AsyncReadExt, BufReader},
    net::unix::pipe::{OpenOptions, Receiver},
};

#[derive(Debug)]
pub struct InputPipe {
    path: PathBuf,
    file: Option<BufReader<Receiver>>,
}

impl InputPipe {
    const MAX_VARINT_SIZE: usize = 19;

    pub fn open(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            file: OpenOptions::new()
                .open_receiver(path)
                .map(BufReader::new)
                .ok(),
        }
    }

    pub async fn read_protobuf_data(&mut self) -> io::Result<Vec<u8>> {
        loop {
            let file = self.ensure_file()?;
            match Self::try_read_protobuf_data(file).await {
                Ok(res) => return Ok(res),
                Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
                    self.file = None;
                    log::debug!("Unexpected EOF");
                }
                Err(err) => return Err(err),
            }
        }
    }

    // [impl->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
    async fn try_read_protobuf_data(file: &mut BufReader<Receiver>) -> Result<Vec<u8>, Error> {
        let varint_data = Self::try_read_varint_data(file).await?;
        let mut varint_data = Box::new(&varint_data[..]);

        let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

        let mut buf = vec![0; size];
        file.read_exact(&mut buf[..]).await?;
        Ok(buf)
    }

    async fn try_read_varint_data(
        file: &mut BufReader<Receiver>,
    ) -> Result<[u8; Self::MAX_VARINT_SIZE], Error> {
        let mut res = [0u8; Self::MAX_VARINT_SIZE];
        for item in res.iter_mut() {
            *item = file.read_u8().await?;
            if *item & 0b10000000 == 0 {
                break;
            }
        }
        Ok(res)
    }

    fn ensure_file(&mut self) -> io::Result<&mut BufReader<Receiver>> {
        if self.file.is_none() {
            self.file = Some(
                OpenOptions::new()
                    .open_receiver(&self.path)
                    .map(BufReader::new)?,
            );
        };

        if let Some(file) = &mut self.file {
            Ok(file)
        } else {
            unreachable!()
        }
    }
}

#[cfg(test)]
mockall::mock! {
    pub InputPipe {
        pub fn open(path: &Path) -> Self;
        pub fn create(path: &Path) -> Self;
        pub async fn read_protobuf_data(&mut self) -> io::Result<Vec<u8>>;
        async fn try_read_protobuf_data(file: &mut BufReader<Receiver>) -> Result<Vec<u8>, Error>;
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
    use std::{io::ErrorKind, time::Duration};

    use nix::{sys::stat::Mode, unistd::mkfifo};
    use tokio::io::AsyncWriteExt;

    const TEST_TIMEOUT: u64 = 50;

    // [utest->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_1byte_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let mut f = super::InputPipe::open(&fifo);

        let jh = tokio::spawn(async move {
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17]);
        });

        let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
        let v = vec![1, 17];
        f.write_all(&v).await.unwrap();
        f.flush().await.unwrap();

        jh.await.unwrap();
    }

    // [utest->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_2byte_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let mut f = super::InputPipe::open(&fifo);

        let jh = tokio::spawn(async move {
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17; 128]);
        });

        let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
        let mut data = vec![0b10000000, 1];
        data.append(&mut vec![17; 128]);
        f.write_all(&data).await.unwrap();
        f.flush().await.unwrap();

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_too_less_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let mut f = super::InputPipe::open(&fifo);

        let jh = tokio::spawn(async move {
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17]);
        });

        {
            let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
            let data = vec![0b10000000];
            f.write_all(&data).await.unwrap();
            f.flush().await.unwrap();
        }
        std::thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        {
            let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
            let data = vec![1, 17];
            f.write_all(&data).await.unwrap();
            f.flush().await.unwrap();
        }

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_too_less_data() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let mut f = super::InputPipe::open(&fifo);

        let jh = tokio::spawn(async move {
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17]);
        });

        {
            let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
            let data = vec![2, 13];
            f.write_all(&data).await.unwrap();
            f.flush().await.unwrap();
        }
        std::thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        {
            let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
            let data = vec![1, 17];
            f.write_all(&data).await.unwrap();
            f.flush().await.unwrap();
        }

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_incorrect_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let mut f = super::InputPipe::open(&fifo);

        let jh = tokio::spawn(async move {
            let data = f.read_protobuf_data().await;
            assert!(data.is_err());
            assert_eq!(data.unwrap_err().kind(), ErrorKind::InvalidData);
        });

        {
            let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
            let data = vec![0b10000000; super::InputPipe::MAX_VARINT_SIZE];
            f.write_all(&data).await.unwrap();
            f.flush().await.unwrap();
        }

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_empty() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let mut f = super::InputPipe::open(&fifo);

        let jh = tokio::spawn(async move {
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, Vec::<u8>::new());
        });

        let mut f = super::OpenOptions::new().open_sender(&fifo).unwrap();
        let v = vec![0];
        f.write_all(&v).await.unwrap();
        f.flush().await.unwrap();

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_read_cannot_open() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        //This should fail as the file does not exist
        let mut f = super::InputPipe::open(&fifo);
        assert!(f.read_protobuf_data().await.is_err());
    }

    #[tokio::test]
    async fn test_read_cannot_read() {
        let tmpdir = tempfile::tempdir().unwrap();
        let mut f = super::InputPipe::open(tmpdir.path());
        assert!(f.read_protobuf_data().await.is_err());
    }
}

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

use std::{
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncReadExt, AsyncWriteExt, BufReader},
    task::JoinHandle,
};

#[derive(Debug)]
pub struct ReopenFile {
    open_options: OpenOptions,
    path: PathBuf,
    file: Option<BufReader<File>>,
    first_file: Option<JoinHandle<io::Result<File>>>,
}

impl ReopenFile {
    const MAX_VARINT_SIZE: usize = 19;

    pub fn open(path: &Path) -> Self {
        let mut open_options = OpenOptions::new();
        open_options.read(true);
        let first_file = Self::get_next_file(&open_options, path);
        Self {
            open_options,
            path: path.to_path_buf(),
            file: None,
            first_file: Some(first_file),
        }
    }

    pub fn create(path: &Path) -> Self {
        let mut open_options = OpenOptions::new();
        open_options.write(true).create(true).truncate(true);
        let first_file = Self::get_next_file(&open_options, path);
        Self {
            open_options,
            path: path.to_path_buf(),
            file: None,
            first_file: Some(first_file),
        }
    }

    fn get_next_file(
        open_options: &OpenOptions,
        path: impl AsRef<Path>,
    ) -> JoinHandle<io::Result<File>> {
        let path = path.as_ref().to_owned();
        let open_options = open_options.to_owned();
        tokio::spawn(async move { open_options.open(path).await })
    }

    pub async fn read_protobuf_data(&mut self) -> io::Result<Vec<u8>> {
        loop {
            let file = self.ensure_file().await?;
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
    async fn try_read_protobuf_data(file: &mut BufReader<File>) -> Result<Vec<u8>, Error> {
        let varint_data = Self::try_read_varint_data(file).await?;
        let mut varint_data = Box::new(&varint_data[..]);

        let size = prost::encoding::decode_varint(&mut varint_data)? as usize;

        let mut buf = vec![0; size];
        file.read_exact(&mut buf[..]).await?;
        Ok(buf)
    }

    async fn try_read_varint_data(
        file: &mut BufReader<File>,
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

    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        loop {
            match self.try_write_all(buf).await {
                Ok(()) => return Ok(()),
                Err(err) if err.kind() == ErrorKind::BrokenPipe => {
                    self.file = None;
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn try_write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        let file = self.ensure_file().await?;
        file.write_all(buf).await?;
        file.flush().await?;
        Ok(())
    }

    async fn ensure_file(&mut self) -> io::Result<&mut BufReader<File>> {
        if self.file.is_none() {
            let file = if let Some(first_file) = &mut self.first_file {
                let first_file = first_file.await?;
                self.first_file = None;
                first_file?
            } else {
                self.open_options.open(&self.path).await?
            };
            let buf_reader = BufReader::new(file);
            self.file = Some(buf_reader);
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
    pub ReopenFile {
        pub fn open(path: &Path) -> Self;
        pub fn create(path: &Path) -> Self;
        pub async fn read_protobuf_data(&mut self) -> io::Result<Vec<u8>>;
        async fn try_read_protobuf_data(file: &mut BufReader<File>) -> Result<Vec<u8>, Error>;
        pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()>;
        async fn try_write_all(&mut self, buf: &[u8]) -> io::Result<()>;
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
    use std::{
        io::{ErrorKind, Write},
        path::Path,
        sync::Arc,
        time::Duration,
    };

    use nix::{sys::stat::Mode, unistd::mkfifo};
    use tokio::{io::AsyncReadExt, sync::Barrier};

    const TEST_TIMEOUT: u64 = 50;

    // [utest->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_1byte_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();
        let jh = tokio::spawn(async move {
            let mut f = super::ReopenFile::open(&fifo2);
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17]);
        });

        let mut f = std::fs::File::create(&fifo).unwrap();
        let v = vec![1, 17];
        f.write_all(&v).unwrap();
        f.flush().unwrap();

        jh.await.unwrap();
    }

    // [utest->swdd~agent-uses-length-delimited-protobuf-for-pipes~1]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_2byte_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();
        let jh = tokio::spawn(async move {
            let mut f = super::ReopenFile::open(&fifo2);
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17; 128]);
        });

        let mut f = std::fs::File::create(&fifo).unwrap();
        let mut data = vec![0b10000000, 1];
        data.append(&mut vec![17; 128]);
        f.write_all(&data).unwrap();
        f.flush().unwrap();

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_too_less_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let jh = tokio::spawn(async move {
            let mut f = super::ReopenFile::open(&fifo2);
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17]);
        });

        {
            let mut f = std::fs::File::create(&fifo).unwrap();
            let data = vec![0b10000000];
            f.write_all(&data).unwrap();
            f.flush().unwrap();
        }
        std::thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        {
            let mut f = std::fs::File::create(&fifo).unwrap();
            let data = vec![1, 17];
            f.write_all(&data).unwrap();
            f.flush().unwrap();
        }

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_too_less_data() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let jh = tokio::spawn(async move {
            let mut f = super::ReopenFile::open(&fifo2);
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, vec![17]);
        });

        {
            let mut f = std::fs::File::create(&fifo).unwrap();
            let data = vec![2, 13];
            f.write_all(&data).unwrap();
            f.flush().unwrap();
        }
        std::thread::sleep(Duration::from_millis(TEST_TIMEOUT));
        {
            let mut f = std::fs::File::create(&fifo).unwrap();
            let data = vec![1, 17];
            f.write_all(&data).unwrap();
            f.flush().unwrap();
        }

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_with_incorrect_varint() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let jh = tokio::spawn(async move {
            let mut f = super::ReopenFile::open(&fifo2);
            let data = f.read_protobuf_data().await;
            assert!(data.is_err());
            assert_eq!(data.unwrap_err().kind(), ErrorKind::InvalidData);
        });

        {
            let mut f = std::fs::File::create(&fifo).unwrap();
            let data = vec![0b10000000; super::ReopenFile::MAX_VARINT_SIZE];
            f.write_all(&data).unwrap();
            f.flush().unwrap();
        }

        jh.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_read_empty() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();
        let jh = tokio::spawn(async move {
            let mut f = super::ReopenFile::open(&fifo2);
            let data = f.read_protobuf_data().await.unwrap();
            assert_eq!(data, Vec::<u8>::new());
        });

        let mut f = std::fs::File::create(&fifo).unwrap();
        let v = vec![0];
        f.write_all(&v).unwrap();
        f.flush().unwrap();

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_read_cannot_open() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        //This should fail as the file does not exist
        let mut f = super::ReopenFile::open(&fifo);
        assert!(f.read_protobuf_data().await.is_err());
    }

    #[tokio::test]
    async fn test_read_cannot_read() {
        let tmpdir = tempfile::tempdir().unwrap();
        let mut f = super::ReopenFile::open(tmpdir.path());
        assert!(f.read_protobuf_data().await.is_err());
    }

    #[tokio::test]
    async fn test_write_reopen() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let barrier1 = Arc::new(Barrier::new(2));
        let barrier2 = barrier1.clone();

        let jh = tokio::spawn(async move {
            let mut f = super::ReopenFile::create(&fifo2);
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            f.write_all(&[1, 2, 3]).await.unwrap();
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            f.write_all(&[4, 5, 6, 7, 8]).await.unwrap();
        });
        {
            let mut f = super::File::open(&fifo).await.unwrap();
            barrier2.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            let mut buf = [0; 64];
            let s = f.read(&mut buf).await.unwrap();
            assert_eq!(s, 3);
            assert_eq!(buf[0..3], vec![1, 2, 3]);
        }
        {
            let mut f = super::File::open(&fifo).await.unwrap();
            barrier2.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            let mut buf = [0; 64];
            let s = f.read(&mut buf).await.unwrap();
            assert_eq!(s, 5);
            assert_eq!(buf[0..5], vec![4, 5, 6, 7, 8]);
        }

        jh.await.unwrap();
    }

    #[tokio::test]
    async fn test_write_empty() {
        let mut f = super::ReopenFile::open(Path::new(""));
        assert!(f.write_all(&[]).await.is_ok());
    }

    #[tokio::test]
    async fn test_write_cannot_open() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        //This should fail as the file does not exist
        let mut f = super::ReopenFile::open(&fifo);
        assert!(f.write_all(&[1, 2, 3]).await.is_err());
    }
}

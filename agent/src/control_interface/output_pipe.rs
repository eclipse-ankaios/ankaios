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
    io::ErrorKind,
    path::{Path, PathBuf},
};

use tokio::{
    io::{self, AsyncWriteExt},
    net::unix::pipe::{OpenOptions, Sender},
};

#[derive(Debug)]
pub struct OutputPipe {
    path: PathBuf,
    file: Option<Sender>,
}

impl OutputPipe {
    pub fn open(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            file: OpenOptions::new().open_sender(path).ok(),
        }
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
        let file: &mut Sender = self.ensure_file()?;
        file.write_all(buf).await?;
        file.flush().await?;
        Ok(())
    }

    fn ensure_file(&mut self) -> io::Result<&mut Sender> {
        if self.file.is_none() {
            self.file = Some(OpenOptions::new().open_sender(&self.path)?);
        }

        if let Some(file) = &mut self.file {
            Ok(file)
        } else {
            unreachable!()
        }
    }
}

#[cfg(test)]
mockall::mock! {
    pub OutputPipe {
        pub fn open(path: &Path) -> Self;
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
    use std::{path::Path, sync::Arc};

    use nix::{sys::stat::Mode, unistd::mkfifo};
    use tokio::{io::AsyncReadExt, sync::Barrier};

    #[tokio::test]
    async fn test_write_reopen() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let barrier1 = Arc::new(Barrier::new(2));
        let barrier2 = barrier1.clone();

        let jh = tokio::spawn(async move {
            let mut f = super::OutputPipe::open(&fifo2);
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            f.write_all(&[1, 2, 3]).await.unwrap();
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            f.write_all(&[4, 5, 6, 7, 8]).await.unwrap();
        });
        {
            let mut f = super::OpenOptions::new().open_receiver(&fifo).unwrap();
            // let mut f = File::open(&fifo).await.unwrap();
            barrier2.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            let mut buf = [0; 64];
            let s = f.read(&mut buf).await.unwrap();
            assert_eq!(s, 3);
            assert_eq!(buf[0..3], vec![1, 2, 3]);
        }
        {
            let mut f = super::OpenOptions::new().open_receiver(&fifo).unwrap();
            // let mut f = File::open(&fifo).await.unwrap();
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
        let mut f = super::OutputPipe::open(Path::new(""));
        assert!(f.write_all(&[]).await.is_ok());
    }

    #[tokio::test]
    async fn test_write_cannot_open() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        //This should fail as the file does not exist
        let mut f = super::OutputPipe::open(&fifo);
        assert!(f.write_all(&[1, 2, 3]).await.is_err());
    }
}

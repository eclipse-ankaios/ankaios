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
    time::Duration,
};

use tokio::{
    io::{self, AsyncWriteExt},
    net::unix::pipe::{OpenOptions, Sender},
    time::sleep,
};

const AGENT_RECONNECT_INTERVAL_MS: u64 = 100;
const OUTPUT_PIPE_WRITE_TIMEOUT_MS: u64 = 500;
const CONTROL_INTERFACE_MAX_RETRIES: u8 = 5;

#[derive(Debug)]
pub struct OutputPipe {
    path: PathBuf,
    file: Option<Sender>,
}

#[derive(Debug)]
pub enum OutputPipeError {
    ReceiverGone(io::Error),
    Io(io::Error),
}

impl From<OutputPipeError> for io::Error {
    fn from(err: OutputPipeError) -> Self {
        match err {
            OutputPipeError::ReceiverGone(e) => e,
            OutputPipeError::Io(e) => e,
        }
    }
}

impl OutputPipe {
    pub fn open(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            file: OpenOptions::new().open_sender(path).ok(),
        }
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), OutputPipeError> {
        if buf.is_empty() {
            return Ok(());
        }

        let mut retries = 0;
        loop {
            // [impl->swdd~agent-handles-control-interface-full-output-pipe-buffer~1]
            let write_result = tokio::time::timeout(
                std::time::Duration::from_millis(OUTPUT_PIPE_WRITE_TIMEOUT_MS),
                self.try_write_all(buf),
            )
            .await
            .map_err(|err| {
                OutputPipeError::ReceiverGone(io::Error::new(ErrorKind::TimedOut, err))
            })?;
            match write_result {
                Ok(()) => {
                    log::trace!("Writing done successfully");
                    return Ok(());
                }
                // [impl->swdd~agent-handles-control-interface-output-pipe-closed~1]
                Err(err) if Self::receiver_gone(&err) => {
                    if retries < CONTROL_INTERFACE_MAX_RETRIES {
                        self.file = None;
                        log::debug!("Broken pipe - the receiver is gone. Waiting for 'AGENT_RECONNECT_INTERVAL'ms before trying again.");
                        sleep(Duration::from_millis(AGENT_RECONNECT_INTERVAL_MS)).await;
                    } else {
                        log::warn!("Failed to write to output pipe after multiple attempts");
                        return Err(OutputPipeError::ReceiverGone(err));
                    }
                    retries += 1;
                }
                Err(err) => {
                    log::warn!("Writing failed with error: {:?}", err);
                    return Err(OutputPipeError::Io(err));
                }
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
            log::debug!("Attempting to reopen the output pipe at {:?}", self.path);
            self.file = Some(OpenOptions::new().open_sender(&self.path)?);
        }

        if let Some(file) = &mut self.file {
            Ok(file)
        } else {
            unreachable!()
        }
    }

    // [impl->swdd~agent-handles-control-interface-output-pipe-closed~1]
    fn receiver_gone(err: &io::Error) -> bool {
        // occurs when trying to write to a pipe that has no reader
        err.kind() == ErrorKind::BrokenPipe
        // occurs when trying to open the pipe for writing, but it is not open for reading
        || err.raw_os_error() == Some(nix::libc::ENXIO)
    }
}

#[cfg(test)]
mockall::mock! {
    pub OutputPipe {
        pub fn open(path: &Path) -> Self;
        pub async fn write_all(&mut self, buf: &[u8]) -> Result<(), OutputPipeError>;
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
    use std::{io::ErrorKind, path::Path, sync::Arc};

    use nix::{sys::stat::Mode, unistd::mkfifo};
    use tokio::{io::AsyncReadExt, sync::Barrier};

    // [utest->swdd~agent-handles-control-interface-output-pipe-closed~1]
    #[test]
    fn test_receiver_gone() {
        let err = std::io::Error::from_raw_os_error(nix::libc::ENXIO);
        assert!(super::OutputPipe::receiver_gone(&err));

        let err = std::io::Error::from(ErrorKind::BrokenPipe);
        assert!(super::OutputPipe::receiver_gone(&err));

        let err = std::io::Error::from(ErrorKind::Other);
        assert!(!super::OutputPipe::receiver_gone(&err));
    }

    #[test]
    fn test_from_output_pipe_error() {
        let err = super::OutputPipeError::ReceiverGone(std::io::Error::from(ErrorKind::BrokenPipe));
        let io_err: std::io::Error = err.into();
        assert_eq!(io_err.kind(), ErrorKind::BrokenPipe);

        let err = super::OutputPipeError::Io(std::io::Error::from(ErrorKind::Other));
        let io_err: std::io::Error = err.into();
        assert_eq!(io_err.kind(), ErrorKind::Other);
    }

    #[tokio::test]
    async fn test_write_reopen() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let barrier1 = Arc::new(Barrier::new(2));
        let barrier2 = barrier1.clone();

        let writing_task = tokio::spawn(async move {
            let mut writing_side = super::OutputPipe::open(&fifo2);
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            writing_side.write_all(&[1, 2, 3]).await.unwrap();
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            writing_side.write_all(&[4, 5, 6, 7, 8]).await.unwrap();
        });

        let mut reading_side = super::OpenOptions::new().open_receiver(&fifo).unwrap();
        barrier2.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
        let mut buf = [0; 64];
        let read_count = reading_side.read(&mut buf).await.unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buf[0..3], vec![1, 2, 3]);
        drop(reading_side); // close the reading side to simulate that the receiver is gone

        let mut reading_side = super::OpenOptions::new().open_receiver(&fifo).unwrap();
        barrier2.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
        let mut buf = [0; 64];
        let read_count = reading_side.read(&mut buf).await.unwrap();
        assert_eq!(read_count, 5);
        assert_eq!(buf[0..5], vec![4, 5, 6, 7, 8]);
        drop(reading_side); // close the reading side to simulate that the receiver is gone

        writing_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_write_empty() {
        let mut writing_side = super::OutputPipe::open(Path::new(""));
        assert!(writing_side.write_all(&[]).await.is_ok());
    }

    #[tokio::test]
    async fn test_write_cannot_open() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        //This should fail as the file does not exist
        let mut writing_side = super::OutputPipe::open(&fifo);
        assert!(writing_side.write_all(&[1, 2, 3]).await.is_err());
    }

    // [utest->swdd~agent-handles-control-interface-output-pipe-closed~1]
    #[tokio::test]
    async fn test_write_when_receiver_gone() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let barrier1 = Arc::new(Barrier::new(2));
        let barrier2 = barrier1.clone();

        let writing_task = tokio::spawn(async move {
            let mut writing_side = super::OutputPipe::open(&fifo2);
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            writing_side.write_all(&[1, 2, 3]).await.unwrap();
            barrier1.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
            let result = writing_side.write_all(&[4, 5, 6, 7, 8]).await;
            assert!(matches!(
                result.unwrap_err(),
                super::OutputPipeError::ReceiverGone(_)
            ));
        });

        let mut reading_side = super::OpenOptions::new().open_receiver(&fifo).unwrap();
        barrier2.wait().await; // synchronize that both ends of the fifo file is open for writing and reading
        let mut buf = [0; 64];
        let read_count = reading_side.read(&mut buf).await.unwrap();
        assert_eq!(read_count, 3);
        assert_eq!(buf[0..3], vec![1, 2, 3]);
        drop(reading_side); // close the reading side to simulate that the receiver is gone

        // Don't open another reader to simulate a problem in the workload
        barrier2.wait().await; // synchronize that both ends of the fifo file is open for writing and reading

        writing_task.await.unwrap();
    }

    // The test test_write_when_receiver_gone_reconnect is explicitly omitted here as the effort is unreasonable

    // [utest->swdd~agent-handles-control-interface-full-output-pipe-buffer~1]
    #[tokio::test]
    async fn test_write_when_receiver_opens_but_does_not_read() {
        let tmpdir = tempfile::tempdir().unwrap();
        let fifo = tmpdir.path().join("fifo");
        mkfifo(&fifo, Mode::S_IRWXU).unwrap();
        let fifo2 = fifo.clone();

        let _reading_side = super::OpenOptions::new().open_receiver(&fifo).unwrap();

        let mut writing_side = super::OutputPipe::open(&fifo2);

        // With the current buffer the loop would run for about 2000 iterations
        for i in 0..=2200 {
            match tokio::time::timeout(
                std::time::Duration::from_millis(10),
                writing_side.write_all(&[
                    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                    23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
                    43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62,
                    63, 64,
                ]),
            )
            .await
            {
                Ok(_) => {
                    // wrote successfully, just continue until it fails...
                }
                Err(_) => {
                    // We should exit here at some point, as the writing should fail after a while
                    assert!(
                        i >= 1024,
                        "Writing timed out before we were able to write at least 64k. Iteration: {}",
                        i
                    );
                    return;
                }
            }
        }

        panic!("The writing should have failed after 2200 iterations, but it did not.");
    }
}

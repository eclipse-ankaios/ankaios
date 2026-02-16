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

use super::{
    log_channel,
    log_fetcher::{self, LogFetcher},
};

#[cfg(test)]
use tests::{JoinHandle, spawn};
#[cfg(not(test))]
use tokio::task::{JoinHandle, spawn};

pub struct LogFetchingRunner {
    join_handles: Vec<JoinHandle<()>>,
}

// [impl->swdd~agent-log-fetching-runs-log-fetchers~1]
impl LogFetchingRunner {
    pub fn start_collecting_logs(
        log_fetchers: Vec<Box<dyn LogFetcher + 'static>>,
    ) -> (Self, Vec<log_channel::Receiver>) {
        let (join_handles, receivers) = log_fetchers
            .into_iter()
            .map(|x| {
                let (sender, receiver) = log_channel::channel();
                let jh = spawn(async move {
                    log_fetcher::run(x, sender).await;
                });
                (jh, receiver)
            })
            .unzip();

        (Self { join_handles }, receivers)
    }
}

// [impl->swdd~agent-log-fetching-stops-collection-when-dropped~1]
impl Drop for LogFetchingRunner {
    fn drop(&mut self) {
        self.join_handles.iter().for_each(|x| x.abort());
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
    use crate::runtime_connectors::{
        log_fetcher::{MockLogFetcher, NextLinesResult},
        log_fetching_runner::LogFetchingRunner,
    };

    use lazy_static::lazy_static;
    use std::{
        future::Future,
        sync::{Arc, Mutex},
    };
    use tokio::{self, sync::Mutex as AsyncMutex};

    lazy_static! {
        static ref SPAWN_JOIN_HANDLE: Mutex<Vec<Box<dyn TypelessJoinHandle>>> =
            Mutex::new(Vec::new());
    }

    const FETCHER_1_LINE_1: &str = "fetcher 1: line 1";
    const FETCHER_1_LINE_2: &str = "fetcher 1: line 2";
    const FETCHER_1_LINE_3: &str = "fetcher 1: line 3";
    const FETCHER_2_LINE_1: &str = "fetcher 2: line 1";
    const FETCHER_2_LINE_2: &str = "fetcher 2: line 2";
    const FETCHER_2_LINE_3: &str = "fetcher 2: line 3";
    const FETCHER_2_LINE_4: &str = "fetcher 2: line 4";

    // [utest->swdd~agent-log-fetching-runs-log-fetchers~1]
    #[tokio::test]
    async fn utest_log_fetching_runner_forwards_logs() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        clear_join_handles();

        let log_fetcher_1 =
            create_mock_log_fetcher(&[&[FETCHER_1_LINE_1, FETCHER_1_LINE_2], &[FETCHER_1_LINE_3]]);

        let log_fetcher_2 = create_mock_log_fetcher(&[
            &[FETCHER_2_LINE_1],
            &[FETCHER_2_LINE_2, FETCHER_2_LINE_3, FETCHER_2_LINE_4],
        ]);

        let (_runner, mut receivers) = LogFetchingRunner::start_collecting_logs(vec![
            Box::new(log_fetcher_1),
            Box::new(log_fetcher_2),
        ]);

        assert_eq!(receivers.len(), 2);
        assert_eq!(
            receivers[0].read_log_lines().await,
            Some(vec![FETCHER_1_LINE_1.into(), FETCHER_1_LINE_2.into()])
        );
        assert_eq!(
            receivers[0].read_log_lines().await,
            Some(vec![FETCHER_1_LINE_3.into()])
        );
        assert_eq!(receivers[0].read_log_lines().await, None);
        assert_eq!(
            receivers[1].read_log_lines().await,
            Some(vec![FETCHER_2_LINE_1.into()])
        );
        assert_eq!(
            receivers[1].read_log_lines().await,
            Some(vec![
                FETCHER_2_LINE_2.into(),
                FETCHER_2_LINE_3.into(),
                FETCHER_2_LINE_4.into()
            ])
        );
        assert_eq!(receivers[1].read_log_lines().await, None);
    }

    // [utest->swdd~agent-log-fetching-stops-collection-when-dropped~1]
    #[tokio::test]
    async fn utest_log_fetching_runner_abort_task_on_drop() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        clear_join_handles();

        let log_fetcher_1 =
            create_mock_log_fetcher(&[&[FETCHER_1_LINE_1, FETCHER_1_LINE_2], &[FETCHER_1_LINE_3]]);

        let log_fetcher_2 = create_mock_log_fetcher(&[
            &[FETCHER_2_LINE_1],
            &[FETCHER_2_LINE_2, FETCHER_2_LINE_3, FETCHER_2_LINE_4],
        ]);

        let (runner, mut receivers) = LogFetchingRunner::start_collecting_logs(vec![
            Box::new(log_fetcher_1),
            Box::new(log_fetcher_2),
        ]);

        assert!(!check_all_aborted());

        // Read all logs from receivers to unblock the fetcher tasks
        for receiver in &mut receivers {
            while receiver.read_log_lines().await.is_some() {}
        }

        for handle in &runner.join_handles {
            let result = {
                let mut jh_guard = handle.jh.lock().await;
                (&mut *jh_guard).await
            };
            assert!(result.is_ok());
        }
        drop(runner);
        assert!(check_all_aborted());
    }

    fn create_mock_log_fetcher(lines: &[&[&str]]) -> MockLogFetcher {
        let mut log_fetcher = MockLogFetcher::new();
        for &line_package in lines {
            let line_package = line_package.iter().map(|s| s.to_string()).collect();
            log_fetcher
                .expect_next_lines()
                .once()
                .return_once(move || NextLinesResult::Stdout(line_package));
        }
        log_fetcher
            .expect_next_lines()
            .once()
            .return_const(NextLinesResult::EoF);
        log_fetcher
    }

    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let jh = tokio::spawn(future);
        let jh = Arc::new(AsyncMutex::new(jh));
        let jh = JoinHandle {
            jh,
            was_aborted: Arc::new(Mutex::new(false)),
        };
        SPAWN_JOIN_HANDLE.lock().unwrap().push(Box::new(jh.clone()));
        jh
    }

    #[derive(Debug)]
    pub struct JoinHandle<T> {
        jh: Arc<AsyncMutex<tokio::task::JoinHandle<T>>>,
        was_aborted: Arc<Mutex<bool>>,
    }

    impl<T> Clone for JoinHandle<T> {
        fn clone(&self) -> Self {
            Self {
                jh: self.jh.clone(),
                was_aborted: self.was_aborted.clone(),
            }
        }
    }

    impl<T> JoinHandle<T> {
        pub fn abort(&self) {
            *self.was_aborted.lock().unwrap() = true;
            if let Ok(jh) = self.jh.try_lock() {
                jh.abort();
            }
        }
    }

    trait TypelessJoinHandle: Send + Sync {
        fn was_aborted(&self) -> bool;
        fn abort(&self);
    }

    impl<T: Send> TypelessJoinHandle for JoinHandle<T> {
        fn abort(&self) {
            JoinHandle::abort(self);
        }

        fn was_aborted(&self) -> bool {
            *self.was_aborted.lock().unwrap()
        }
    }

    fn clear_join_handles() {
        let mut join_handlers = SPAWN_JOIN_HANDLE.lock().unwrap();
        for x in &*join_handlers {
            x.as_ref().abort();
        }
        *join_handlers = Vec::new();
    }

    fn check_all_aborted() -> bool {
        SPAWN_JOIN_HANDLE
            .lock()
            .unwrap()
            .iter()
            .all(|jh| jh.was_aborted())
    }
}

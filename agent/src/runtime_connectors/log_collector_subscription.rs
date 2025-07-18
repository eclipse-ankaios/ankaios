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

#[cfg(test)]
use tests::{spawn, JoinHandle};
#[cfg(not(test))]
use tokio::task::{spawn, JoinHandle};

use super::{
    log_channel,
    log_picker::{self, LogPicker},
};

pub struct LogPickingRunner {
    join_handles: Vec<JoinHandle<()>>,
}

impl LogPickingRunner {
    pub fn start_collecting_logs(
        log_collectors: Vec<Box<dyn LogPicker + 'static>>,
    ) -> (Self, Vec<log_channel::Receiver>) {
        let (join_handles, receivers) = log_collectors
            .into_iter()
            .map(|x| {
                let (sender, receiver) = log_channel::channel();
                let jh = spawn(async move {
                    log_picker::run(x, sender).await;
                });
                (jh, receiver)
            })
            .unzip();

        (Self { join_handles }, receivers)
    }
}

impl Drop for LogPickingRunner {
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
    use lazy_static::lazy_static;
    use std::{
        future::Future,
        sync::{Arc, Mutex},
    };
    use tokio::{self};

    use crate::runtime_connectors::{
        log_collector_subscription::LogPickingRunner,
        log_picker::{MockLogPicker, NextLinesResult},
    };

    lazy_static! {
        static ref SPAWN_JOIN_HANDLE: Mutex<Vec<Box<dyn TypelessJoinHandle>>> =
            Mutex::new(Vec::new());
    }

    const COLLECTOR_1_LINE_1: &str = "collector 1: line 1";
    const COLLECTOR_1_LINE_2: &str = "collector 1: line 2";
    const COLLECTOR_1_LINE_3: &str = "collector 1: line 3";
    const COLLECTOR_2_LINE_1: &str = "collector 2: line 1";
    const COLLECTOR_2_LINE_2: &str = "collector 2: line 2";
    const COLLECTOR_2_LINE_3: &str = "collector 2: line 3";
    const COLLECTOR_2_LINE_4: &str = "collector 2: line 4";

    #[tokio::test]
    async fn utest_log_collector_subscription_forwards_logs() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        clear_join_handles();

        let log_collector_1 = create_mock_log_collector(&[
            &[COLLECTOR_1_LINE_1, COLLECTOR_1_LINE_2],
            &[COLLECTOR_1_LINE_3],
        ]);

        let log_collector_2 = create_mock_log_collector(&[
            &[COLLECTOR_2_LINE_1],
            &[COLLECTOR_2_LINE_2, COLLECTOR_2_LINE_3, COLLECTOR_2_LINE_4],
        ]);

        let (_subscription, mut receivers) = LogPickingRunner::start_collecting_logs(vec![
            Box::new(log_collector_1),
            Box::new(log_collector_2),
        ]);

        assert_eq!(receivers.len(), 2);
        assert_eq!(
            receivers[0].read_log_lines().await,
            Some(vec![COLLECTOR_1_LINE_1.into(), COLLECTOR_1_LINE_2.into()])
        );
        assert_eq!(
            receivers[0].read_log_lines().await,
            Some(vec![COLLECTOR_1_LINE_3.into()])
        );
        assert_eq!(receivers[0].read_log_lines().await, None);
        assert_eq!(
            receivers[1].read_log_lines().await,
            Some(vec![COLLECTOR_2_LINE_1.into()])
        );
        assert_eq!(
            receivers[1].read_log_lines().await,
            Some(vec![
                COLLECTOR_2_LINE_2.into(),
                COLLECTOR_2_LINE_3.into(),
                COLLECTOR_2_LINE_4.into()
            ])
        );
        assert_eq!(receivers[1].read_log_lines().await, None);
    }

    #[tokio::test]
    async fn utest_log_collector_subscription_abort_task_on_drop() {
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;
        clear_join_handles();

        let log_collector_1 = create_mock_log_collector(&[
            &[COLLECTOR_1_LINE_1, COLLECTOR_1_LINE_2],
            &[COLLECTOR_1_LINE_3],
        ]);

        let log_collector_2 = create_mock_log_collector(&[
            &[COLLECTOR_2_LINE_1],
            &[COLLECTOR_2_LINE_2, COLLECTOR_2_LINE_3, COLLECTOR_2_LINE_4],
        ]);

        let (subscription, mut _receivers) = LogPickingRunner::start_collecting_logs(vec![
            Box::new(log_collector_1),
            Box::new(log_collector_2),
        ]);

        assert!(!check_all_aborted());
        drop(subscription);
        assert!(check_all_aborted());
    }

    fn create_mock_log_collector(lines: &[&[&str]]) -> MockLogPicker {
        let mut log_collector = MockLogPicker::new();
        for &line_package in lines {
            let line_package = line_package.iter().map(|s| s.to_string()).collect();
            log_collector
                .expect_next_lines()
                .once()
                .return_once(move || NextLinesResult::Stdout(line_package));
        }
        log_collector
            .expect_next_lines()
            .once()
            .return_const(NextLinesResult::EoF);
        log_collector
    }

    pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let jh = tokio::spawn(future);
        let jh = Arc::new(Mutex::new(jh));
        let jh = JoinHandle {
            jh,
            was_aborted: Arc::new(Mutex::new(false)),
        };
        SPAWN_JOIN_HANDLE.lock().unwrap().push(Box::new(jh.clone()));
        jh
    }

    #[derive(Debug)]
    pub struct JoinHandle<T> {
        jh: Arc<Mutex<tokio::task::JoinHandle<T>>>,
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
            self.jh.lock().unwrap().abort();
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

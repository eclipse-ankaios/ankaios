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

use api::ank_base;
use common::commands::LogsRequest;
use common::objects::WorkloadInstanceName;
use common::std_extensions::IllegalStateResult;
use common::to_server_interface::{ToServerInterface, ToServerSender};
use futures_util::{stream::FuturesUnordered, StreamExt};
use std::{future::Future, pin::Pin};

use crate::agent_manager::SynchronizedSubscriptionStore;

#[cfg(not(test))]
use crate::runtime_connectors::log_channel::Receiver;
#[cfg(test)]
use tests::MockRuntimeConnectorReceiver as Receiver;

#[cfg(not(test))]
use crate::runtime_connectors::log_collector_subscription::LogCollectorSubscription;
#[cfg(test)]
use tests::MockLogCollectorSubscription as LogCollectorSubscription;

#[cfg_attr(test, mockall_double::double)]
use crate::runtime_manager::RuntimeManager;

#[cfg(test)]
use mockall::automock;

#[cfg(not(test))]
use crate::subscription_store::SubscriptionEntry;

#[cfg(test)]
use crate::subscription_store::MockSubscriptionEntry as SubscriptionEntry;

pub struct WorkloadLogFacade;

type ContinuableResult = (WorkloadInstanceName, Receiver, Option<Vec<String>>);
type UnorderedLogReceiverFutures =
    FuturesUnordered<Pin<Box<dyn Future<Output = ContinuableResult> + Send>>>;

#[cfg_attr(test, automock)]
impl WorkloadLogFacade {
    // [impl->swdd~workload-log-facade-starts-log-collection-campaign~1]
    pub async fn spawn_log_collection(
        request_id: String,
        logs_request: LogsRequest,
        to_server: ToServerSender,
        synchronized_subscription_store: SynchronizedSubscriptionStore,
        runtime_manager: &RuntimeManager,
    ) {
        let (names, log_collectors): (Vec<_>, _) = runtime_manager
            .get_logs(logs_request)
            .await
            .into_iter()
            .unzip();
        let (subscription, receivers) =
            LogCollectorSubscription::start_collecting_logs(log_collectors);
        let receivers = names.into_iter().zip(receivers).collect::<Vec<_>>();
        let cloned_request_id = request_id.clone();
        let subscription_store = synchronized_subscription_store.clone();

        let log_collection_join_handle = tokio::spawn(async move {
            let _subscription = subscription;
            let futures = Self::convert_log_receivers_to_futures(receivers);

            // [impl->swdd~workload-log-facade-forwards-logs-to-server~1]
            Self::consume_futures_and_forward_logs_until_stop(
                futures,
                cloned_request_id.clone(),
                &to_server,
            )
            .await;

            // [impl->swdd~workload-log-facade-automatically-unsubscribes-log-subscriptions~1]
            subscription_store
                .lock()
                .unwrap()
                .delete_subscription(&cloned_request_id);
            log::debug!("Log collection for request '{}' finished. Subscription has been deleted successfully. ", cloned_request_id);
        });

        synchronized_subscription_store
            .lock()
            .unwrap()
            .add_subscription(
                request_id,
                SubscriptionEntry::new(log_collection_join_handle),
            );
    }

    fn convert_log_receivers_to_futures(
        receivers: Vec<(WorkloadInstanceName, Receiver)>,
    ) -> UnorderedLogReceiverFutures {
        FuturesUnordered::from_iter(receivers.into_iter().map(
            |workload_log_info| -> Pin<Box<dyn Future<Output = ContinuableResult> + Send>> {
                let workload_instance_name = workload_log_info.0;
                let mut log_receiver = workload_log_info.1;

                Box::pin(async {
                    let received_log_lines = log_receiver.read_log_lines().await;
                    (workload_instance_name, log_receiver, received_log_lines)
                })
            },
        ))
    }

    // [impl->swdd~workload-log-facade-forwards-logs-to-server~1]
    async fn consume_futures_and_forward_logs_until_stop(
        mut log_futures: UnorderedLogReceiverFutures,
        request_id: String,
        to_server: &ToServerSender,
    ) {
        while let Some((workload_instance_name, mut receiver, log_lines)) = log_futures.next().await
        {
            log::debug!("Got new log lines: {:?}", log_lines);
            if let Some(log_lines) = log_lines {
                to_server
                    .log_entries_response(
                        request_id.clone(),
                        ank_base::LogEntriesResponse {
                            log_entries: log_lines
                                .into_iter()
                                .map(|log_message| ank_base::LogEntry {
                                    workload_name: Some(workload_instance_name.clone().into()),
                                    message: log_message,
                                })
                                .collect(),
                        },
                    )
                    .await
                    .unwrap_or_illegal_state();

                let workload_log_info = async move {
                    let received_log_lines = receiver.read_log_lines().await;
                    (workload_instance_name, receiver, received_log_lines)
                };
                log_futures.push(Box::pin(workload_log_info));
            } else {
                // [impl->swdd~workload-log-facade-sends-logs-stop-response~1]
                log::debug!(
                    "No more log lines available for workload '{}', sending logs stop response.",
                    workload_instance_name
                );
                to_server
                    .logs_stop_response(
                        request_id.clone(),
                        ank_base::LogsStopResponse {
                            workload_name: Some(workload_instance_name.into()),
                        },
                    )
                    .await
                    .unwrap_or_illegal_state();
            }
        }
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
    use super::SynchronizedSubscriptionStore;
    use crate::runtime_connectors::log_collector::MockLogCollector;
    use crate::runtime_manager::MockRuntimeManager;
    use crate::subscription_store::{MockJoinHandle, MockSubscriptionEntry, SubscriptionEntry};
    use crate::workload_log_facade::WorkloadLogFacade;
    use api::ank_base;
    use common::to_server_interface::ToServer;
    use mockall::{mock, predicate};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::{sync::mpsc, sync::mpsc::channel, time::timeout};

    const BUFFER_SIZE: usize = 20;
    const AGENT_NAME: &str = "agent_x";
    const WORKLOAD_1_NAME: &str = "workload1";
    const WORKLOAD_2_NAME: &str = "workload2";
    const REQUEST_ID: &str = "request_id";

    async fn get_log_responses(
        num: usize,
        to_server: &mut mpsc::Receiver<ToServer>,
    ) -> Option<HashMap<(String, String), Vec<String>>> {
        let mut result: HashMap<(String, String), Vec<String>> = HashMap::new();
        let mut responses = 0;
        while responses != num {
            let candidate = to_server.recv().await?;
            if let ToServer::LogEntriesResponse(request_id, logs_response) = candidate {
                responses += 1;
                for entry in logs_response.log_entries {
                    result
                        .entry((
                            request_id.clone(),
                            entry.workload_name.unwrap().workload_name,
                        ))
                        .or_default()
                        .push(entry.message);
                }
            };
        }
        Some(result)
    }

    mock! {
        pub LogCollectorSubscription {
            pub fn start_collecting_logs(log_collectors: Vec<Box<dyn crate::runtime_connectors::log_collector::LogCollector>>) -> (Self, Vec<MockRuntimeConnectorReceiver>);
        }

        impl Drop for LogCollectorSubscription {
            fn drop(&mut self);
        }
    }

    mock! {
        pub RuntimeConnectorReceiver {
            pub async fn read_log_lines(&mut self) -> Option<Vec<String>>;
        }
    }

    // [utest->swdd~workload-log-facade-starts-log-collection-campaign~1]
    // [utest->swdd~workload-log-facade-forwards-logs-to-server~1]
    #[tokio::test]
    async fn utest_workload_log_facade_spawn_log_collection() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_server, mut to_server_receiver) = channel(BUFFER_SIZE);

        let mock_log_collector_1 = MockLogCollector::new();
        let mock_log_collector_2 = MockLogCollector::new();

        let mut mock_runtime_connector_receiver_1 = MockRuntimeConnectorReceiver::new();
        let mut mock_runtime_connector_receiver_2 = MockRuntimeConnectorReceiver::new();

        mock_runtime_connector_receiver_1
            .expect_read_log_lines()
            .once()
            .return_once(|| Some(vec!["rec1: line1".into(), "rec1: line2".into()]));
        mock_runtime_connector_receiver_2
            .expect_read_log_lines()
            .once()
            .return_once(|| Some(vec!["rec2: line1".into()]));
        mock_runtime_connector_receiver_2
            .expect_read_log_lines()
            .once()
            .return_once(|| None);
        mock_runtime_connector_receiver_1
            .expect_read_log_lines()
            .once()
            .return_once(|| Some(vec!["rec1: line3".into()]));
        mock_runtime_connector_receiver_1
            .expect_read_log_lines()
            .once()
            .return_once(|| None);

        let mut mock_log_collector_subscription = MockLogCollectorSubscription::new();
        let mock_log_collector_subscription_dropped = Arc::new(Mutex::new(false));
        let mock_log_collector_subscription_dropped_clone =
            mock_log_collector_subscription_dropped.clone();
        mock_log_collector_subscription
            .expect_drop()
            .returning(move || {
                *mock_log_collector_subscription_dropped_clone
                    .lock()
                    .unwrap() = true;
            });

        let collecting_logs_context = MockLogCollectorSubscription::start_collecting_logs_context();
        collecting_logs_context.expect().return_once(|_| {
            (
                mock_log_collector_subscription,
                vec![
                    mock_runtime_connector_receiver_1,
                    mock_runtime_connector_receiver_2,
                ],
            )
        });

        let mut mock_join_handle = MockJoinHandle::new();
        mock_join_handle.expect_abort().once().return_const(());
        let mock_subscription_entry = MockSubscriptionEntry::new_context();
        mock_subscription_entry
            .expect()
            .return_once(move |_| SubscriptionEntry::new(mock_join_handle));

        let workload_instance_name_1 = ank_base::WorkloadInstanceName {
            workload_name: WORKLOAD_1_NAME.into(),
            agent_name: AGENT_NAME.into(),
            id: "1234".into(),
        };

        let workload_instance_name_2 = ank_base::WorkloadInstanceName {
            workload_name: WORKLOAD_2_NAME.into(),
            agent_name: AGENT_NAME.into(),
            id: "1234".into(),
        };

        let logs_request = ank_base::LogsRequest {
            workload_names: vec![
                workload_instance_name_1.clone(),
                workload_instance_name_2.clone(),
            ],
            follow: None,
            tail: None,
            since: None,
            until: None,
        };

        let mut mock_runtime_manager = MockRuntimeManager::default();
        mock_runtime_manager
            .expect_get_logs()
            .with(predicate::eq(common::commands::LogsRequest::from(
                logs_request.clone(),
            )))
            .return_once(|_| {
                vec![
                    (
                        workload_instance_name_1.into(),
                        Box::new(mock_log_collector_1),
                    ),
                    (
                        workload_instance_name_2.into(),
                        Box::new(mock_log_collector_2),
                    ),
                ]
            });

        WorkloadLogFacade::spawn_log_collection(
            REQUEST_ID.into(),
            common::commands::LogsRequest::from(logs_request),
            to_server,
            SynchronizedSubscriptionStore::default(),
            &mock_runtime_manager,
        )
        .await;

        let log_responses = get_log_responses(3, &mut to_server_receiver).await.unwrap();

        assert_eq!(log_responses.len(), 2);
        assert!(log_responses.contains_key(&(REQUEST_ID.into(), WORKLOAD_1_NAME.into())));
        assert_eq!(
            log_responses
                .get(&(REQUEST_ID.into(), WORKLOAD_1_NAME.into()))
                .unwrap(),
            &vec![
                "rec1: line1".to_string(),
                "rec1: line2".to_string(),
                "rec1: line3".to_string(),
            ]
        );
        assert!(log_responses.contains_key(&(REQUEST_ID.into(), WORKLOAD_2_NAME.into())));
        assert_eq!(
            log_responses
                .get(&(REQUEST_ID.into(), WORKLOAD_2_NAME.into()))
                .unwrap(),
            &vec!["rec2: line1".to_string(),]
        );

        let log_responses = timeout(
            Duration::from_millis(10),
            get_log_responses(1, &mut to_server_receiver),
        )
        .await;
        assert!(log_responses.is_ok());
        assert!(log_responses.unwrap().is_none());

        let log_responses = timeout(
            Duration::from_millis(10),
            get_log_responses(1, &mut to_server_receiver),
        )
        .await;
        assert!(log_responses.is_ok());
        assert!(log_responses.unwrap().is_none());

        assert!(*mock_log_collector_subscription_dropped.lock().unwrap());
    }

    // [utest->swdd~workload-log-facade-automatically-unsubscribes-log-subscriptions~1]
    // [utest->swdd~workload-log-facade-sends-logs-stop-response~1]
    #[tokio::test]
    async fn utest_workload_log_facade_unsubscribe_subscription_and_send_logs_stop_response_on_no_more_logs(
    ) {
        let _ = env_logger::builder().is_test(true).try_init();
        let _guard = crate::test_helper::MOCKALL_CONTEXT_SYNC
            .get_lock_async()
            .await;

        let (to_server, mut to_server_receiver) = channel(BUFFER_SIZE);

        let mock_log_collector_1 = MockLogCollector::new();

        let mut mock_runtime_connector_receiver_1 = MockRuntimeConnectorReceiver::new();

        mock_runtime_connector_receiver_1
            .expect_read_log_lines()
            .once()
            .return_once(|| None);

        let mut mock_log_collector_subscription = MockLogCollectorSubscription::new();
        mock_log_collector_subscription
            .expect_drop()
            .once()
            .return_const(());

        let collecting_logs_context = MockLogCollectorSubscription::start_collecting_logs_context();
        collecting_logs_context.expect().return_once(|_| {
            (
                mock_log_collector_subscription,
                vec![mock_runtime_connector_receiver_1],
            )
        });

        let workload_instance_name_1 = ank_base::WorkloadInstanceName {
            workload_name: WORKLOAD_1_NAME.into(),
            agent_name: AGENT_NAME.into(),
            id: "1234".into(),
        };

        let logs_request = ank_base::LogsRequest {
            workload_names: vec![workload_instance_name_1.clone()],
            follow: None,
            tail: None,
            since: None,
            until: None,
        };

        let mut mock_runtime_manager = MockRuntimeManager::default();
        let cloned_workload_instance_name_1 = workload_instance_name_1.clone();
        mock_runtime_manager.expect_get_logs().return_once(|_| {
            vec![(
                cloned_workload_instance_name_1.into(),
                Box::new(mock_log_collector_1),
            )]
        });

        let mut mock_join_handle = MockJoinHandle::new();
        mock_join_handle.expect_abort().once().return_const(());
        let mock_subscription_entry = MockSubscriptionEntry::new_context();
        mock_subscription_entry
            .expect()
            .return_once(move |_| SubscriptionEntry::new(mock_join_handle));

        let synchronized_subscription_store = SynchronizedSubscriptionStore::default();
        WorkloadLogFacade::spawn_log_collection(
            REQUEST_ID.into(),
            common::commands::LogsRequest::from(logs_request),
            to_server,
            synchronized_subscription_store.clone(),
            &mock_runtime_manager,
        )
        .await;

        let logs_stop_response =
            tokio::time::timeout(Duration::from_millis(100), to_server_receiver.recv()).await;
        assert_eq!(
            logs_stop_response,
            Ok(Some(ToServer::LogsStopResponse(
                REQUEST_ID.into(),
                ank_base::LogsStopResponse {
                    workload_name: Some(workload_instance_name_1),
                },
            )))
        );

        assert!(
            synchronized_subscription_store.lock().unwrap().is_empty(),
            "Expected empty subscription store, but it contains subscriptions.",
        );
    }
}

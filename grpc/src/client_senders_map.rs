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

use crate::grpc_api::FromServer;
use common::std_extensions::IllegalStateResult;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;
use tonic::Status;

type ShareableHashMap<K, V> = Arc<Mutex<HashMap<K, V>>>;

#[derive(Debug, Clone)]
pub struct ClientSendersMap {
    client_senders: ShareableHashMap<String, Sender<Result<FromServer, Status>>>,
}

// Beside improving readability by hiding the lock steps, this trait helps improve the
// performance as it uses only the sync std::sync::Mutex.
// The get function here returns a clone of the sender eliminating the need to wait on
// the lock across the await of the send() of the sender.
//
// https://tokio.rs/tokio/tutorial/shared-state#on-using-stdsyncmutex
impl ClientSendersMap {
    pub fn new() -> Self {
        ClientSendersMap {
            client_senders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get(&self, name: &str) -> Option<Sender<Result<FromServer, Status>>> {
        self.client_senders
            .lock()
            .unwrap_or_illegal_state()
            .get(name)
            .cloned()
    }

    pub fn insert(&self, name: &str, sender: Sender<Result<FromServer, Status>>) {
        self.client_senders
            .lock()
            .unwrap_or_illegal_state()
            .insert(name.to_owned(), sender)
            .map_or_else(
                || {
                    log::trace!("Successfully added a new client sender.");
                },
                |_replaced| {
                    log::warn!(
                        "Received a NEW hello from client {name}. Replacing sender for this client."
                    )
                },
            );
    }

    pub fn get_all_client_names(&self) -> Vec<String> {
        self.client_senders
            .lock()
            .unwrap_or_illegal_state()
            .keys()
            .cloned()
            .collect()
    }

    pub fn remove(&self, name: &str) {
        self.client_senders
            .lock()
            .unwrap_or_illegal_state()
            .remove(name);
    }
}

impl Default for ClientSendersMap {
    fn default() -> Self {
        ClientSendersMap::new()
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
    use super::ClientSendersMap;
    use crate::grpc_api::FromServer;
    use tokio::sync::mpsc::Sender;
    use tonic::Status;

    fn create_test_sender() -> Sender<Result<FromServer, Status>> {
        tokio::sync::mpsc::channel::<Result<FromServer, Status>>(common::CHANNEL_CAPACITY).0
    }

    #[test]
    fn utest_client_senders_map_new_is_empty() {
        let senders = ClientSendersMap::new();
        assert!(senders.get_all_client_names().is_empty());
        assert!(senders.get("does_not_exist").is_none());
    }

    #[test]
    fn utest_client_senders_map_default_is_empty() {
        let senders = ClientSendersMap::default();
        assert!(senders.get_all_client_names().is_empty());
    }

    #[test]
    fn utest_client_senders_map_insert_and_get() {
        let senders = ClientSendersMap::new();
        senders.insert("name_1", create_test_sender());

        assert!(senders.get("name_1").is_some());
        assert!(senders.get("name_2").is_none());
    }

    #[test]
    fn utest_client_senders_map_insert_replaces_existing_entry() {
        let senders = ClientSendersMap::new();
        let first_sender = create_test_sender();
        let second_sender = create_test_sender();

        senders.insert("name_1", first_sender.clone());
        // inserting with the same name shall replace the previous sender
        senders.insert("name_1", second_sender.clone());

        assert_eq!(senders.get_all_client_names(), vec!["name_1".to_string()]);
        let stored_sender = senders.get("name_1").unwrap();
        assert!(stored_sender.same_channel(&second_sender));
        assert!(!stored_sender.same_channel(&first_sender));
    }

    #[test]
    fn utest_client_senders_map_get_all_client_names() {
        let senders = ClientSendersMap::new();
        senders.insert("name_1", create_test_sender());
        senders.insert("name_2", create_test_sender());

        let mut names = senders.get_all_client_names();
        names.sort();
        assert_eq!(names, vec!["name_1".to_string(), "name_2".to_string()]);
    }

    #[test]
    fn utest_client_senders_map_remove() {
        let senders = ClientSendersMap::new();
        senders.insert("name_1", create_test_sender());
        senders.insert("name_2", create_test_sender());

        senders.remove("name_1");

        assert!(senders.get("name_1").is_none());
        assert_eq!(senders.get_all_client_names(), vec!["name_2".to_string()]);
    }

    #[test]
    fn utest_client_senders_map_remove_non_existing_is_noop() {
        let senders = ClientSendersMap::new();
        senders.insert("name_1", create_test_sender());

        senders.remove("does_not_exist");

        assert_eq!(senders.get_all_client_names(), vec!["name_1".to_string()]);
    }
}

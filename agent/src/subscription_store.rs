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

use std::collections::HashMap;
#[cfg(not(test))]
use tokio::task::JoinHandle;

#[cfg(test)]
use tests::MockJoinHandle as JoinHandle;

#[derive(Debug)]
pub struct SubscriptionEntry {
    join_handle: JoinHandle<()>,
}

impl SubscriptionEntry {
    pub fn new(join_handle: JoinHandle<()>) -> Self {
        Self { join_handle }
    }
}

impl Drop for SubscriptionEntry {
    fn drop(&mut self) {
        log::trace!("Dropping join handle of subscription entry from the log subscription store.");
        self.join_handle.abort();
    }
}

type SubscriptionId = String;

#[derive(Default, Debug)]
pub struct SubscriptionStore {
    store: HashMap<SubscriptionId, SubscriptionEntry>,
}

impl SubscriptionStore {
    pub fn add_subscription(&mut self, id: SubscriptionId, subscription: SubscriptionEntry) {
        self.store.insert(id, subscription);
    }

    pub fn delete_subscription(&mut self, id: &SubscriptionId) {
        self.store.remove(id);
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
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
pub use tests::{MockJoinHandle, MockSubscriptionEntry};

#[cfg(test)]
mod tests {
    use super::{SubscriptionEntry, SubscriptionStore};
    use mockall::mock;

    const ID_1: &str = "id_1";
    const ID_2: &str = "id_2";

    #[test]
    fn utest_overwrite_drops_old_element() {
        let mut mock_join_handle_1 = MockJoinHandle::new();
        mock_join_handle_1.expect_abort().once().return_const(());

        let mut mock_join_handle_2 = MockJoinHandle::new();
        mock_join_handle_2.expect_abort().once().return_const(());

        let subscription_entry_1 = SubscriptionEntry::new(mock_join_handle_1);
        let subscription_entry_2 = SubscriptionEntry::new(mock_join_handle_2);

        let mut subscription_store = SubscriptionStore::default();
        subscription_store.add_subscription(ID_1.into(), subscription_entry_1);
        subscription_store.add_subscription(ID_2.into(), subscription_entry_2);

        let mut new_mock_join_handle_2 = MockJoinHandle::new();
        new_mock_join_handle_2
            .expect_abort()
            .once()
            .return_const(());

        let new_subscription_2 = SubscriptionEntry::new(new_mock_join_handle_2);

        // overwrite the existing subscription entry
        subscription_store.add_subscription(ID_2.into(), new_subscription_2);

        assert!(subscription_store.store.contains_key(ID_1));
        assert!(subscription_store.store.contains_key(ID_2));
    }

    mock! {
        #[derive(Debug)]
        pub JoinHandle<T> {
            pub fn abort(&self);
        }
    }

    mock! {
        pub SubscriptionEntry {
            /* In the non-mock version, passing the JoinHandle and returning a SubscriptionEntry is done for the following reasons:
                1. To avoid the need to implement complex tokio::spawn test helpers for tests in the module that constructs the SubscriptionEntry.
                2. Testing that the abort of the JoinHandle is called when the SubscriptionEntry is deallocated using a standard mock. */
            pub fn new(join_handle: tokio::task::JoinHandle<()>) -> crate::subscription_store::SubscriptionEntry;
        }
    }
}

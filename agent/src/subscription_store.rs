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
// SPDX-License-Identifier: Apache-2.

use std::{any::Any, collections::HashMap};

type SubscriptionId = String;

#[derive(Default)]
pub struct SubscriptionStore {
    store: HashMap<SubscriptionId, Box<dyn Any + Send>>,
}

impl SubscriptionStore {
    pub fn add_subscription(&mut self, id: SubscriptionId, subscription: impl Any + Send) {
        self.store.insert(id, Box::new(subscription));
    }

    #[cfg(test)]
    pub fn delete_subscritption(&mut self, id: &SubscriptionId) {
        self.store.remove(id);
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
    use std::sync::{Arc, Mutex};

    use super::SubscriptionStore;

    const ID_1: &str = "id 1";
    const ID_2: &str = "id 2";

    #[test]
    fn utest_none_dropped() {
        let element_1 = MockSubscription::default();
        let element_1_dropped = element_1.was_dropped.clone();
        let element_2 = MockSubscription::default();
        let element_2_dropped = element_2.was_dropped.clone();

        let mut subscription_store = SubscriptionStore::default();
        subscription_store.add_subscription(ID_1.into(), element_1);
        subscription_store.add_subscription(ID_2.into(), element_2);

        assert!(!*element_1_dropped.lock().unwrap());
        assert!(!*element_2_dropped.lock().unwrap());
    }

    #[test]
    fn utest_remove_drops_old_element() {
        let element_1 = MockSubscription::default();
        let element_1_dropped = element_1.was_dropped.clone();
        let element_2 = MockSubscription::default();
        let element_2_dropped = element_2.was_dropped.clone();

        let mut subscription_store = SubscriptionStore::default();
        subscription_store.add_subscription(ID_1.into(), element_1);
        subscription_store.add_subscription(ID_2.into(), element_2);

        assert!(!*element_1_dropped.lock().unwrap());
        assert!(!*element_2_dropped.lock().unwrap());

        subscription_store.delete_subscritption(&ID_2.into());

        assert!(!*element_1_dropped.lock().unwrap());
        assert!(*element_2_dropped.lock().unwrap());
    }

    #[test]
    fn utest_overwrite_drops_old_element() {
        let element_1 = MockSubscription::default();
        let element_1_dropped = element_1.was_dropped.clone();
        let element_2 = MockSubscription::default();
        let element_2_dropped = element_2.was_dropped.clone();

        let mut subscription_store = SubscriptionStore::default();
        subscription_store.add_subscription(ID_1.into(), element_1);
        subscription_store.add_subscription(ID_2.into(), element_2);

        assert!(!*element_1_dropped.lock().unwrap());
        assert!(!*element_2_dropped.lock().unwrap());

        subscription_store.add_subscription(ID_2.into(), MockSubscription::default());

        assert!(!*element_1_dropped.lock().unwrap());
        assert!(*element_2_dropped.lock().unwrap());
    }

    struct MockSubscription {
        was_dropped: Arc<Mutex<bool>>,
    }

    impl Default for MockSubscription {
        fn default() -> Self {
            Self {
                was_dropped: Arc::new(Mutex::new(false)),
            }
        }
    }

    impl Drop for MockSubscription {
        fn drop(&mut self) {
            *self.was_dropped.lock().unwrap() = true;
        }
    }
}

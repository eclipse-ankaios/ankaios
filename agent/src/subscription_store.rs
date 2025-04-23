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

    // will be used to cancel subscriptions, comment for now to pass clippy
    // pub fn delete_subscritption(&mut self, id: &SubscriptionId) {
    //     self.store.remove(id);
    // }
}

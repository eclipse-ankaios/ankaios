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

    pub fn delete_subscritptino(&mut self, id: &SubscriptionId) {
        self.store.remove(id);
    }
}

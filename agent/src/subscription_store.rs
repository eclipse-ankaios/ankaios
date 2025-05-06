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

    pub fn delete_subscription(&mut self, id: &SubscriptionId) {
        self.store.remove(id);
    }

    #[cfg(test)]
    pub fn contains_key(&self, id: &SubscriptionId) -> bool {
        self.store.contains_key(id)
    }
}

use std::{cell::RefCell, collections::HashMap};

use super::Store;

#[derive(Debug, Default)]
pub struct MemoryStore {
    entries: RefCell<HashMap<String, (String, u64)>>,
}

impl MemoryStore {
    pub fn value(&self, key: &str) -> Option<String> {
        self.entries
            .borrow()
            .get(key)
            .map(|(value, _)| value.clone())
    }

    pub fn ttl(&self, key: &str) -> Option<u64> {
        self.entries.borrow().get(key).map(|(_, ttl)| *ttl)
    }
}

impl Store for MemoryStore {
    fn get(&self, key: &str) -> impl Future<Output = Option<String>> {
        std::future::ready(self.value(key))
    }

    fn put(&self, key: &str, value: String, ttl_secs: u64) -> impl Future<Output = ()> {
        self.entries
            .borrow_mut()
            .insert(key.to_owned(), (value, ttl_secs));
        std::future::ready(())
    }
}

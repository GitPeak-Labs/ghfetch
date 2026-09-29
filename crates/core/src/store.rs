#[allow(async_fn_in_trait)]
pub trait Store {
    async fn get(&self, key: &str) -> Option<String>;

    fn put(&self, key: &str, value: String, ttl_secs: u64);
}

#[cfg(test)]
pub mod memory;

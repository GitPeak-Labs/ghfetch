use ghfetch_core::store::Store;
use worker::{KvStore, console_error};

#[derive(Clone)]
pub struct KvCache(KvStore);

impl KvCache {
    pub fn new(kv: KvStore) -> Self {
        Self(kv)
    }
}

impl Store for KvCache {
    async fn get(&self, key: &str) -> Option<String> {
        match self.0.get(key).text().await {
            Ok(value) => value,
            Err(error) => {
                console_error!("KV read failed for {key}: {error}");
                None
            }
        }
    }

    async fn put(&self, key: &str, value: String, ttl_secs: u64) {
        let result = match self.0.put(key, value) {
            Ok(builder) => builder.expiration_ttl(ttl_secs).execute().await,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            console_error!("KV write failed for {key}: {error}");
        }
    }
}

use ghfetch_core::store::Store;
use worker::{Context, KvStore, console_error};

pub struct KvCache<'a> {
    kv: KvStore,
    ctx: &'a Context,
}

impl<'a> KvCache<'a> {
    pub fn new(kv: KvStore, ctx: &'a Context) -> Self {
        Self { kv, ctx }
    }
}

impl Store for KvCache<'_> {
    async fn get(&self, key: &str) -> Option<String> {
        match self.kv.get(key).text().await {
            Ok(value) => value,
            Err(error) => {
                console_error!("KV read failed for {key}: {error}");
                None
            }
        }
    }

    fn put(&self, key: &str, value: String, ttl_secs: u64) {
        let kv = self.kv.clone();
        let key = key.to_owned();

        self.ctx.wait_until(async move {
            let result = match kv.put(&key, value) {
                Ok(builder) => builder.expiration_ttl(ttl_secs).execute().await,
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                console_error!("KV write failed for {key}: {error}");
            }
        });
    }
}

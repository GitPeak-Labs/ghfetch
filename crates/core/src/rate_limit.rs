use futures_util::future::join;

use crate::{store::Store, username::Username};

const MIN_TTL_SECS: u64 = 60;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub ip_per_window: u32,
    pub username_per_window: u32,
    pub pair_per_window: u32,
    pub window_secs: u64,
    pub block_secs: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            ip_per_window: 100,
            username_per_window: 60,
            pair_per_window: 10,
            window_secs: 60,
            block_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allowed { remaining: u32, reset: u64 },
    IpLimited,
    UsernameLimited,
    Blocked,
}

#[derive(Debug)]
pub struct RateLimiter<'a, S> {
    store: &'a S,
    limits: Limits,
}

impl<'a, S: Store> RateLimiter<'a, S> {
    pub fn new(store: &'a S, limits: Limits) -> Self {
        Self { store, limits }
    }

    pub async fn check(&self, ip: &str, username: &Username, now_secs: u64) -> Verdict {
        let window = self.limits.window_secs;
        let window_start = now_secs / window * window;
        let ttl = (window - (now_secs - window_start)).max(MIN_TTL_SECS);

        let ip_key = format!("rl:gl:{ip}:{window_start}");
        let username_key = format!("rl:ugl:{username}:{window_start}");
        let (ip_raw, username_raw) =
            join(self.store.get(&ip_key), self.store.get(&username_key)).await;
        let ip_count = parse_count(ip_raw);
        let username_count = parse_count(username_raw);

        self.store.put(&ip_key, (ip_count + 1).to_string(), ttl);
        self.store
            .put(&username_key, (username_count + 1).to_string(), ttl);

        if ip_count >= self.limits.ip_per_window {
            return Verdict::IpLimited;
        }
        if username_count >= self.limits.username_per_window {
            return Verdict::UsernameLimited;
        }

        let pair = format!("{ip}:{username}");
        let block_key = format!("block:{pair}");
        let count_key = format!("rl:{pair}:{window_start}");
        let (block_raw, count_raw) =
            join(self.store.get(&block_key), self.store.get(&count_key)).await;

        if block_raw.is_some() {
            return Verdict::Blocked;
        }

        let max = self.limits.pair_per_window;
        let count = parse_count(count_raw);
        if count >= max {
            self.store
                .put(&block_key, "blocked".to_owned(), self.limits.block_secs);
            return Verdict::Blocked;
        }

        let new_count = count + 1;
        self.store.put(&count_key, new_count.to_string(), ttl);

        Verdict::Allowed {
            remaining: max - new_count,
            reset: window_start + window,
        }
    }
}

fn parse_count(raw: Option<String>) -> u32 {
    raw.and_then(|value| value.parse().ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests;

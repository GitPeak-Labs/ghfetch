use futures_util::future::join;
use serde::Serialize;

use crate::{
    error::ApiError,
    github::GitHub,
    rate_limit::{Limits, RateLimiter, Verdict},
    stats::{self, GitHubStats},
    store::Store,
    username::Username,
};

pub const CACHE_TTL_SECS: u64 = 900;
const CACHE_HARD_TTL_SECS: u64 = 21_600;
const REFRESH_LOCK_SECS: u64 = 60;
const CACHED_REMAINING: u32 = 10;
const CACHED_RESET_SECS: u64 = 60;
const DEV_ORIGIN: &str = "localhost:5173";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    Public,
    Portfolio,
}

impl Audience {
    #[must_use]
    pub fn classify(origin: Option<&str>, referer: Option<&str>, portfolio_origin: &str) -> Self {
        let mentions_portfolio = [origin, referer]
            .into_iter()
            .flatten()
            .any(|value| value.contains(portfolio_origin) || value.contains(DEV_ORIGIN));

        if mentions_portfolio {
            Self::Portfolio
        } else {
            Self::Public
        }
    }

    #[must_use]
    pub fn includes_private(self) -> bool {
        self == Self::Portfolio
    }

    fn cache_label(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Portfolio => "private",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Hit,
    Miss,
}

impl CacheStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "HIT",
            Self::Miss => "MISS",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StatsRequest<'a> {
    pub username: Option<&'a str>,
    pub origin: Option<&'a str>,
    pub referer: Option<&'a str>,
    pub client_ip: &'a str,
    pub now_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsReply {
    pub body: String,
    pub cache: CacheStatus,
    pub remaining: u32,
    pub reset: u64,
    pub needs_refresh: bool,
    pub username: Username,
    pub audience: Audience,
}

#[derive(Debug)]
pub struct Service<'a, G, S> {
    pub github: &'a G,
    pub store: &'a S,
    pub portfolio_origin: &'a str,
    pub limits: Limits,
}

#[derive(Serialize)]
struct Envelope<'a> {
    ok: bool,
    data: &'a GitHubStats,
}

impl<G: GitHub, S: Store> Service<'_, G, S> {
    pub async fn stats(&self, request: StatsRequest<'_>) -> Result<StatsReply, ApiError> {
        let username: Username = request
            .username
            .ok_or(ApiError::InvalidUsername)?
            .parse()
            .map_err(|_| ApiError::InvalidUsername)?;

        let audience = Audience::classify(request.origin, request.referer, self.portfolio_origin);
        let cache_key = Self::cache_key(&username, audience);
        let meta_key = Self::meta_key(&cache_key);

        let (cached_body, cached_meta) =
            join(self.store.get(&cache_key), self.store.get(&meta_key)).await;

        if let Some(body) = cached_body {
            let fetched_at = cached_meta.and_then(|raw| raw.parse().ok()).unwrap_or(0);
            let stale = request.now_secs.saturating_sub(fetched_at) >= CACHE_TTL_SECS;
            let needs_refresh = stale && self.claim_refresh(&cache_key).await;

            return Ok(StatsReply {
                body,
                cache: CacheStatus::Hit,
                remaining: CACHED_REMAINING,
                reset: CACHED_RESET_SECS,
                needs_refresh,
                username,
                audience,
            });
        }

        let limiter = RateLimiter::new(self.store, self.limits);
        let (remaining, reset) = match limiter
            .check(request.client_ip, &username, request.now_secs)
            .await
        {
            Verdict::Allowed { remaining, reset } => (remaining, reset),
            Verdict::IpLimited => return Err(ApiError::IpRateLimited),
            Verdict::UsernameLimited => return Err(ApiError::UsernameRateLimited),
            Verdict::Blocked => return Err(ApiError::Blocked),
        };

        let body = self
            .fetch_and_store(&username, audience, request.now_secs)
            .await?;

        Ok(StatsReply {
            body,
            cache: CacheStatus::Miss,
            remaining,
            reset,
            needs_refresh: false,
            username,
            audience,
        })
    }

    pub async fn refresh(
        &self,
        username: &Username,
        audience: Audience,
        now_secs: u64,
    ) -> Result<(), ApiError> {
        self.fetch_and_store(username, audience, now_secs)
            .await
            .map(|_| ())
    }

    async fn fetch_and_store(
        &self,
        username: &Username,
        audience: Audience,
        now_secs: u64,
    ) -> Result<String, ApiError> {
        let stats = stats::load(self.github, username, audience.includes_private())
            .await?
            .ok_or(ApiError::UserNotFound)?;

        let body = serde_json::to_string(&Envelope {
            ok: true,
            data: &stats,
        })
        .map_err(|_| ApiError::Internal)?;

        let cache_key = Self::cache_key(username, audience);
        let meta_key = Self::meta_key(&cache_key);
        join(
            self.store
                .put(&cache_key, body.clone(), CACHE_HARD_TTL_SECS),
            self.store
                .put(&meta_key, now_secs.to_string(), CACHE_HARD_TTL_SECS),
        )
        .await;

        Ok(body)
    }

    async fn claim_refresh(&self, cache_key: &str) -> bool {
        let lock_key = format!("{cache_key}:lock");
        if self.store.get(&lock_key).await.is_some() {
            return false;
        }
        self.store
            .put(&lock_key, "1".to_owned(), REFRESH_LOCK_SECS)
            .await;
        true
    }

    fn cache_key(username: &Username, audience: Audience) -> String {
        format!("cache:v3:{username}:{}", audience.cache_label())
    }

    fn meta_key(cache_key: &str) -> String {
        format!("{cache_key}:meta")
    }
}

#[cfg(test)]
mod tests;

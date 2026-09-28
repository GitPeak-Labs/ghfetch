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
        let cache_key = format!("cache:v3:{username}:{}", audience.cache_label());

        if let Some(body) = self.store.get(&cache_key).await {
            return Ok(StatsReply {
                body,
                cache: CacheStatus::Hit,
                remaining: CACHED_REMAINING,
                reset: CACHED_RESET_SECS,
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

        let stats = stats::load(self.github, &username, audience.includes_private())
            .await?
            .ok_or(ApiError::UserNotFound)?;

        let body = serde_json::to_string(&Envelope {
            ok: true,
            data: &stats,
        })
        .map_err(|_| ApiError::Internal)?;
        self.store.put(&cache_key, body.clone(), CACHE_TTL_SECS);

        Ok(StatsReply {
            body,
            cache: CacheStatus::Miss,
            remaining,
            reset,
        })
    }
}

#[cfg(test)]
mod tests;

use worker::{event, Env, Request, Response, Result};

mod types;
mod rate_limit;
mod github;
mod processor;
mod collaborators;
mod response;
mod tests;

use types::{ClientId, RateConfig, Username};
use rate_limit::{flush_global_writes, flush_rate_writes, Limiter};
use github::GitHubClient;
use processor::{build_stats, process_repos};
use response::{cors_preflight, err, health, success};
use shared::github::GitHubStats;

const CACHE_TTL: u64 = 300; // 5 minutes
const VERSION: &str = "1.0.0";

#[event(fetch)]
pub async fn main(req: Request, env: Env, ctx: worker::Context) -> Result<Response> {
    if req.method() == worker::Method::Options {
        return cors_preflight();
    }

    let url = req.url()?;
    let path = url.path();

    if path == "/" || path.is_empty() {
        return response::root(VERSION);
    }

    if path == "/health" {
        return health(VERSION);
    }

    if path == "/v1/stats" {
        return handle_stats(req, env, ctx).await;
    }

    err(404, "Not found", None)
}

async fn handle_stats(req: Request, env: Env, ctx: worker::Context) -> Result<Response> {
    let url = req.url()?;
    let raw = url
        .query_pairs()
        .find(|(k, _)| k == "username")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();

    let user = match Username::new(&raw) {
        Some(u) => u,
        None => return err(400, "Invalid username format", None),
    };

    // --- Privacy Filter Logic ---
    let headers = req.headers();
    let origin_str = headers.get("Origin").unwrap_or(None).unwrap_or_default();
    let referer_str = headers.get("Referer").unwrap_or(None).unwrap_or_default();

    let portfolio_domain = env
        .secret("PORTFOLIO_ORIGIN")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "carlosranara.com".to_string());

    let is_portfolio_request = origin_str.contains(&portfolio_domain)
        || referer_str.contains(&portfolio_domain)
        || origin_str.contains("localhost:5173")
        || referer_str.contains("localhost:5173");

    // Partition Cache Keys based on request classification
    let cache_key_url = if is_portfolio_request {
        format!("{}&_private=1", url)
    } else {
        format!("{}&_private=0", url)
    };

    let kv_cache_key = if is_portfolio_request {
        format!("cache:{}:private", user.as_str())
    } else {
        format!("cache:{}:public", user.as_str())
    };

    let edge_cache = worker::Cache::default();

    if let Ok(Some(cached_resp)) = edge_cache.get(&cache_key_url, false).await {
        worker::console_log!("Edge cache hit for {} (private_authorized: {})", user.as_str(), is_portfolio_request);
        return Ok(cached_resp);
    }

    let kv = env.kv("RATE_LIMIT_KV").ok();

    if let Some(ref kv_store) = kv {
        if let Ok(Some(cached)) = kv_store.get(&kv_cache_key).text().await {
            if let Ok(stats) = serde_json::from_str::<GitHubStats>(&cached) {
                worker::console_log!("KV cache hit for {} (private_authorized: {})", user.as_str(), is_portfolio_request);
                let resp = success(stats.clone(), 10, 60, true)?;

                let cache_url = cache_key_url.clone();
                ctx.wait_until(async move {
                    let cache = worker::Cache::default();
                    if let Ok(cache_resp) = success(stats, 10, 60, true) {
                        let _ = cache.put(cache_url, cache_resp).await;
                    }
                });

                return Ok(resp);
            }
        }
    }

    let token = env
        .secret("GITHUB_TOKEN")
        .map(|s| s.to_string())
        .unwrap_or_default();

    if token.is_empty() {
        return err(500, "Server configuration error", None);
    }

    let client = ClientId::from_req(&req);

    let (remaining, reset) = if let Some(ref kv_store) = kv {
        let limiter = Limiter::new(kv_store);

        let global_result = limiter.check_globals(&client, &user).await?;

        let global_ip_blocked = global_result.global_ip_blocked;
        let username_blocked = global_result.username_blocked;

        if let Ok(kv_bg) = env.kv("RATE_LIMIT_KV") {
            ctx.wait_until(async move {
                flush_global_writes(&kv_bg, global_result).await;
            });
        }

        if global_ip_blocked {
            return err(429, "Too many requests. Slow down.", Some((0, 60)));
        }
        if username_blocked {
            return err(429, "Too many requests for this user. Try again in a minute.", Some((0, 60)));
        }

        let cfg = RateConfig::default();
        match limiter.check_with_info(&client, &user, cfg).await? {
            Some(((remaining, reset), writes)) => {
                let is_blocked = writes.block_key.is_some();

                if let Ok(kv_bg) = env.kv("RATE_LIMIT_KV") {
                    ctx.wait_until(async move {
                        flush_rate_writes(&kv_bg, writes).await;
                    });
                }

                if is_blocked {
                    return err(429, "Rate limit exceeded. Try again in 5 minutes.", Some((0, 300)));
                }

                (remaining, reset)
            }
            None => return err(429, "Rate limit exceeded. Try again in 5 minutes.", Some((0, 300))),
        }
    } else {
        (10u32, 60u64)
    };

    let gh = GitHubClient::new(token.clone());
    let gql = match gh.fetch(&user).await {
        Ok(r) => r,
        Err(e) => {
            worker::console_log!("GitHub error: {:?}", e);
            let msg = e.to_string();
            let code = if msg.contains("rate limit") { 503 } else { 502 };
            let display_msg = if code == 503 {
                "GitHub API rate limit exceeded"
            } else {
                "Failed to fetch data from GitHub"
            };
            return err(code, display_msg, None);
        }
    };

    let mut gql_user = match gql.data {
        Some(d) => d.user,
        None => return err(404, "User not found", None),
    };

    let private_repos = if is_portfolio_request {
        std::mem::take(&mut gql_user.repositories.nodes)
    } else {
        Vec::new()
    };

    let contributed: Vec<_> = gql_user
        .contributions_collection
        .commit_contributions_by_repository
        .iter()
        .filter(|c| is_portfolio_request || !c.repository.is_private)
        .map(|c| {
            let occurred_at = c
                .contributions
                .as_ref()
                .and_then(|conn| conn.nodes.first())
                .map(|node| node.occurred_at.clone());
            (c.repository.clone(), occurred_at)
        })
        .collect();

    let processed = process_repos(
        user.as_str(),
        &private_repos,
        &gql_user.public_repositories.nodes,
        &contributed,
    );

    let collaborators = collaborators::fetch_collaborators(
        &token,
        user.as_str(),
        &processed.involved_repos,
        is_portfolio_request,
    ).await;

    let mut stats = build_stats(
        gql_user,
        user.as_str(),
        processed.repo_count,
        processed.total_stars,
        processed.languages,
        processed.most_starred_repo,
        processed.involved_repos,
    );
    stats.collaborators = collaborators;

    let resp = success(stats.clone(), remaining, reset, false)?;

    let user_str = user.as_str().to_string();
    let cache_url = cache_key_url;
    let kv_key = kv_cache_key;

    ctx.wait_until(async move {
        let cache = worker::Cache::default();
        if let Ok(cache_resp) = success(stats.clone(), remaining, reset, false) {
            let _ = cache.put(cache_url, cache_resp).await;
        }

        if let Ok(kv_bg) = env.kv("RATE_LIMIT_KV") {
            if let Ok(json) = serde_json::to_string(&stats) {
                if let Ok(builder) = kv_bg.put(&kv_key, &json) {
                    let _ = builder.expiration_ttl(CACHE_TTL).execute().await;
                    worker::console_log!("Cached stats for {}", user_str);
                }
            }
        }
    });

    Ok(resp)
}

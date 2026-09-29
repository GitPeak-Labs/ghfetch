mod github;
mod http;
mod store;

use ghfetch_core::{
    error::ApiError,
    rate_limit::Limits,
    service::{Service, StatsReply, StatsRequest},
    username::Username,
};
use worker::{
    Context, Date, Env, Method, Request, Response, Result, Url, console_error, console_log, event,
};

use crate::{github::WorkerGitHub, store::KvCache};

const NAME: &str = "ghfetch";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PORTFOLIO_ORIGIN: &str = "carlosranara.com";

#[event(start)]
fn start() {
    console_error_panic_hook::set_once();
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let response = match route(&req, &env, &ctx).await {
        Ok(response) => response,
        Err(error) => {
            console_error!("unhandled error: {error}");
            http::error(&ApiError::Internal)?
        }
    };

    http::finish(response)
}

async fn route(req: &Request, env: &Env, ctx: &Context) -> Result<Response> {
    let url = req.url()?;

    match (req.method(), url.path()) {
        (Method::Options, _) => http::preflight(),
        (Method::Get, "/") => http::info(NAME, VERSION),
        (Method::Get, "/health") => http::health(VERSION),
        (Method::Get, "/v1/stats") => stats(req, env, ctx, &url).await,
        _ => http::error(&ApiError::NotFound),
    }
}

async fn stats(req: &Request, env: &Env, ctx: &Context, url: &Url) -> Result<Response> {
    let token = env.secret("GITHUB_TOKEN").map(|secret| secret.to_string());
    let Some(token) = token.ok().filter(|token| !token.is_empty()) else {
        console_error!("GITHUB_TOKEN is not configured");
        return http::error(&ApiError::Misconfigured);
    };

    let portfolio_origin = env
        .var("PORTFOLIO_ORIGIN")
        .map(|var| var.to_string())
        .ok()
        .filter(|origin| !origin.is_empty())
        .unwrap_or_else(|| DEFAULT_PORTFOLIO_ORIGIN.to_owned());

    let username = url
        .query_pairs()
        .find(|(key, _)| key == "username")
        .map(|(_, value)| value.into_owned());
    let origin = req.headers().get("Origin")?;
    let referer = req.headers().get("Referer")?;
    let client_ip = req
        .headers()
        .get("CF-Connecting-IP")?
        .unwrap_or_else(|| "unknown".to_owned());

    let github = WorkerGitHub::new(token.clone());
    let store = KvCache::new(env.kv("RATE_LIMIT_KV")?);
    let now_secs = Date::now().as_millis() / 1000;

    let outcome = {
        let service = Service {
            github: &github,
            store: &store,
            portfolio_origin: &portfolio_origin,
            limits: Limits::default(),
        };

        service
            .stats(StatsRequest {
                username: username.as_deref(),
                origin: origin.as_deref(),
                referer: referer.as_deref(),
                client_ip: &client_ip,
                now_secs,
            })
            .await
    };

    match outcome {
        Ok(reply) => {
            if reply.needs_refresh {
                spawn_refresh(ctx, token, store, portfolio_origin, &reply, now_secs);
            }
            http::stats(&reply)
        }
        Err(error) => {
            log_failure(&error, &client_ip);
            http::error(&error)
        }
    }
}

fn spawn_refresh(
    ctx: &Context,
    token: String,
    store: KvCache,
    portfolio_origin: String,
    reply: &StatsReply,
    now_secs: u64,
) {
    let username = reply.username.clone();
    let audience = reply.audience;

    ctx.wait_until(async move {
        let github = WorkerGitHub::new(token);
        let service = Service {
            github: &github,
            store: &store,
            portfolio_origin: &portfolio_origin,
            limits: Limits::default(),
        };

        if let Err(error) = service.refresh(&username, audience, now_secs).await {
            log_refresh_failure(&username, &error);
        }
    });
}

fn log_refresh_failure(username: &Username, error: &ApiError) {
    console_error!("background refresh failed for {username}: {error}");
}

fn log_failure(error: &ApiError, client_ip: &str) {
    match error {
        ApiError::Upstream(source) => console_error!("GitHub error: {source}"),
        ApiError::UpstreamRateLimited => console_error!("GitHub API rate limit reached"),
        ApiError::IpRateLimited | ApiError::UsernameRateLimited | ApiError::Blocked => {
            console_log!("{error} ({client_ip})");
        }
        _ => {}
    }
}

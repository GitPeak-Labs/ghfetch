use ghfetch_core::{error::ApiError, service::StatsReply};
use serde_json::json;
use worker::{Cors, Method, Response, Result};

const STATS_CACHE_CONTROL: &str = "public, max-age=900, stale-while-revalidate=120";
const NO_STORE: &str = "no-store, no-cache, must-revalidate";

fn json_response(
    status: u16,
    body: String,
    cache_control: &str,
    extra: &[(&str, String)],
) -> Result<Response> {
    let mut response = Response::from_bytes(body.into_bytes())?.with_status(status);

    let headers = response.headers_mut();
    headers.set("Content-Type", "application/json")?;
    headers.set("Cache-Control", cache_control)?;
    headers.set("Vary", "Accept-Encoding")?;
    for (name, value) in extra {
        headers.set(name, value)?;
    }
    Ok(response)
}

pub fn stats(reply: &StatsReply) -> Result<Response> {
    json_response(
        200,
        reply.body.clone(),
        STATS_CACHE_CONTROL,
        &[
            ("X-RateLimit-Remaining", reply.remaining.to_string()),
            ("X-RateLimit-Reset", reply.reset.to_string()),
            ("X-Cache", reply.cache.as_str().to_owned()),
        ],
    )
}

pub fn error(error: &ApiError) -> Result<Response> {
    let body = json!({ "ok": false, "error": error.to_string() }).to_string();

    let extra: Vec<(&str, String)> = error
        .retry_after_secs()
        .map(|reset| {
            vec![
                ("X-RateLimit-Remaining", "0".to_owned()),
                ("X-RateLimit-Reset", reset.to_string()),
            ]
        })
        .unwrap_or_default();

    json_response(error.status(), body, NO_STORE, &extra)
}

pub fn info(name: &str, version: &str) -> Result<Response> {
    let body = json!({
        "name": name,
        "version": version,
        "description": "GitHub stats API",
        "endpoints": {
            "stats": "/v1/stats?username=<github-username>",
            "health": "/health",
        },
        "source": "https://github.com/GitPeak-Labs/ghfetch",
    })
    .to_string();

    json_response(200, body, "public, max-age=900", &[])
}

pub fn health(version: &str) -> Result<Response> {
    let body = json!({ "status": "ok", "version": version }).to_string();
    json_response(200, body, "no-store", &[])
}

pub fn preflight() -> Result<Response> {
    Ok(Response::empty()?.with_status(204))
}

pub fn finish(response: Response) -> Result<Response> {
    let cors = Cors::new()
        .with_origins(["*"])
        .with_methods([Method::Get, Method::Options])
        .with_allowed_headers(["Content-Type"])
        .with_max_age(86_400);

    let mut response = response.with_cors(&cors)?;

    let headers = response.headers_mut();
    headers.set("X-Content-Type-Options", "nosniff")?;
    headers.set("X-Frame-Options", "DENY")?;
    headers.set("Referrer-Policy", "strict-origin-when-cross-origin")?;
    headers.set("X-XSS-Protection", "1; mode=block")?;
    headers.set(
        "Strict-Transport-Security",
        "max-age=15552000; includeSubDomains",
    )?;
    Ok(response)
}

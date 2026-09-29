pub mod graphql;

use serde::Deserialize;
use serde_json::json;

use crate::username::Username;

#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    #[error("GitHub rejected the access token")]
    Unauthorized,
    #[error("GitHub API rate limit exceeded")]
    RateLimited,
    #[error("GitHub responded with HTTP {0}")]
    Status(u16),
    #[error("GitHub returned GraphQL errors: {0}")]
    GraphQl(String),
    #[error("GitHub sent a response that could not be decoded: {0}")]
    Decode(String),
    #[error("request to GitHub failed: {0}")]
    Transport(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Contributor {
    pub login: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub contributions: u64,
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[allow(async_fn_in_trait)]
pub trait GitHub {
    async fn user(&self, login: &Username) -> Result<Option<graphql::User>, GitHubError>;

    async fn contributors(&self, owner: &str, repo: &str) -> Result<Vec<Contributor>, GitHubError>;
}

#[must_use]
pub fn user_request_body(login: &Username) -> String {
    json!({ "query": graphql::QUERY, "variables": { "u": login } }).to_string()
}

#[must_use]
pub fn contributors_path(owner: &str, repo: &str) -> String {
    format!("/repos/{owner}/{repo}/contributors?per_page=100&anon=false")
}

pub fn parse_user(status: u16, body: &[u8]) -> Result<Option<graphql::User>, GitHubError> {
    match status {
        200 => {}
        401 => return Err(GitHubError::Unauthorized),
        403 | 429 => return Err(GitHubError::RateLimited),
        other => return Err(GitHubError::Status(other)),
    }

    let response: graphql::Response =
        serde_json::from_slice(body).map_err(|error| GitHubError::Decode(error.to_string()))?;

    match response.data.and_then(|data| data.user) {
        Some(user) => Ok(Some(user)),
        None => missing_user(&response.errors),
    }
}

pub fn parse_contributors(status: u16, body: &[u8]) -> Result<Vec<Contributor>, GitHubError> {
    match status {
        200 => serde_json::from_slice(body).map_err(|error| GitHubError::Decode(error.to_string())),
        204 => Ok(Vec::new()),
        other => Err(GitHubError::Status(other)),
    }
}

fn missing_user(errors: &[graphql::Error]) -> Result<Option<graphql::User>, GitHubError> {
    let kind_is = |error: &graphql::Error, kind: &str| error.kind.as_deref() == Some(kind);

    if errors.iter().any(|error| kind_is(error, "RATE_LIMITED")) {
        return Err(GitHubError::RateLimited);
    }

    if errors.iter().all(|error| kind_is(error, "NOT_FOUND")) {
        return Ok(None);
    }

    let messages: Vec<_> = errors.iter().map(|error| error.message.as_str()).collect();
    Err(GitHubError::GraphQl(messages.join("; ")))
}

#[cfg(test)]
mod tests;

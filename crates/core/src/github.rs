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
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    fn user_json() -> Value {
        json!({ "data": { "user": {
            "avatarUrl": "a", "name": null, "bio": null, "createdAt": "2011-01-25T18:44:36Z",
            "followers": { "totalCount": 1 }, "following": { "totalCount": 2 },
            "contributionsCollection": {
                "contributionCalendar": { "totalContributions": 3 },
                "totalCommitContributions": 1,
                "totalPullRequestContributions": 1,
                "totalIssueContributions": 1,
                "commitContributionsByRepository": []
            },
            "repositories": { "nodes": [] },
            "publicRepositories": { "nodes": [] }
        } } })
    }

    #[test]
    fn parses_a_user() {
        let user = parse_user(200, &bytes(&user_json())).unwrap().unwrap();
        assert_eq!(user.followers.total_count, 1);
        assert!(user.name.is_none());
    }

    #[test]
    fn maps_http_statuses() {
        assert!(matches!(
            parse_user(401, b""),
            Err(GitHubError::Unauthorized)
        ));
        assert!(matches!(
            parse_user(403, b""),
            Err(GitHubError::RateLimited)
        ));
        assert!(matches!(
            parse_user(429, b""),
            Err(GitHubError::RateLimited)
        ));
        assert!(matches!(
            parse_user(500, b""),
            Err(GitHubError::Status(500))
        ));
    }

    #[test]
    fn unknown_users_are_none() {
        let not_found = json!({
            "data": { "user": null },
            "errors": [{ "type": "NOT_FOUND", "message": "Could not resolve to a User" }],
        });
        assert!(parse_user(200, &bytes(&not_found)).unwrap().is_none());
        assert!(
            parse_user(200, &bytes(&json!({ "data": null })))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn graphql_rate_limit_errors_are_rate_limited() {
        let body =
            json!({ "data": null, "errors": [{ "type": "RATE_LIMITED", "message": "slow" }] });
        assert!(matches!(
            parse_user(200, &bytes(&body)),
            Err(GitHubError::RateLimited)
        ));
    }

    #[test]
    fn other_graphql_errors_are_failures() {
        let body = json!({ "data": { "user": null }, "errors": [{ "type": "INTERNAL", "message": "boom" }] });
        assert!(
            matches!(parse_user(200, &bytes(&body)), Err(GitHubError::GraphQl(m)) if m == "boom")
        );
    }

    #[test]
    fn malformed_bodies_are_decode_errors() {
        assert!(matches!(
            parse_user(200, b"not json"),
            Err(GitHubError::Decode(_))
        ));
    }

    #[test]
    fn parses_contributors_with_defaults() {
        let body = json!([{ "login": "a" }, { "login": "b", "contributions": 4, "type": "Bot" }]);
        let parsed = parse_contributors(200, &bytes(&body)).unwrap();

        assert_eq!(parsed[0].contributions, 0);
        assert_eq!(parsed[0].avatar_url, "");
        assert_eq!(parsed[1].kind, "Bot");
    }

    #[test]
    fn contributors_status_handling() {
        assert!(parse_contributors(204, b"").unwrap().is_empty());
        assert!(matches!(
            parse_contributors(404, b""),
            Err(GitHubError::Status(404))
        ));
    }

    #[test]
    fn builds_request_parts() {
        let login: Username = "octocat".parse().unwrap();
        let body: Value = serde_json::from_str(&user_request_body(&login)).unwrap();

        assert_eq!(body["variables"]["u"], "octocat");
        assert!(
            body["query"]
                .as_str()
                .unwrap()
                .contains("contributionsCollection")
        );
        assert_eq!(
            contributors_path("octocat", "hello"),
            "/repos/octocat/hello/contributors?per_page=100&anon=false"
        );
    }
}

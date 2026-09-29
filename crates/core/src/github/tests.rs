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
    let body = json!({ "data": null, "errors": [{ "type": "RATE_LIMITED", "message": "slow" }] });
    assert!(matches!(
        parse_user(200, &bytes(&body)),
        Err(GitHubError::RateLimited)
    ));
}

#[test]
fn other_graphql_errors_are_failures() {
    let body =
        json!({ "data": { "user": null }, "errors": [{ "type": "INTERNAL", "message": "boom" }] });
    assert!(matches!(parse_user(200, &bytes(&body)), Err(GitHubError::GraphQl(m)) if m == "boom"));
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

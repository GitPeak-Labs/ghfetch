use std::{cell::Cell, collections::HashMap};

use serde_json::{Value, json};

use super::*;
use crate::{
    github::{Contributor, GitHubError, graphql, parse_contributors, parse_user},
    store::memory::MemoryStore,
};

const NOW: u64 = 1_800_000_000;
const PORTFOLIO: &str = "portfolio.example.com";

struct FakeGitHub {
    user: (u16, Vec<u8>),
    contributors: HashMap<String, (u16, Vec<u8>)>,
    user_calls: Cell<u32>,
}

impl FakeGitHub {
    fn new(payload: &Value) -> Self {
        Self::with_status(200, payload)
    }

    fn with_status(status: u16, payload: &Value) -> Self {
        Self::with_raw(status, serde_json::to_vec(payload).unwrap())
    }

    fn with_raw(status: u16, body: Vec<u8>) -> Self {
        Self {
            user: (status, body),
            contributors: HashMap::new(),
            user_calls: Cell::new(0),
        }
    }

    fn contributors(mut self, owner: &str, repo: &str, payload: &Value) -> Self {
        self.contributors.insert(
            format!("{owner}/{repo}"),
            (200, serde_json::to_vec(payload).unwrap()),
        );
        self
    }
}

impl GitHub for FakeGitHub {
    fn user(
        &self,
        _login: &Username,
    ) -> impl Future<Output = Result<Option<graphql::User>, GitHubError>> {
        self.user_calls.set(self.user_calls.get() + 1);
        std::future::ready(parse_user(self.user.0, &self.user.1))
    }

    fn contributors(
        &self,
        owner: &str,
        repo: &str,
    ) -> impl Future<Output = Result<Vec<Contributor>, GitHubError>> {
        std::future::ready(match self.contributors.get(&format!("{owner}/{repo}")) {
            Some((status, body)) => parse_contributors(*status, body),
            None => Err(GitHubError::Status(404)),
        })
    }
}

fn repo(name: &str, owner: &str, stars: u64, is_private: bool, languages: &[(&str, u64)]) -> Value {
    json!({
        "name": name,
        "owner": { "login": owner },
        "stargazerCount": stars,
        "url": format!("https://github.com/{owner}/{name}"),
        "languages": {
            "edges": languages
                .iter()
                .map(|(language, size)| json!({ "size": size, "node": { "name": language } }))
                .collect::<Vec<_>>(),
        },
        "pushedAt": "2026-04-09T00:00:00Z",
        "isPrivate": is_private,
        "isFork": false,
    })
}

fn contribution(repository: &Value, occurred_at: &str) -> Value {
    json!({
        "repository": repository,
        "contributions": { "nodes": [{ "occurredAt": occurred_at }] },
    })
}

fn user_payload(public: &[Value], private: &[Value], contributed: &[Value]) -> Value {
    json!({
        "data": {
            "user": {
                "avatarUrl": "https://avatars.example/octocat",
                "name": "The Octocat",
                "bio": null,
                "createdAt": "2011-01-25T18:44:36Z",
                "followers": { "totalCount": 10 },
                "following": { "totalCount": 2 },
                "contributionsCollection": {
                    "contributionCalendar": { "totalContributions": 100 },
                    "totalCommitContributions": 80,
                    "totalPullRequestContributions": 15,
                    "totalIssueContributions": 5,
                    "commitContributionsByRepository": contributed,
                },
                "repositories": { "nodes": private },
                "publicRepositories": { "nodes": public },
            }
        }
    })
}

fn one_public_repo() -> Value {
    user_payload(
        &[repo(
            "hello",
            "octocat",
            100,
            false,
            &[("Rust", 900), ("C", 100)],
        )],
        &[],
        &[],
    )
}

fn request(username: &str) -> StatsRequest<'_> {
    StatsRequest {
        username: Some(username),
        origin: None,
        referer: None,
        client_ip: "203.0.113.1",
        now_secs: NOW,
    }
}

fn as_portfolio(request: StatsRequest<'_>) -> StatsRequest<'_> {
    StatsRequest {
        origin: Some("https://portfolio.example.com"),
        ..request
    }
}

fn service<'a>(
    github: &'a FakeGitHub,
    store: &'a MemoryStore,
    limits: Limits,
) -> Service<'a, FakeGitHub, MemoryStore> {
    Service {
        github,
        store,
        portfolio_origin: PORTFOLIO,
        limits,
    }
}

fn body(reply: &StatsReply) -> Value {
    serde_json::from_str(&reply.body).unwrap()
}

#[tokio::test]
async fn rejects_missing_or_invalid_usernames() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    let mut missing = request("x");
    missing.username = None;
    let error = service.stats(missing).await.unwrap_err();
    assert!(matches!(error, ApiError::InvalidUsername));
    assert_eq!(error.status(), 400);
    assert_eq!(error.to_string(), "Invalid username format");

    for bad in ["", "--bad--", "has space", "-lead", "trail-"] {
        let error = service.stats(request(bad)).await.unwrap_err();
        assert!(matches!(error, ApiError::InvalidUsername), "{bad}");
    }
    assert_eq!(github.user_calls.get(), 0);
}

#[tokio::test]
async fn happy_path_returns_stats() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    let reply = service.stats(request("octocat")).await.unwrap();

    assert_eq!(reply.cache, CacheStatus::Miss);
    assert_eq!(reply.remaining, 9);
    assert_eq!(reply.reset, NOW / 60 * 60 + 60);

    let json = body(&reply);
    let data = &json["data"];
    assert_eq!(json["ok"], true);
    assert_eq!(data["displayName"], "The Octocat");
    assert_eq!(data["bio"], "");
    assert_eq!(data["accountCreatedAt"], "2011-01-25T18:44:36Z");
    assert_eq!(data["totalRepos"], 1);
    assert_eq!(data["totalStars"], 100);
    assert_eq!(data["totalContributions"], 100);
    assert_eq!(data["totalCommits"], 80);
    assert_eq!(data["totalPrs"], 15);
    assert_eq!(data["totalIssues"], 5);
    assert_eq!(data["followers"], 10);
    assert_eq!(data["following"], 2);
    assert_eq!(data["mostStarredRepo"]["name"], "hello");
    assert_eq!(
        data["languages"],
        json!([{ "name": "Rust", "percentage": 90 }, { "name": "C", "percentage": 10 }])
    );
    assert_eq!(data["involvedRepos"][0]["isOwned"], true);
    assert_eq!(data["involvedRepos"][0]["primaryLanguage"], "Rust");
    assert_eq!(
        data["involvedRepos"][0]["lastContributedAt"],
        "2026-04-09T00:00:00Z"
    );
}

#[tokio::test]
async fn repeat_requests_are_served_from_cache_without_calling_github() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    let first = service.stats(request("octocat")).await.unwrap();
    let second = service.stats(request("OctoCat")).await.unwrap();

    assert_eq!(first.cache, CacheStatus::Miss);
    assert_eq!(second.cache, CacheStatus::Hit);
    assert_eq!(second.remaining, 10);
    assert_eq!(second.reset, 60);
    assert_eq!(first.body, second.body);
    assert_eq!(github.user_calls.get(), 1);
    assert!(!second.needs_refresh);
    assert_eq!(
        store.ttl("cache:v3:octocat:public"),
        Some(CACHE_HARD_TTL_SECS)
    );
    assert_eq!(
        store.ttl("cache:v3:octocat:public:meta"),
        Some(CACHE_HARD_TTL_SECS)
    );
}

#[tokio::test]
async fn a_stale_hit_is_still_served_immediately_but_flagged_for_refresh() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    service.stats(request("octocat")).await.unwrap();

    let later = StatsRequest {
        now_secs: NOW + CACHE_TTL_SECS,
        ..request("octocat")
    };
    let reply = service.stats(later).await.unwrap();

    assert_eq!(reply.cache, CacheStatus::Hit);
    assert!(reply.needs_refresh);
    assert_eq!(github.user_calls.get(), 1);
}

#[tokio::test]
async fn only_one_stale_hit_claims_the_refresh() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    service.stats(request("octocat")).await.unwrap();

    let later = StatsRequest {
        now_secs: NOW + CACHE_TTL_SECS,
        ..request("octocat")
    };
    let first_stale = service.stats(later).await.unwrap();
    let second_stale = service.stats(later).await.unwrap();

    assert!(first_stale.needs_refresh);
    assert!(!second_stale.needs_refresh);
}

#[tokio::test]
async fn refresh_overwrites_the_cache_with_fresh_data() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());
    let username: Username = "octocat".parse().unwrap();

    service.stats(request("octocat")).await.unwrap();
    service
        .refresh(&username, Audience::Public, NOW + CACHE_TTL_SECS)
        .await
        .unwrap();

    let reply = service.stats(request("octocat")).await.unwrap();
    assert_eq!(reply.cache, CacheStatus::Hit);
    assert!(!reply.needs_refresh);
    assert_eq!(github.user_calls.get(), 2);
}

#[tokio::test]
async fn private_repos_are_only_visible_to_the_portfolio_audience() {
    let payload = user_payload(
        &[repo("public-one", "octocat", 5, false, &[])],
        &[repo("secret", "octocat", 50, true, &[])],
        &[contribution(
            &repo("secret-work", "acme", 7, true, &[]),
            "2026-05-01T00:00:00Z",
        )],
    );
    let github = FakeGitHub::new(&payload);
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    let public = body(&service.stats(request("octocat")).await.unwrap());
    let portfolio = body(
        &service
            .stats(as_portfolio(request("octocat")))
            .await
            .unwrap(),
    );

    assert_eq!(public["data"]["totalRepos"], 1);
    assert_eq!(public["data"]["totalStars"], 5);
    assert_eq!(portfolio["data"]["totalRepos"], 3);
    assert_eq!(portfolio["data"]["totalStars"], 62);
    assert_eq!(github.user_calls.get(), 2);
    assert!(store.value("cache:v3:octocat:public").is_some());
    assert!(store.value("cache:v3:octocat:private").is_some());
}

#[tokio::test]
async fn counts_repositories_contributed_to_but_not_owned() {
    let payload = user_payload(
        &[repo("mine", "octocat", 1, false, &[])],
        &[],
        &[contribution(
            &repo("theirs", "friend", 40, false, &[]),
            "2026-06-01T00:00:00Z",
        )],
    );
    let github = FakeGitHub::new(&payload);
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    let json = body(&service.stats(request("octocat")).await.unwrap());
    let data = &json["data"];

    assert_eq!(data["totalRepos"], 2);
    assert_eq!(data["totalStars"], 41);
    assert_eq!(data["mostStarredRepo"]["name"], "mine");
    assert_eq!(data["involvedRepos"][0]["name"], "theirs");
    assert_eq!(data["involvedRepos"][0]["isOwned"], false);
}

#[tokio::test]
async fn aggregates_collaborators_from_contributors() {
    let github = FakeGitHub::new(&one_public_repo()).contributors(
        "octocat",
        "hello",
        &json!([
            { "login": "octocat", "avatar_url": "https://a/octocat", "contributions": 10, "type": "User" },
            { "login": "friend", "avatar_url": "https://a/friend", "contributions": 5, "type": "User" },
            { "login": "dependabot[bot]", "avatar_url": "", "contributions": 99, "type": "Bot" },
        ]),
    );
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    let json = body(&service.stats(request("octocat")).await.unwrap());
    let collaborators = json["data"]["collaborators"].as_array().unwrap();

    assert_eq!(collaborators.len(), 1);
    assert_eq!(collaborators[0]["login"], "friend");
    assert_eq!(collaborators[0]["sharedRepos"], 1);
    assert_eq!(collaborators[0]["commits"], 5);
    assert_eq!(collaborators[0]["repos"][0]["name"], "hello");
}

#[tokio::test]
async fn failing_contributors_lookups_yield_no_collaborators_not_an_error() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let service = service(&github, &store, Limits::default());

    let json = body(&service.stats(request("octocat")).await.unwrap());

    assert_eq!(json["data"]["collaborators"], json!([]));
}

#[tokio::test]
async fn maps_github_failures_to_gateway_errors() {
    let rate_limited =
        json!({ "data": null, "errors": [{ "type": "RATE_LIMITED", "message": "slow" }] });
    let internal =
        json!({ "data": { "user": null }, "errors": [{ "type": "INTERNAL", "message": "boom" }] });

    let cases = [
        (
            FakeGitHub::with_raw(403, Vec::new()),
            503,
            "GitHub API rate limit exceeded",
        ),
        (
            FakeGitHub::with_raw(401, Vec::new()),
            502,
            "Failed to fetch data from GitHub",
        ),
        (
            FakeGitHub::with_raw(500, Vec::new()),
            502,
            "Failed to fetch data from GitHub",
        ),
        (
            FakeGitHub::with_raw(200, b"not json".to_vec()),
            502,
            "Failed to fetch data from GitHub",
        ),
        (
            FakeGitHub::new(&rate_limited),
            503,
            "GitHub API rate limit exceeded",
        ),
        (
            FakeGitHub::new(&internal),
            502,
            "Failed to fetch data from GitHub",
        ),
    ];

    for (github, status, message) in cases {
        let store = MemoryStore::default();
        let service = service(&github, &store, Limits::default());

        let error = service.stats(request("octocat")).await.unwrap_err();

        assert_eq!(error.status(), status, "{message}");
        assert_eq!(error.to_string(), message);
        assert!(store.value("cache:v3:octocat:public").is_none());
    }
}

#[tokio::test]
async fn unknown_users_get_a_404() {
    let not_found = json!({
        "data": { "user": null },
        "errors": [{ "type": "NOT_FOUND", "message": "Could not resolve to a User" }],
    });

    for payload in [not_found, json!({ "data": null })] {
        let github = FakeGitHub::new(&payload);
        let store = MemoryStore::default();
        let service = service(&github, &store, Limits::default());

        let error = service.stats(request("nosuchuser")).await.unwrap_err();

        assert!(matches!(error, ApiError::UserNotFound));
        assert_eq!(error.status(), 404);
        assert_eq!(error.to_string(), "User not found");
    }
}

#[tokio::test]
async fn limits_total_requests_per_ip() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let limits = Limits {
        ip_per_window: 1,
        ..Limits::default()
    };
    let service = service(&github, &store, limits);

    service.stats(request("octocat")).await.unwrap();
    let error = service.stats(request("someone")).await.unwrap_err();
    let other_ip = StatsRequest {
        client_ip: "198.51.100.9",
        ..request("octocat")
    };
    let cached = service.stats(other_ip).await.unwrap();

    assert!(matches!(error, ApiError::IpRateLimited));
    assert_eq!(error.status(), 429);
    assert_eq!(error.to_string(), "Too many requests. Slow down.");
    assert_eq!(error.retry_after_secs(), Some(60));
    assert_eq!(cached.cache, CacheStatus::Hit);
}

#[tokio::test]
async fn limits_total_requests_per_username() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let limits = Limits {
        username_per_window: 1,
        ..Limits::default()
    };
    let service = service(&github, &store, limits);

    service.stats(request("octocat")).await.unwrap();
    let other_ip = StatsRequest {
        client_ip: "198.51.100.9",
        ..request("octocat")
    };
    let error = service.stats(as_portfolio(other_ip)).await.unwrap_err();

    assert!(matches!(error, ApiError::UsernameRateLimited));
    assert_eq!(
        error.to_string(),
        "Too many requests for this user. Try again in a minute."
    );
}

#[tokio::test]
async fn blocks_a_client_that_keeps_missing_the_cache_for_one_user() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    let limits = Limits {
        pair_per_window: 1,
        ..Limits::default()
    };
    let service = service(&github, &store, limits);

    service.stats(request("octocat")).await.unwrap();
    let blocked = service
        .stats(as_portfolio(request("octocat")))
        .await
        .unwrap_err();
    let cached = service.stats(request("octocat")).await.unwrap();

    assert!(matches!(blocked, ApiError::Blocked));
    assert_eq!(
        blocked.to_string(),
        "Rate limit exceeded. Try again in 5 minutes."
    );
    assert_eq!(blocked.retry_after_secs(), Some(300));
    assert_eq!(cached.cache, CacheStatus::Hit);
}

#[test]
fn classifies_the_audience_from_origin_and_referer() {
    let classify = |origin, referer| Audience::classify(origin, referer, PORTFOLIO);

    assert_eq!(classify(None, None), Audience::Public);
    assert_eq!(
        classify(Some("https://example.com"), None),
        Audience::Public
    );
    assert_eq!(
        classify(Some("https://portfolio.example.com"), None),
        Audience::Portfolio
    );
    assert_eq!(
        classify(None, Some("https://portfolio.example.com/projects")),
        Audience::Portfolio
    );
    assert_eq!(
        classify(Some("http://localhost:5173"), None),
        Audience::Portfolio
    );
    assert!(Audience::Portfolio.includes_private());
    assert!(!Audience::Public.includes_private());
}

#[tokio::test]
async fn ignores_cache_entries_written_by_earlier_versions() {
    let github = FakeGitHub::new(&one_public_repo());
    let store = MemoryStore::default();
    store
        .put(
            "cache:octocat:public",
            r#"{"totalRepos":1}"#.to_owned(),
            900,
        )
        .await;
    let service = service(&github, &store, Limits::default());

    let reply = service.stats(request("octocat")).await.unwrap();

    assert_eq!(reply.cache, CacheStatus::Miss);
    assert_eq!(body(&reply)["ok"], true);
    assert_eq!(github.user_calls.get(), 1);
}

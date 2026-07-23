use crate::types::{GqlResp, Username};
use worker::{Headers, Method, Request, RequestInit};

const API_URL: &str = "https://api.github.com/graphql";
const QUERY: &str = "
query($u: String!) {
  user(login: $u) {
    avatarUrl name bio createdAt
    followers { totalCount }
    following { totalCount }
    contributionsCollection {
      contributionCalendar { totalContributions }
      totalCommitContributions
      totalPullRequestContributions
      totalIssueContributions
      commitContributionsByRepository(maxRepositories: 100) {
        repository {
          name stargazerCount url pushedAt isPrivate isFork
          owner { login }
          languages(first: 10, orderBy: {field: SIZE, direction: DESC}) {
            edges { size node { name } }
          }
        }
        contributions(first: 1) {
          nodes {
            occurredAt
          }
        }
      }
    }
    repositories(
        first: 100,
        ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER],
        orderBy: {field: PUSHED_AT, direction: DESC},
        privacy: PRIVATE) {
      nodes {
        name stargazerCount url pushedAt isPrivate isFork
        owner { login }
        languages(first: 10, orderBy: {field: SIZE, direction: DESC}) {
          edges { size node { name } }
        }
      }
    }
    publicRepositories: repositories(
        first: 100,
        orderBy: {field: PUSHED_AT, direction: DESC},
        ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER],
        privacy: PUBLIC) {
      nodes {
        name stargazerCount url pushedAt isPrivate isFork
        owner { login }
        languages(first: 10, orderBy: {field: SIZE, direction: DESC}) {
          edges { size node { name } }
        }
      }
    }
  }
}";

pub struct GitHubClient {
    token: String,
}

impl GitHubClient {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    pub async fn fetch(&self, user: &Username) -> Result<GqlResp, worker::Error> {
        let body = serde_json::json!({
            "query": QUERY,
            "variables": { "u": user.as_str() }
        });

        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_body(Some(body.to_string().into()));

        let h = Headers::new();
        h.set("Authorization", &format!("Bearer {}", self.token))?;
        h.set("Content-Type", "application/json")?;
        h.set("User-Agent", "portfolio-worker/1.0")?;
        init.with_headers(h);

        let req = Request::new_with_init(API_URL, &init)?;
        let mut resp = worker::Fetch::Request(req).send().await?;

        match resp.status_code() {
            200 => resp.json().await,
            401 => Err(worker::Error::RustError("GitHub auth failed".into())),
            403 => Err(worker::Error::RustError("GitHub rate limit".into())),
            s => Err(worker::Error::RustError(format!("GitHub error: {}", s))),
        }
    }
}

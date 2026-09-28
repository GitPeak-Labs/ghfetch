use chrono::{DateTime, Utc};
use serde::Deserialize;

pub const QUERY: &str = r"
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
      commitContributionsByRepository(maxRepositories: 50) {
        repository {
          name stargazerCount url pushedAt isPrivate isFork
          owner { login }
          languages(first: 8, orderBy: {field: SIZE, direction: DESC}) {
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
        first: 50,
        ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER],
        orderBy: {field: PUSHED_AT, direction: DESC},
        privacy: PRIVATE) {
      nodes {
        name stargazerCount url pushedAt isPrivate isFork
        owner { login }
        languages(first: 8, orderBy: {field: SIZE, direction: DESC}) {
          edges { size node { name } }
        }
      }
    }
    publicRepositories: repositories(
        first: 50,
        orderBy: {field: PUSHED_AT, direction: DESC},
        ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER],
        privacy: PUBLIC) {
      nodes {
        name stargazerCount url pushedAt isPrivate isFork
        owner { login }
        languages(first: 8, orderBy: {field: SIZE, direction: DESC}) {
          edges { size node { name } }
        }
      }
    }
  }
}";

#[derive(Debug, Deserialize)]
pub struct Response {
    pub data: Option<Data>,
    #[serde(default)]
    pub errors: Vec<Error>,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    pub user: Option<User>,
}

#[derive(Debug, Deserialize)]
pub struct Error {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub avatar_url: String,
    pub name: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
    pub followers: Count,
    pub following: Count,
    pub contributions_collection: ContributionsCollection,
    #[serde(rename = "repositories")]
    pub private_repositories: Nodes<Repo>,
    pub public_repositories: Nodes<Repo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Count {
    pub total_count: u64,
}

#[derive(Debug, Deserialize)]
pub struct Nodes<T> {
    pub nodes: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionsCollection {
    pub contribution_calendar: ContributionCalendar,
    pub total_commit_contributions: u64,
    pub total_pull_request_contributions: u64,
    pub total_issue_contributions: u64,
    pub commit_contributions_by_repository: Vec<CommitContribution>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionCalendar {
    pub total_contributions: u64,
}

#[derive(Debug, Deserialize)]
pub struct CommitContribution {
    pub repository: Repo,
    pub contributions: Option<Nodes<Contribution>>,
}

impl CommitContribution {
    #[must_use]
    pub fn occurred_at(&self) -> Option<DateTime<Utc>> {
        let contributions = self.contributions.as_ref()?;
        contributions.nodes.first().map(|node| node.occurred_at)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contribution {
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub name: String,
    pub owner: Owner,
    pub stargazer_count: u64,
    pub url: String,
    pub languages: Languages,
    pub pushed_at: Option<DateTime<Utc>>,
    pub is_private: bool,
    #[serde(default)]
    pub is_fork: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Owner {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Languages {
    pub edges: Vec<LanguageEdge>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageEdge {
    pub size: u64,
    pub node: LanguageNode,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageNode {
    pub name: String,
}

pub mod collaborators;
pub mod repos;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{
    github::{
        GitHub, GitHubError,
        graphql::{Repo, User},
    },
    username::Username,
};
use collaborators::fetch_collaborators;
use repos::{ProcessedRepos, process_repos};

pub async fn load<G: GitHub>(
    github: &G,
    username: &Username,
    include_private: bool,
) -> Result<Option<GitHubStats>, GitHubError> {
    let Some(user) = github.user(username).await? else {
        return Ok(None);
    };

    let private: &[Repo] = if include_private {
        &user.private_repositories.nodes
    } else {
        &[]
    };
    let contributed: Vec<_> = user
        .contributions_collection
        .commit_contributions_by_repository
        .iter()
        .filter(|entry| include_private || !entry.repository.is_private)
        .map(|entry| (&entry.repository, entry.occurred_at()))
        .collect();

    let processed = process_repos(
        username,
        private,
        &user.public_repositories.nodes,
        &contributed,
    );
    let collaborators =
        fetch_collaborators(github, username, &processed.involved_repos, include_private).await;

    let mut stats = GitHubStats::assemble(&user, username, processed);
    stats.collaborators = collaborators;
    Ok(Some(stats))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubStats {
    pub total_repos: usize,
    pub total_contributions: u64,
    pub languages: Vec<Language>,
    pub total_stars: u64,
    pub followers: u64,
    pub following: u64,
    pub total_commits: u64,
    pub total_prs: u64,
    pub total_issues: u64,
    pub account_created_at: DateTime<Utc>,
    pub most_starred_repo: Option<MostStarredRepo>,
    pub avatar_url: String,
    pub display_name: String,
    pub bio: String,
    pub involved_repos: Vec<InvolvedRepo>,
    pub collaborators: Vec<Collaborator>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Language {
    pub name: String,
    pub percentage: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MostStarredRepo {
    pub name: String,
    pub stars: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvolvedRepo {
    pub name: String,
    pub owner: String,
    pub url: String,
    pub last_contributed_at: DateTime<Utc>,
    pub stars: u64,
    pub primary_language: Option<String>,
    pub is_owned: bool,
    pub is_private: bool,
    pub is_fork: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollabRepo {
    pub name: String,
    pub owner: String,
    pub url: String,
    pub commits: u64,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collaborator {
    pub login: String,
    pub avatar_url: String,
    pub shared_repos: usize,
    pub commits: u64,
    pub repos: Vec<CollabRepo>,
}

impl GitHubStats {
    #[must_use]
    pub fn assemble(user: &User, username: &Username, processed: ProcessedRepos) -> Self {
        let contributions = &user.contributions_collection;

        let languages = processed
            .languages
            .into_iter()
            .map(|(name, share)| Language {
                name,
                percentage: percentage(share),
            })
            .filter(|language| language.percentage > 0)
            .collect();

        Self {
            total_repos: processed.repo_count,
            total_contributions: contributions.contribution_calendar.total_contributions,
            languages,
            total_stars: processed.total_stars,
            followers: user.followers.total_count,
            following: user.following.total_count,
            total_commits: contributions.total_commit_contributions,
            total_prs: contributions.total_pull_request_contributions,
            total_issues: contributions.total_issue_contributions,
            account_created_at: user.created_at,
            most_starred_repo: processed.most_starred_repo,
            avatar_url: user.avatar_url.clone(),
            display_name: user
                .name
                .clone()
                .unwrap_or_else(|| username.as_str().to_owned()),
            bio: user.bio.clone().unwrap_or_default(),
            involved_repos: processed.involved_repos,
            collaborators: Vec::new(),
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn percentage(share: f64) -> u32 {
    (share * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_rounds_to_nearest() {
        assert_eq!(percentage(0.0), 0);
        assert_eq!(percentage(0.004), 0);
        assert_eq!(percentage(0.006), 1);
        assert_eq!(percentage(1.0), 100);
    }
}

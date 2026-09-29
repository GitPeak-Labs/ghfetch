use std::collections::HashSet;

use futures_util::future::join_all;
use indexmap::IndexMap;

use super::{CollabRepo, Collaborator, InvolvedRepo};
use crate::{
    github::{Contributor, GitHub},
    username::Username,
};

const MAX_COLLABORATORS: usize = 10;
const MAX_REPOS: usize = 10;

#[must_use]
pub fn is_eligible(repo: &InvolvedRepo, allow_private: bool) -> bool {
    (!repo.is_private || allow_private) && !repo.is_fork
}

pub async fn fetch_collaborators<G: GitHub>(
    github: &G,
    target: &Username,
    repos: &[InvolvedRepo],
    allow_private: bool,
) -> Vec<Collaborator> {
    let eligible: Vec<&InvolvedRepo> = repos
        .iter()
        .filter(|repo| is_eligible(repo, allow_private))
        .take(MAX_REPOS)
        .collect();

    let per_repo = join_all(eligible.iter().map(|repo| async move {
        github
            .contributors(&repo.owner, &repo.name)
            .await
            .unwrap_or_default()
    }))
    .await;

    aggregate(target, &eligible, &per_repo)
}

struct Accumulator {
    login: String,
    avatar_url: String,
    commits: u64,
    repos: Vec<CollabRepo>,
}

#[must_use]
pub fn aggregate(
    target: &Username,
    repos: &[&InvolvedRepo],
    per_repo: &[Vec<Contributor>],
) -> Vec<Collaborator> {
    let mut aggregated: IndexMap<String, Accumulator> = IndexMap::new();

    for (repo, contributors) in repos.iter().zip(per_repo) {
        let mut seen_in_repo = HashSet::new();

        for contributor in contributors {
            let is_bot = contributor.kind == "Bot" || contributor.login.ends_with("[bot]");
            if is_bot || contributor.login.eq_ignore_ascii_case(target.as_str()) {
                continue;
            }

            let key = contributor.login.to_ascii_lowercase();
            if !seen_in_repo.insert(key.clone()) {
                continue;
            }

            let entry = aggregated.entry(key).or_insert_with(|| Accumulator {
                login: contributor.login.clone(),
                avatar_url: contributor.avatar_url.clone(),
                commits: 0,
                repos: Vec::new(),
            });
            entry.commits += contributor.contributions;
            entry.repos.push(CollabRepo {
                name: repo.name.clone(),
                owner: repo.owner.clone(),
                url: repo.url.clone(),
                commits: contributor.contributions,
                last_activity_at: repo.last_contributed_at,
            });
        }
    }

    let mut collaborators: Vec<Collaborator> = aggregated
        .into_values()
        .map(|entry| Collaborator {
            login: entry.login,
            avatar_url: entry.avatar_url,
            shared_repos: entry.repos.len(),
            commits: entry.commits,
            repos: entry.repos,
        })
        .collect();

    collaborators.sort_by(|a, b| {
        b.shared_repos
            .cmp(&a.shared_repos)
            .then_with(|| b.commits.cmp(&a.commits))
    });
    collaborators.truncate(MAX_COLLABORATORS);
    collaborators
}

#[cfg(test)]
mod tests;

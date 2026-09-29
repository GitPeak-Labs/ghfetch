use std::collections::HashSet;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;

use super::{InvolvedRepo, MostStarredRepo};
use crate::{github::graphql::Repo, username::Username};

const MAX_INVOLVED_REPOS: usize = 15;

type RepoKey = (String, String);

#[derive(Debug, Default)]
pub struct ProcessedRepos {
    pub repo_count: usize,
    pub total_stars: u64,
    pub languages: Vec<(String, f64)>,
    pub most_starred_repo: Option<MostStarredRepo>,
    pub involved_repos: Vec<InvolvedRepo>,
}

#[must_use]
pub fn process_repos(
    target: &Username,
    private: &[Repo],
    public: &[Repo],
    contributed: &[(&Repo, Option<DateTime<Utc>>)],
) -> ProcessedRepos {
    let mut accumulator = Accumulator::new(target);

    for repo in private.iter().chain(public) {
        accumulator.add(repo, repo.pushed_at);
    }
    for (repo, occurred_at) in contributed {
        accumulator.add(repo, occurred_at.or(repo.pushed_at));
    }

    accumulator.finish()
}

struct Accumulator<'a> {
    target: &'a Username,
    seen: HashSet<RepoKey>,
    total_stars: u64,
    most_starred: Option<MostStarredRepo>,
    language_shares: IndexMap<String, f64>,
    repos_with_languages: usize,
    involved: IndexMap<RepoKey, InvolvedRepo>,
}

impl<'a> Accumulator<'a> {
    fn new(target: &'a Username) -> Self {
        Self {
            target,
            seen: HashSet::new(),
            total_stars: 0,
            most_starred: None,
            language_shares: IndexMap::new(),
            repos_with_languages: 0,
            involved: IndexMap::new(),
        }
    }

    fn is_owned(&self, repo: &Repo) -> bool {
        repo.owner.login.eq_ignore_ascii_case(self.target.as_str())
    }

    fn add(&mut self, repo: &Repo, involved_at: Option<DateTime<Utc>>) {
        let key = (
            repo.owner.login.to_ascii_lowercase(),
            repo.name.to_ascii_lowercase(),
        );

        if self.seen.insert(key.clone()) {
            self.add_stats(repo);
        }
        if let Some(at) = involved_at {
            self.add_involved(key, repo, at);
        }
    }

    fn add_stats(&mut self, repo: &Repo) {
        self.total_stars += repo.stargazer_count;

        let best = self.most_starred.as_ref().map_or(0, |top| top.stars);
        if self.is_owned(repo) && repo.stargazer_count > best {
            self.most_starred = Some(MostStarredRepo {
                name: repo.name.clone(),
                stars: repo.stargazer_count,
                url: repo.url.clone(),
            });
        }

        let total_bytes: u64 = repo.languages.edges.iter().map(|edge| edge.size).sum();
        if total_bytes == 0 {
            return;
        }

        self.repos_with_languages += 1;
        for edge in &repo.languages.edges {
            let share = ratio(edge.size, total_bytes);
            *self
                .language_shares
                .entry(edge.node.name.clone())
                .or_default() += share;
        }
    }

    fn add_involved(&mut self, key: RepoKey, repo: &Repo, at: DateTime<Utc>) {
        let is_owned = self.is_owned(repo);

        self.involved
            .entry(key)
            .and_modify(|existing| {
                existing.last_contributed_at = existing.last_contributed_at.max(at);
            })
            .or_insert_with(|| InvolvedRepo {
                name: repo.name.clone(),
                owner: repo.owner.login.clone(),
                url: repo.url.clone(),
                last_contributed_at: at,
                stars: repo.stargazer_count,
                primary_language: repo
                    .languages
                    .edges
                    .first()
                    .map(|edge| edge.node.name.clone()),
                is_owned,
                is_private: repo.is_private,
                is_fork: repo.is_fork,
            });
    }

    fn finish(self) -> ProcessedRepos {
        let mut languages: Vec<(String, f64)> = self
            .language_shares
            .into_iter()
            .map(|(name, total)| (name, total / count_as_f64(self.repos_with_languages)))
            .collect();
        languages.sort_by(|a, b| b.1.total_cmp(&a.1));

        let mut involved_repos: Vec<InvolvedRepo> = self.involved.into_values().collect();
        involved_repos.sort_by_key(|repo| std::cmp::Reverse(repo.last_contributed_at));
        involved_repos.truncate(MAX_INVOLVED_REPOS);

        ProcessedRepos {
            repo_count: self.seen.len(),
            total_stars: self.total_stars,
            languages,
            most_starred_repo: self.most_starred,
            involved_repos,
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn ratio(part: u64, whole: u64) -> f64 {
    part as f64 / whole as f64
}

#[allow(clippy::cast_precision_loss)]
fn count_as_f64(count: usize) -> f64 {
    count as f64
}

#[cfg(test)]
mod tests;

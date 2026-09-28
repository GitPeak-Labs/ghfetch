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
mod tests {
    use super::*;
    use crate::github::graphql::{LanguageEdge, LanguageNode, Languages, Owner};

    fn at(iso: &str) -> DateTime<Utc> {
        iso.parse().unwrap()
    }

    fn repo(name: &str, stars: u64, owner: &str, languages: &[(&str, u64)]) -> Repo {
        Repo {
            name: name.to_owned(),
            owner: Owner {
                login: owner.to_owned(),
            },
            stargazer_count: stars,
            url: format!("https://github.com/{owner}/{name}"),
            languages: Languages {
                edges: languages
                    .iter()
                    .map(|&(language, size)| LanguageEdge {
                        size,
                        node: LanguageNode {
                            name: language.to_owned(),
                        },
                    })
                    .collect(),
            },
            pushed_at: Some(at("2026-04-09T00:00:00Z")),
            is_private: false,
            is_fork: false,
        }
    }

    fn user() -> Username {
        "user".parse().unwrap()
    }

    fn share(processed: &ProcessedRepos, language: &str) -> f64 {
        processed
            .languages
            .iter()
            .find(|(name, _)| name == language)
            .map_or(0.0, |(_, share)| *share)
    }

    #[test]
    fn counts_unique_repos() {
        let repos = [
            repo("repo-a", 5, "user", &[]),
            repo("repo-b", 3, "user", &[]),
        ];
        assert_eq!(process_repos(&user(), &repos, &[], &[]).repo_count, 2);
    }

    #[test]
    fn deduplicates_by_owner_and_name() {
        let private = [repo("repo-a", 5, "user", &[])];
        let public = [repo("Repo-A", 5, "User", &[])];
        let processed = process_repos(&user(), &private, &public, &[]);
        assert_eq!(processed.repo_count, 1);
        assert_eq!(processed.total_stars, 5);
    }

    #[test]
    fn same_name_under_different_owners_counts_twice() {
        let repos = [
            repo("dotfiles", 1, "user", &[]),
            repo("dotfiles", 2, "org", &[]),
        ];
        let processed = process_repos(&user(), &repos, &[], &[]);
        assert_eq!(processed.repo_count, 2);
        assert_eq!(processed.total_stars, 3);
    }

    #[test]
    fn sums_stars() {
        let repos = [repo("a", 10, "user", &[]), repo("b", 20, "user", &[])];
        assert_eq!(process_repos(&user(), &repos, &[], &[]).total_stars, 30);
    }

    #[test]
    fn finds_most_starred() {
        let repos = [
            repo("low", 1, "user", &[]),
            repo("high", 99, "user", &[]),
            repo("mid", 50, "user", &[]),
        ];
        let processed = process_repos(&user(), &repos, &[], &[]);
        assert_eq!(processed.most_starred_repo.unwrap().name, "high");
    }

    #[test]
    fn most_starred_must_be_owned() {
        let owned = [repo("owned", 10, "user", &[])];
        let other = repo("other", 100, "someone-else", &[]);
        let processed = process_repos(&user(), &owned, &[], &[(&other, None)]);

        assert_eq!(processed.total_stars, 110);
        assert_eq!(processed.most_starred_repo.unwrap().name, "owned");
    }

    #[test]
    fn averages_language_shares_across_repos() {
        let repos = [
            repo("a", 0, "user", &[("Rust", 1000), ("C", 500)]),
            repo("b", 0, "user", &[("Rust", 500)]),
        ];
        let processed = process_repos(&user(), &repos, &[], &[]);

        assert!((share(&processed, "Rust") - 0.833).abs() < 0.01);
        assert!((share(&processed, "C") - 0.167).abs() < 0.01);
    }

    #[test]
    fn sorts_languages_by_share_descending() {
        let repos = [repo(
            "r",
            0,
            "user",
            &[("C", 100), ("Rust", 900), ("Python", 500)],
        )];
        let processed = process_repos(&user(), &repos, &[], &[]);
        let order: Vec<_> = processed
            .languages
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(order, ["Rust", "Python", "C"]);
    }

    #[test]
    fn empty_repos_do_not_dilute_language_percentages() {
        let repos = [
            repo("a", 0, "user", &[("Rust", 1000)]),
            repo("b", 0, "user", &[]),
        ];
        let processed = process_repos(&user(), &repos, &[], &[]);
        assert!((share(&processed, "Rust") - 1.0).abs() < 0.01);
    }

    #[test]
    fn includes_owned_repos_as_involved() {
        let owned = [repo("owned-repo", 10, "user", &[("Rust", 1)])];
        let processed = process_repos(&user(), &owned, &[], &[]);

        let involved = &processed.involved_repos;
        assert_eq!(involved.len(), 1);
        assert_eq!(involved[0].name, "owned-repo");
        assert!(involved[0].is_owned);
        assert_eq!(involved[0].primary_language.as_deref(), Some("Rust"));
    }

    #[test]
    fn marks_external_repos_as_not_owned() {
        let external = repo("other", 100, "someone-else", &[]);
        let contributed = [(&external, Some(at("2026-05-01T00:00:00Z")))];
        let processed = process_repos(&user(), &[], &[], &contributed);

        let involved = &processed.involved_repos;
        assert_eq!(involved.len(), 1);
        assert_eq!(involved[0].owner, "someone-else");
        assert!(!involved[0].is_owned);
        assert_eq!(involved[0].last_contributed_at, at("2026-05-01T00:00:00Z"));
    }

    #[test]
    fn contributing_to_an_owned_repo_keeps_the_latest_date() {
        let owned = [repo("mine", 0, "user", &[])];
        let same = repo("mine", 0, "user", &[]);
        let contributed = [(&same, Some(at("2026-06-01T00:00:00Z")))];
        let processed = process_repos(&user(), &owned, &[], &contributed);

        assert_eq!(processed.repo_count, 1);
        assert_eq!(processed.involved_repos.len(), 1);
        assert_eq!(
            processed.involved_repos[0].last_contributed_at,
            at("2026-06-01T00:00:00Z")
        );
    }

    #[test]
    fn involved_repos_are_newest_first_and_capped() {
        let mut repos: Vec<Repo> = (0..20)
            .map(|i| repo(&format!("r{i}"), 0, "user", &[]))
            .collect();
        for (i, repo) in repos.iter_mut().enumerate() {
            repo.pushed_at = Some(
                at("2026-01-01T00:00:00Z") + chrono::Duration::days(i64::try_from(i).unwrap()),
            );
        }
        let processed = process_repos(&user(), &repos, &[], &[]);

        assert_eq!(processed.involved_repos.len(), MAX_INVOLVED_REPOS);
        assert_eq!(processed.involved_repos[0].name, "r19");
    }

    #[test]
    fn handles_empty_input() {
        let processed = process_repos(&user(), &[], &[], &[]);
        assert_eq!(processed.repo_count, 0);
        assert_eq!(processed.total_stars, 0);
        assert!(processed.languages.is_empty());
        assert!(processed.most_starred_repo.is_none());
        assert!(processed.involved_repos.is_empty());
    }

    #[test]
    fn sums_large_star_counts_exactly() {
        let max = u64::from(u32::MAX);
        let repos = [repo("a", max, "user", &[]), repo("b", max, "user", &[])];
        assert_eq!(
            process_repos(&user(), &repos, &[], &[]).total_stars,
            max * 2
        );
    }
}

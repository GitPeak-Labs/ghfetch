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
        repo.pushed_at =
            Some(at("2026-01-01T00:00:00Z") + chrono::Duration::days(i64::try_from(i).unwrap()));
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

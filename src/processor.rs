use crate::types::{GqlUser, Repo};
use shared::github::{GitHubLanguage, GitHubStats, MostStarredRepo, InvolvedRepo};
use std::collections::{BTreeMap, BTreeSet};
use std::cmp::Ordering;

pub struct ProcessedRepos {
    pub repo_count: u32,
    pub total_stars: u32,
    pub languages: Vec<(String, f64)>,
    pub most_starred_repo: Option<MostStarredRepo>,
    pub involved_repos: Vec<InvolvedRepo>,
}

pub fn process_repos(
    target_user: &str,
    private: &[Repo],
    public: &[Repo],
    contributed: &[(Repo, Option<String>)],
) -> ProcessedRepos {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut lang_shares: BTreeMap<String, f64> = BTreeMap::new();
    let mut total_stars: u32 = 0;
    let mut top: Option<(&str, u32, &str)> = None;
    let mut repos_with_langs: u32 = 0;

    let mut involved_map: BTreeMap<String, InvolvedRepo> = BTreeMap::new();

    let mut add_involved = |r: &Repo, date: String| {
        let is_owned = r.owner.login.eq_ignore_ascii_case(target_user);

        let key = format!("{}/{}", r.owner.login.to_lowercase(), r.name.to_lowercase());
        let primary_lang = r.languages.edges.first().map(|e| e.node.name.clone());

        let new_item = InvolvedRepo {
            name: r.name.clone(),
            owner: r.owner.login.clone(),
            url: r.url.clone(),
            last_contributed_at: date,
            stars: r.stargazer_count,
            primary_language: primary_lang,
            is_owned,
            is_private: r.is_private,
        };

        involved_map
            .entry(key)
            .and_modify(|existing| {
                if new_item.last_contributed_at > existing.last_contributed_at {
                    existing.last_contributed_at = new_item.last_contributed_at.clone();
                }
            })
            .or_insert(new_item);
    };

    // 1. Process private repos (general stats only)
    for r in private {
        let key_name = r.name.to_lowercase();
        if !seen.insert(key_name) {
            continue;
        }

        if let Some(ref pushed_at) = r.pushed_at {
            add_involved(r, pushed_at.clone());
        }

        total_stars = total_stars.saturating_add(r.stargazer_count);

        let best = top.as_ref().map(|(_, s, _)| *s).unwrap_or(0);
        if r.owner.login.eq_ignore_ascii_case(target_user) && r.stargazer_count > best {
            top = Some((r.name.as_str(), r.stargazer_count, r.url.as_str()));
        }

        let total_repo_bytes: u64 = r.languages.edges.iter().map(|e| e.size).sum();
        if total_repo_bytes > 0 {
            repos_with_langs += 1;
            for e in &r.languages.edges {
                let share = e.size as f64 / total_repo_bytes as f64;
                *lang_shares.entry(e.node.name.clone()).or_insert(0.0) += share;
            }
        }
    }

    // 2. Process public repos (general stats only)
    for r in public {
        let key_name = r.name.to_lowercase();
        if !seen.insert(key_name) {
            continue;
        }

        if let Some(ref pushed_at) = r.pushed_at {
            add_involved(r, pushed_at.clone());
        }

        total_stars = total_stars.saturating_add(r.stargazer_count);

        let best = top.as_ref().map(|(_, s, _)| *s).unwrap_or(0);
        if r.owner.login.eq_ignore_ascii_case(target_user) && r.stargazer_count > best {
            top = Some((r.name.as_str(), r.stargazer_count, r.url.as_str()));
        }

        let total_repo_bytes: u64 = r.languages.edges.iter().map(|e| e.size).sum();
        if total_repo_bytes > 0 {
            repos_with_langs += 1;
            for e in &r.languages.edges {
                let share = e.size as f64 / total_repo_bytes as f64;
                *lang_shares.entry(e.node.name.clone()).or_insert(0.0) += share;
            }
        }
    }

    // 3. Process contributed repos (actual personal commit targets)
    for (r, occurred_at) in contributed {
        let key_name = r.name.to_lowercase();
        if seen.insert(key_name) {
            total_stars = total_stars.saturating_add(r.stargazer_count);

            let best = top.as_ref().map(|(_, s, _)| *s).unwrap_or(0);
            if r.owner.login.eq_ignore_ascii_case(target_user) && r.stargazer_count > best {
                top = Some((r.name.as_str(), r.stargazer_count, r.url.as_str()));
            }

            let total_repo_bytes: u64 = r.languages.edges.iter().map(|e| e.size).sum();
            if total_repo_bytes > 0 {
                repos_with_langs += 1;
                for e in &r.languages.edges {
                    let share = e.size as f64 / total_repo_bytes as f64;
                    *lang_shares.entry(e.node.name.clone()).or_insert(0.0) += share;
                }
            }
        }

        let date = occurred_at.clone().or(r.pushed_at.clone()).unwrap_or_default();
        if !date.is_empty() {
            add_involved(r, date);
        }
    }

    let cnt = seen.len() as u32;

    let mut langs: Vec<(String, f64)> = if repos_with_langs > 0 {
        let denom = repos_with_langs as f64;
        lang_shares
            .into_iter()
            .map(|(name, total_share)| (name, total_share / denom))
            .collect()
    } else {
        Vec::new()
    };

    langs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    let most_starred = top.map(|(n, s, u)| MostStarredRepo {
        name: n.to_string(),
        stars: s,
        url: u.to_string(),
    });

    let mut involved_repos: Vec<InvolvedRepo> = involved_map.into_values().collect();
    involved_repos.sort_by(|a, b| b.last_contributed_at.cmp(&a.last_contributed_at));
    involved_repos.truncate(15);

    ProcessedRepos {
        repo_count: cnt,
        total_stars,
        languages: langs,
        most_starred_repo: most_starred,
        involved_repos,
    }
}

pub fn build_stats(
    user: GqlUser,
    username: &str,
    repo_cnt: u32,
    total_stars: u32,
    langs: Vec<(String, f64)>,
    top_repo: Option<MostStarredRepo>,
    involved_repos: Vec<InvolvedRepo>,
) -> GitHubStats {
    let languages: Vec<GitHubLanguage> = langs
        .into_iter()
        .filter_map(|(name, avg_share)| {
            let pct = (avg_share * 100.0).round() as u32;
            if pct > 0 {
                Some(GitHubLanguage { name, percentage: pct })
            } else {
                None
            }
        })
        .collect();

    GitHubStats {
        total_repos: repo_cnt,
        total_contributions: user.contributions_collection
            .contribution_calendar.total_contributions,
        total_stars,
        followers: user.followers.total_count,
        following: user.following.total_count,
        total_commits: user.contributions_collection
            .total_commit_contributions,
        total_prs: user.contributions_collection
            .total_pull_request_contributions,
        total_issues: user.contributions_collection
            .total_issue_contributions,
        account_created_at: user.created_at,
        most_starred_repo: top_repo,
        avatar_url: user.avatar_url,
        display_name: user.name.unwrap_or_else(|| username.to_string()),
        bio: user.bio.unwrap_or_default(),
        languages,
        involved_repos,
        collaborators: Vec::new(),
    }
}

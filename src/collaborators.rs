use futures::future::join_all;
use serde::Deserialize;
use shared::github::{CollabRepo, Collaborator, InvolvedRepo};
use std::collections::{BTreeMap, BTreeSet};
use worker::{Headers, Method, Request, RequestInit};

const MAX_COLLABORATORS: usize = 10;

#[derive(Deserialize)]
struct ContributorEntry {
    login: String,
    #[serde(rename = "avatar_url", default)]
    avatar_url: String,
    #[serde(default)]
    contributions: u32,
    #[serde(rename = "type", default)]
    account_type: String,
}

async fn fetch_repo_contributors(token: &str, owner: &str, name: &str) -> Vec<ContributorEntry> {
    let url = format!("https://api.github.com/repos/{owner}/{name}/contributors?per_page=100&anon=false");

    let mut init = RequestInit::new();
    init.with_method(Method::Get);

    let h = Headers::new();
    if h.set("Authorization", &format!("Bearer {token}")).is_err() {
        return Vec::new();
    }
    let _ = h.set("User-Agent", "ghfetch-worker/1.0");
    let _ = h.set("Accept", "application/vnd.github+json");
    init.with_headers(h);

    let req = match Request::new_with_init(&url, &init) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut resp = match worker::Fetch::Request(req).send().await {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    if resp.status_code() != 200 {
        return Vec::new();
    }

    resp.json::<Vec<ContributorEntry>>().await.unwrap_or_default()
}

struct RepoRef {
    name: String,
    owner: String,
    url: String,
    last_activity_at: String,
}

fn aggregate(target_user: &str, repos: Vec<RepoRef>, per_repo: Vec<Vec<ContributorEntry>>) -> Vec<Collaborator> {
    let mut agg: BTreeMap<String, (String, String, u32, Vec<CollabRepo>)> = BTreeMap::new();

    for (repo, entries) in repos.into_iter().zip(per_repo) {
        let mut seen_this_repo: BTreeSet<String> = BTreeSet::new();
        for e in entries {
            if e.account_type == "Bot" || e.login.ends_with("[bot]") {
                continue;
            }
            if e.login.eq_ignore_ascii_case(target_user) {
                continue;
            }

            let key = e.login.to_lowercase();
            if !seen_this_repo.insert(key.clone()) {
                continue;
            }

            let entry = agg
                .entry(key)
                .or_insert_with(|| (e.login.clone(), e.avatar_url.clone(), 0, Vec::new()));
            entry.2 = entry.2.saturating_add(e.contributions);
            entry.3.push(CollabRepo {
                name: repo.name.clone(),
                owner: repo.owner.clone(),
                url: repo.url.clone(),
                commits: e.contributions,
                last_activity_at: repo.last_activity_at.clone(),
            });
        }
    }

    let mut list: Vec<Collaborator> = agg
        .into_values()
        .map(|(login, avatar_url, commits, repos)| Collaborator {
            login,
            avatar_url,
            shared_repos: repos.len() as u32,
            commits,
            repos,
        })
        .collect();

    list.sort_by(|a, b| {
        b.shared_repos
            .cmp(&a.shared_repos)
            .then(b.commits.cmp(&a.commits))
    });
    list.truncate(MAX_COLLABORATORS);
    list
}

pub async fn fetch_collaborators(
    token: &str,
    target_user: &str,
    repos: &[InvolvedRepo],
    allow_private: bool,
) -> Vec<Collaborator> {
    let eligible: Vec<&InvolvedRepo> = repos.iter().filter(|r| !r.is_private || allow_private).collect();

    let futs = eligible
        .iter()
        .map(|r| fetch_repo_contributors(token, &r.owner, &r.name));
    let per_repo = join_all(futs).await;

    let repo_refs: Vec<RepoRef> = eligible
        .into_iter()
        .map(|r| RepoRef {
            name: r.name.clone(),
            owner: r.owner.clone(),
            url: r.url.clone(),
            last_activity_at: r.last_contributed_at.clone(),
        })
        .collect();

    aggregate(target_user, repo_refs, per_repo)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(login: &str, contributions: u32, account_type: &str) -> ContributorEntry {
        ContributorEntry {
            login: login.to_string(),
            avatar_url: format!("https://avatars/{login}"),
            contributions,
            account_type: account_type.to_string(),
        }
    }

    fn repo(name: &str) -> RepoRef {
        RepoRef {
            name: name.to_string(),
            owner: "owner".to_string(),
            url: format!("https://github.com/owner/{name}"),
            last_activity_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn excludes_target_user() {
        let repos = vec![repo("r1")];
        let per_repo = vec![vec![entry("target", 10, "User"), entry("friend", 5, "User")]];
        let result = aggregate("target", repos, per_repo);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].login, "friend");
    }

    #[test]
    fn excludes_bots() {
        let repos = vec![repo("r1")];
        let per_repo = vec![vec![
            entry("dependabot[bot]", 50, "Bot"),
            entry("friend", 5, "User"),
        ]];
        let result = aggregate("target", repos, per_repo);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].login, "friend");
    }

    #[test]
    fn counts_shared_repos_and_sums_commits() {
        let repos = vec![repo("r1"), repo("r2")];
        let per_repo = vec![
            vec![entry("friend", 10, "User")],
            vec![entry("friend", 7, "User")],
        ];
        let result = aggregate("target", repos, per_repo);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].shared_repos, 2);
        assert_eq!(result[0].commits, 17);
        assert_eq!(result[0].repos.len(), 2);
        assert_eq!(result[0].repos[0].name, "r1");
        assert_eq!(result[0].repos[0].commits, 10);
        assert_eq!(result[0].repos[0].last_activity_at, "2026-01-01T00:00:00Z");
        assert_eq!(result[0].repos[1].name, "r2");
        assert_eq!(result[0].repos[1].commits, 7);
    }

    #[test]
    fn sorts_by_shared_repos_then_commits() {
        let repos = vec![repo("r1"), repo("r2")];
        let per_repo = vec![
            vec![entry("a", 100, "User"), entry("b", 1, "User")],
            vec![entry("b", 1, "User")],
        ];
        let result = aggregate("target", repos, per_repo);
        assert_eq!(result[0].login, "b");
        assert_eq!(result[1].login, "a");
    }

    #[test]
    fn caps_at_max_collaborators() {
        let repos = vec![repo("r1")];
        let entries: Vec<ContributorEntry> = (0..20)
            .map(|i| entry(&format!("user{i}"), i, "User"))
            .collect();
        let result = aggregate("target", repos, vec![entries]);
        assert_eq!(result.len(), MAX_COLLABORATORS);
    }
}

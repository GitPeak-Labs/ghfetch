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
mod tests {
    use super::*;

    fn involved(name: &str, is_private: bool, is_fork: bool) -> InvolvedRepo {
        InvolvedRepo {
            name: name.to_owned(),
            owner: "owner".to_owned(),
            url: format!("https://github.com/owner/{name}"),
            last_contributed_at: "2026-01-01T00:00:00Z".parse().unwrap(),
            stars: 0,
            primary_language: None,
            is_owned: true,
            is_private,
            is_fork,
        }
    }

    fn contributor(login: &str, contributions: u64, kind: &str) -> Contributor {
        Contributor {
            login: login.to_owned(),
            avatar_url: format!("https://avatars/{login}"),
            contributions,
            kind: kind.to_owned(),
        }
    }

    fn target() -> Username {
        "target".parse().unwrap()
    }

    #[test]
    fn excludes_the_target_user() {
        let repo = involved("r1", false, false);
        let per_repo = [vec![
            contributor("Target", 10, "User"),
            contributor("friend", 5, "User"),
        ]];
        let result = aggregate(&target(), &[&repo], &per_repo);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].login, "friend");
    }

    #[test]
    fn excludes_bots() {
        let repo = involved("r1", false, false);
        let per_repo = [vec![
            contributor("dependabot[bot]", 50, "Bot"),
            contributor("renovate[bot]", 40, "User"),
            contributor("friend", 5, "User"),
        ]];
        let result = aggregate(&target(), &[&repo], &per_repo);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].login, "friend");
    }

    #[test]
    fn counts_shared_repos_and_sums_commits() {
        let (r1, r2) = (involved("r1", false, false), involved("r2", false, false));
        let per_repo = [
            vec![contributor("friend", 10, "User")],
            vec![contributor("Friend", 7, "User")],
        ];
        let result = aggregate(&target(), &[&r1, &r2], &per_repo);

        assert_eq!(result.len(), 1);
        let friend = &result[0];
        assert_eq!(friend.shared_repos, 2);
        assert_eq!(friend.commits, 17);
        assert_eq!(friend.repos[0].name, "r1");
        assert_eq!(friend.repos[0].commits, 10);
        assert_eq!(friend.repos[1].name, "r2");
        assert_eq!(friend.repos[1].commits, 7);
        assert_eq!(friend.repos[0].last_activity_at, r1.last_contributed_at);
    }

    #[test]
    fn sorts_by_shared_repos_then_commits() {
        let (r1, r2) = (involved("r1", false, false), involved("r2", false, false));
        let per_repo = [
            vec![contributor("a", 100, "User"), contributor("b", 1, "User")],
            vec![contributor("b", 1, "User")],
        ];
        let result = aggregate(&target(), &[&r1, &r2], &per_repo);

        let order: Vec<_> = result.iter().map(|c| c.login.as_str()).collect();
        assert_eq!(order, ["b", "a"]);
    }

    #[test]
    fn caps_at_max_collaborators() {
        let repo = involved("r1", false, false);
        let contributors: Vec<_> = (0..20)
            .map(|i| contributor(&format!("user{i}"), i, "User"))
            .collect();
        let result = aggregate(&target(), &[&repo], &[contributors]);

        assert_eq!(result.len(), MAX_COLLABORATORS);
    }

    #[test]
    fn forks_are_never_eligible() {
        assert!(!is_eligible(&involved("f", false, true), false));
        assert!(!is_eligible(&involved("f", false, true), true));
        assert!(!is_eligible(&involved("f", true, true), true));
    }

    #[test]
    fn private_repos_need_permission() {
        assert!(is_eligible(&involved("n", false, false), false));
        assert!(!is_eligible(&involved("n", true, false), false));
        assert!(is_eligible(&involved("n", true, false), true));
    }
}

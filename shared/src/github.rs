use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitHubLanguage {
    pub name: String,
    pub percentage: u32,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitHubStats {
    pub total_repos: u32,
    pub total_contributions: u32,
    pub languages: Vec<GitHubLanguage>,
    pub total_stars: u32,
    pub followers: u32,
    pub following: u32,
    pub total_commits: u32,
    pub total_prs: u32,
    pub total_issues: u32,
    pub account_created_at: String,
    pub most_starred_repo: Option<MostStarredRepo>,
    pub avatar_url: String,
    pub display_name: String,
    pub bio: String,
    #[serde(default)]
    pub involved_repos: Vec<InvolvedRepo>,
    #[serde(default)]
    pub collaborators: Vec<Collaborator>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MostStarredRepo {
    pub name: String,
    pub stars: u32,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct InvolvedRepo {
    pub name: String,
    pub owner: String,
    pub url: String,
    pub last_contributed_at: String,
    pub stars: u32,
    pub primary_language: Option<String>,
    pub is_owned: bool,
    #[serde(default)]
    pub is_private: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Collaborator {
    pub login: String,
    pub avatar_url: String,
    pub shared_repos: u32,
    pub commits: u32,
    #[serde(default)]
    pub repos: Vec<CollabRepo>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct CollabRepo {
    pub name: String,
    pub owner: String,
    pub url: String,
    pub commits: u32,
    #[serde(default)]
    pub last_activity_at: String,
}

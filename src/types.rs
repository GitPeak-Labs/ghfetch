use serde::Deserialize;

#[derive(Debug, Clone, Copy)]
pub struct RateConfig {
    pub max_req: u32,
    pub window_secs: u64,
    pub block_secs: u64,
}

impl Default for RateConfig {
    fn default() -> Self {
        Self {
            max_req: 10,
            window_secs: 60,
            block_secs: 300,
        }
    }
}

#[derive(Debug)]
pub struct ClientId(pub String);

impl ClientId {
    pub fn from_req(req: &worker::Request) -> Self {
        let ip = req
            .headers()
            .get("CF-Connecting-IP")
            .unwrap_or(None)
            .unwrap_or_else(|| "unknown".to_string());
        Self(ip)
    }
}

#[derive(Debug)]
pub struct Username(String);

impl Username {
    pub fn new(raw: &str) -> Option<Self> {
        let t = raw.trim();
        if t.is_empty() || t.len() > 39 {
            return None;
        }
        let c: Vec<char> = t.chars().collect();
        if c.first() == Some(&'-') || c.last() == Some(&'-') {
            return None;
        }
        if c.windows(2).any(|w| w == ['-', '-']) {
            return None;
        }
        if !c.iter().all(|&ch| ch.is_alphanumeric() || ch == '-') {
            return None;
        }
        Some(Self(t.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Deserialize)]
pub struct GqlResp {
    pub data: Option<GqlData>,
}

#[derive(Deserialize)]
pub struct GqlData {
    pub user: GqlUser,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GqlUser {
    pub avatar_url: String,
    pub name: Option<String>,
    pub bio: Option<String>,
    pub created_at: String,
    pub followers: CountConn,
    pub following: CountConn,
    pub contributions_collection: ContribColl,
    pub repositories: RepoConn,
    pub public_repositories: RepoConn,
}

#[derive(Deserialize)]
pub struct CountConn {
    #[serde(rename = "totalCount")]
    pub total_count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContribColl {
    pub contribution_calendar: Calendar,
    pub total_commit_contributions: u32,
    pub total_pull_request_contributions: u32,
    pub total_issue_contributions: u32,
    pub commit_contributions_by_repository: Vec<CommitContribByRepo>,
}

#[derive(Deserialize, Clone)]
pub struct CommitContribByRepo {
    pub repository: Repo,
    #[serde(default)]
    pub contributions: Option<ContribConn>,
}

#[derive(Deserialize, Clone)]
pub struct ContribConn {
    pub nodes: Vec<ContribNode>,
}

#[derive(Deserialize, Clone)]
pub struct ContribNode {
    #[serde(rename = "occurredAt")]
    pub occurred_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Calendar {
    pub total_contributions: u32,
}

#[derive(Deserialize)]
pub struct RepoConn {
    pub nodes: Vec<Repo>,
}

#[derive(Deserialize, Clone)]
pub struct Repo {
    pub name: String,
    pub owner: Owner,
    #[serde(rename = "stargazerCount")]
    pub stargazer_count: u32,
    pub url: String,
    pub languages: LangConn,
    #[serde(rename = "pushedAt")]
    pub pushed_at: Option<String>,
    #[serde(rename = "isPrivate")]
    pub is_private: bool,
    #[serde(rename = "isFork", default)]
    pub is_fork: bool,
}

#[derive(Deserialize, Clone)]
pub struct Owner {
    pub login: String,
}

#[derive(Deserialize, Clone)]
pub struct LangConn {
    pub edges: Vec<LangEdge>,
}

#[derive(Deserialize, Clone)]
pub struct LangEdge {
    pub size: u64,
    pub node: LangNode,
}

#[derive(Deserialize, Clone)]
pub struct LangNode {
    pub name: String,
}

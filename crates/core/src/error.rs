use crate::github::GitHubError;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Invalid username format")]
    InvalidUsername,
    #[error("Not found")]
    NotFound,
    #[error("User not found")]
    UserNotFound,
    #[error("Too many requests. Slow down.")]
    IpRateLimited,
    #[error("Too many requests for this user. Try again in a minute.")]
    UsernameRateLimited,
    #[error("Rate limit exceeded. Try again in 5 minutes.")]
    Blocked,
    #[error("GitHub API rate limit exceeded")]
    UpstreamRateLimited,
    #[error("Failed to fetch data from GitHub")]
    Upstream(#[source] GitHubError),
    #[error("Server configuration error")]
    Misconfigured,
    #[error("Internal server error")]
    Internal,
}

impl ApiError {
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::InvalidUsername => 400,
            Self::NotFound | Self::UserNotFound => 404,
            Self::IpRateLimited | Self::UsernameRateLimited | Self::Blocked => 429,
            Self::UpstreamRateLimited => 503,
            Self::Upstream(_) => 502,
            Self::Misconfigured | Self::Internal => 500,
        }
    }

    #[must_use]
    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::IpRateLimited | Self::UsernameRateLimited => Some(60),
            Self::Blocked => Some(300),
            _ => None,
        }
    }
}

impl From<GitHubError> for ApiError {
    fn from(error: GitHubError) -> Self {
        match error {
            GitHubError::RateLimited => Self::UpstreamRateLimited,
            other => Self::Upstream(other),
        }
    }
}

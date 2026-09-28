use std::{pin::pin, time::Duration};

use futures_util::future::{Either, select};
use ghfetch_core::{
    github::{self, Contributor, GitHub, GitHubError},
    username::Username,
};
use worker::{
    AbortController, Delay, Fetch, Headers, Method, Request, RequestInit, console_error,
    wasm_bindgen::JsValue,
};

const API_URL: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("ghfetch/", env!("CARGO_PKG_VERSION"));
const GRAPHQL_TIMEOUT: Duration = Duration::from_secs(20);
const CONTRIBUTORS_TIMEOUT: Duration = Duration::from_secs(5);

pub struct WorkerGitHub {
    token: String,
}

impl WorkerGitHub {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        extra_headers: &[(&str, &str)],
        body: Option<String>,
    ) -> Result<Request, GitHubError> {
        let headers = Headers::new();
        headers
            .set("Authorization", &format!("Bearer {}", self.token))
            .map_err(transport)?;
        headers.set("User-Agent", USER_AGENT).map_err(transport)?;
        for (name, value) in extra_headers {
            headers.set(name, value).map_err(transport)?;
        }

        let mut init = RequestInit::new();
        init.with_method(method).with_headers(headers);
        if let Some(body) = body {
            init.with_body(Some(JsValue::from_str(&body)));
        }

        Request::new_with_init(&format!("{API_URL}{path}"), &init).map_err(transport)
    }
}

impl GitHub for WorkerGitHub {
    async fn user(&self, login: &Username) -> Result<Option<github::graphql::User>, GitHubError> {
        let request = self.request(
            Method::Post,
            "/graphql",
            &[("Content-Type", "application/json")],
            Some(github::user_request_body(login)),
        )?;
        let (status, body) = send(request, GRAPHQL_TIMEOUT).await?;
        github::parse_user(status, &body)
    }

    async fn contributors(&self, owner: &str, repo: &str) -> Result<Vec<Contributor>, GitHubError> {
        let result = async {
            let request = self.request(
                Method::Get,
                &github::contributors_path(owner, repo),
                &[("Accept", "application/vnd.github+json")],
                None,
            )?;
            let (status, body) = send(request, CONTRIBUTORS_TIMEOUT).await?;
            github::parse_contributors(status, &body)
        }
        .await;

        if let Err(error) = &result {
            console_error!("contributors lookup failed for {owner}/{repo}: {error}");
        }
        result
    }
}

async fn send(request: Request, timeout: Duration) -> Result<(u16, Vec<u8>), GitHubError> {
    let controller = AbortController::default();
    let signal = controller.signal();

    let exchange = async {
        let mut response = Fetch::Request(request).send_with_signal(&signal).await?;
        let status = response.status_code();
        let body = response.bytes().await?;
        Ok::<_, worker::Error>((status, body))
    };

    match select(pin!(exchange), pin!(Delay::from(timeout))).await {
        Either::Left((result, _)) => result.map_err(transport),
        Either::Right(((), _)) => {
            controller.abort();
            Err(GitHubError::Transport(format!(
                "timed out after {}s",
                timeout.as_secs()
            )))
        }
    }
}

fn transport(error: impl std::fmt::Display) -> GitHubError {
    GitHubError::Transport(error.to_string())
}

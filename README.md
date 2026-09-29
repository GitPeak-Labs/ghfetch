# ghfetch

![CI](https://github.com/GitPeak-Labs/ghfetch/actions/workflows/ci-cd.yml/badge.svg?branch=master)

A GitHub stats API. Returns aggregated repository and contribution data for any GitHub user via a single HTTP request.

## Features

- Aggregates owned, collaborated, and contributed repositories
- Surfaces top collaborators (shared repos + commit counts) via the GitHub REST contributors API
- Per-IP and global rate limiting
- Response caching with a 15-minute TTL to protect GitHub token limits
- Cached responses are served in milliseconds

## Live Instance

```
https://ghfetch.amanekai.workers.dev
```

```bash
curl "https://ghfetch.amanekai.workers.dev/v1/stats?username=torvalds"
```

## Endpoints

| Endpoint                        | Description             |
| ------------------------------- | ----------------------- |
| `GET /v1/stats?username=<user>` | GitHub stats for a user |
| `GET /health`                   | Health check            |
| `GET /`                         | API info                |

## Response Shape

```json
{
  "ok": true,
  "data": {
    "displayName": "string",
    "avatarUrl": "string",
    "bio": "string",
    "accountCreatedAt": "ISO 8601",
    "totalRepos": 0,
    "totalStars": 0,
    "totalContributions": 0,
    "totalCommits": 0,
    "totalPrs": 0,
    "totalIssues": 0,
    "followers": 0,
    "following": 0,
    "mostStarredRepo": {
      "name": "string",
      "stars": 0,
      "url": "string"
    },
    "languages": [
      {
        "name": "string",
        "percentage": 0
      }
    ],
    "involvedRepos": [
      {
        "name": "string",
        "owner": "string",
        "url": "string",
        "lastContributedAt": "ISO 8601",
        "stars": 0,
        "primaryLanguage": "string | null",
        "isOwned": true,
        "isPrivate": false,
        "isFork": false
      }
    ],
    "collaborators": [
      {
        "login": "string",
        "avatarUrl": "string",
        "sharedRepos": 0,
        "commits": 0,
        "repos": [
          {
            "name": "string",
            "owner": "string",
            "url": "string",
            "commits": 0,
            "lastActivityAt": "ISO 8601"
          }
        ]
      }
    ]
  }
}
```

On error:

```json
{
  "ok": false,
  "error": "string"
}
```

| Status | Meaning                                                     |
| ------ | ----------------------------------------------------------- |
| `400`  | Invalid or missing username                                 |
| `404`  | Unknown route, or GitHub has no such user                   |
| `429`  | Rate limited (see below)                                    |
| `500`  | The server is missing its `GITHUB_TOKEN`                    |
| `502`  | GitHub failed, rejected the token, or sent an unusable body |
| `503`  | GitHub's own API rate limit was hit                         |

## How Stats Are Calculated

**Time window.** `totalContributions`, `totalCommits`, `totalPrs`, and `totalIssues` come from
GitHub's `contributionsCollection`, which without an explicit date range covers the trailing 12
months, not all-time totals.

**Which repos count.** Up to 50 most-recently-pushed repos you own, collaborate on, or belong to
via an org (public and private), plus up to 50 repos you've committed to in the last 12 months
even if you don't own them. These are deduplicated by owner/name into one repo set.

**`totalRepos`** is the size of that deduplicated repo set. **`totalStars`** is the sum of
`stargazerCount` across every repo in that set, including repos you don't own if you're a
collaborator or contributor. **`mostStarredRepo`** only considers repos you actually own, so a
high-star repo you contribute to elsewhere won't show up here.

**Language percentages.** For each repo, GitHub reports its top 8 languages by byte size. A
language's share within one repo is:

$$
share_{i,r} = \frac{\text{bytes}(i, r)}{\sum_{k} \text{bytes}(k, r)}
$$

A language's overall percentage is the average of that share across every repo that has language
data. Every repo counts equally regardless of its size, so a small repo written entirely in Rust
pulls the Rust percentage up just as much as a huge one would:

$$
percentage_i = \text{round}\left(100 \times \frac{\sum_{r=1}^{N} share_{i,r}}{N}\right)
$$

where $N$ is the number of repos with language data, and $share_{i,r} = 0$ for any repo whose top
8 languages don't include language $i$.

Languages that round down to 0% are dropped from the response.

**Involved repos.** The 15 repos you most recently pushed to or committed to, newest first.

**Collaborators.** Pulled from GitHub's REST contributors endpoint on up to 10 of your most
recently active, non-fork repos. Bot accounts and you yourself are excluded, and the list is
ranked by shared-repo count, then total commits. Each contributors request has a 5 second
timeout; a repo that fails or times out simply contributes no collaborators.

## Rate Limiting

| Limit                 | Value                   |
| --------------------- | ----------------------- |
| Per IP global         | 100 req/min             |
| Per IP per username   | 10 req/min              |
| Global username limit | 60 unique usernames/min |
| Block duration        | 5 min                   |

Requests answered from the cache don't count against these limits. Exceeding a limit returns
`429 Too Many Requests` with `X-RateLimit-Remaining` and `X-RateLimit-Reset` headers.

Counters are kept in an eventually consistent key-value store with no atomic increment, so the
limits are approximate under concurrent bursts.

## Deploy Your Own

### Prerequisites

- [Rust](https://rustup.rs) with the WASM target: `rustup target add wasm32-unknown-unknown`
- [worker-build](https://crates.io/crates/worker-build): `cargo install worker-build`
- [wrangler](https://developers.cloudflare.com/workers/wrangler/install-and-update/)
- A Cloudflare account
- A GitHub personal access token with `read:user` and `repo` scopes

### Setup

1. Clone the repo

```bash
git clone https://github.com/GitPeak-Labs/ghfetch
cd ghfetch
```

2. Create a KV namespace

```bash
wrangler kv namespace create RATE_LIMIT_KV
```

Copy the `id` from the output and update `wrangler.toml`:

```toml
[[kv_namespaces]]
binding = "RATE_LIMIT_KV"
id = "your-kv-namespace-id"
```

3. Set your GitHub token

```bash
wrangler secret put GITHUB_TOKEN
```

4. Deploy (Wrangler runs `worker-build` for you)

```bash
wrangler deploy
```

5. Test

```bash
curl "https://<your-worker>.workers.dev/v1/stats?username=<github-username>"
```

### Optional settings

| Variable           | Default            | Description                                                                      |
| ------------------ | ------------------ | -------------------------------------------------------------------------------- |
| `PORTFOLIO_ORIGIN` | `carlosranara.com` | Requests whose `Origin` or `Referer` contains this also see private repositories |

Set it under `[vars]` in `wrangler.toml`.

### Continuous deployment

Pushes to `master` deploy automatically through GitHub Actions. Add a `CLOUDFLARE_API_TOKEN`
repository secret with permission to edit Workers.

## Development

```bash
# Run the tests (logic runs natively; no WASM toolchain needed)
cargo test

# Format and lint (CI enforces both)
cargo fmt --all
cargo clippy -p ghfetch-core --all-targets -- -D warnings
cargo clippy -p ghfetch-worker --target wasm32-unknown-unknown -- -D warnings

# Run the Worker locally in workerd with a local KV
echo 'GITHUB_TOKEN=ghp_...' > .dev.vars
wrangler dev
```

## Layout

```
crates/
├── core/     all logic; no runtime dependencies, tested natively
│   └── src/  service (request flow), stats, github (trait + parsing),
│             rate_limit, store (KV trait), username, error
└── worker/   Cloudflare entrypoint: routing, fetch, KV, CORS/security headers
```

`ghfetch-core` talks to GitHub and storage through two small traits, `GitHub` and `Store`. The
Worker implements them with `fetch` and Workers KV; the tests implement them with in-memory fakes.

## Stack

- Rust, compiled to WASM
- [workers-rs](https://github.com/cloudflare/workers-rs)
- Cloudflare Workers and KV
- [serde](https://serde.rs), [chrono](https://github.com/chronotope/chrono), [indexmap](https://github.com/indexmap-rs/indexmap), [thiserror](https://github.com/dtolnay/thiserror)
- GitHub GraphQL API v4

## License

MIT

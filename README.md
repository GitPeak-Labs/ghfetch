# ghfetch

![CI](https://github.com/AmaneKai/ghfetch/actions/workflows/ci-cd.yml/badge.svg?branch=master)

A GitHub stats API built with TypeScript on Cloudflare Workers. Returns aggregated repository and contribution data for any GitHub user via a single HTTP request.

## Features

- Aggregates owned, collaborated, and contributed repositories
- Surfaces top collaborators (shared repos + commit counts) via the GitHub REST contributors API
- Per-IP and global rate limiting backed by Cloudflare KV
- Response caching with 5-minute TTL to protect GitHub token limits
- Runs at the edge with sub-10ms cached response times

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
        "percentage": 0,
        "color": "string"
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

**Collaborators.** Pulled from GitHub's REST contributors endpoint on up to 10 of your most
recently active, non-fork repos. Bot accounts and you yourself are excluded, and the list is
ranked by shared-repo count, then total commits.

## Rate Limiting

| Limit                 | Value                   |
| --------------------- | ----------------------- |
| Per IP global         | 100 req/min             |
| Per IP per username   | 10 req/min              |
| Global username limit | 60 unique usernames/min |
| Block duration        | 5 min                   |

Exceeding limits returns `429 Too Many Requests` with `X-RateLimit-Remaining` and `X-RateLimit-Reset` headers.

## Deploy Your Own

### Prerequisites

- [Bun](https://bun.sh)
- [wrangler](https://developers.cloudflare.com/workers/wrangler/install-and-update/)
- A Cloudflare account
- A GitHub personal access token with `read:user` and `repo` scopes

### Setup

1. Clone the repo

```bash
git clone https://github.com/AmaneKai/ghfetch
cd ghfetch
```

2. Install dependencies

```bash
bun install
```

3. Create a KV namespace

```bash
wrangler kv namespace create RATE_LIMIT_KV
```

Copy the `id` from the output and update `wrangler.toml`:

```toml
[[kv_namespaces]]
binding = "RATE_LIMIT_KV"
id = "your-kv-namespace-id"
```

4. Set your GitHub token

```bash
wrangler secret put GITHUB_TOKEN
```

5. Deploy

```bash
wrangler deploy
```

6. Test

```bash
curl "https://<your-worker>.workers.dev/v1/stats?username=<github-username>"
```

## Development

```bash
# Run tests
bun test

# Typecheck
bun run typecheck

# Lint
bun run lint

# Local dev server (uses remote KV bindings)
bun run dev
```

## Stack

- TypeScript
- [Hono](https://hono.dev)
- Cloudflare Workers
- Cloudflare KV
- GitHub GraphQL API v4

## License

MIT

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [2.0.0] - 2026-07-25

### Changed
- Rewrote the API from Rust (`worker-rs`) to TypeScript on [Hono](https://hono.dev), same behavior, endpoints, and response shape. Bun replaces Cargo for package management and is now the test runner; Wrangler continues to handle bundling and deploy.
- CI now runs `bun install` / `bun run typecheck` / `bun run lint` / `bun test` instead of `cargo check` / `cargo clippy` / `cargo test`.
- CORS and security headers now go through Hono's built-in `cors`/`secureHeaders` middleware instead of hand-built headers. `OPTIONS` preflight now returns `204` (was `200`), and responses pick up a few additional hardening headers Hono sets by default (`Strict-Transport-Security`, `Cross-Origin-Opener-Policy`, `Cross-Origin-Resource-Policy`, `Origin-Agent-Cluster`, `X-DNS-Prefetch-Control`, `X-Download-Options`, `X-Permitted-Cross-Domain-Policies`).
- Username validation moved to a Zod schema, validated at the route boundary via `@hono/zod-validator`, matching how the GraphQL/REST responses are already validated.
- Privacy classification (portfolio vs. public origin) is now its own middleware instead of living inline in the route handler.
- The edge-cache tier now uses Hono's built-in `hono/cache` middleware (with a custom partitioned cache key) instead of hand-written `caches.default` calls; the KV-backed stats cache is unchanged.
- The rate limiter's internals were simplified: `Limiter` methods now return a `commit()` closure instead of separate "pending write" data structures for the caller to dispatch. No change to rate-limit thresholds, KV key formats, or TTLs.
- Added a route-level integration test suite (`test/stats.test.ts`) exercising the full request lifecycle — validation, rate-limit tiers, GitHub error mapping, both cache tiers, CORS/security headers — via Hono's test client with in-memory fakes for KV, the Cache API, and `fetch`. Previously only the pure domain logic had test coverage.

## [1.0.0] - 2026-04-09

### Added
- Initial release
- `GET /v1/stats?username=<github-username>` — aggregated GitHub stats endpoint
- `GET /health` — health check endpoint
- `GET /` — API info and endpoint documentation
- KV response caching with 5-minute TTL
- Per-IP rate limiting (10 req/min, 5-minute block)
- Global IP rate limiting (100 req/min)
- Global username rate limiting (60 unique usernames/min) to prevent enumeration attacks
- Rate limit response headers (`X-RateLimit-Remaining`, `X-RateLimit-Reset`, `X-Cache`)
- Security headers (`X-Content-Type-Options`, `X-Frame-Options`, `X-XSS-Protection`, `Referrer-Policy`)
- CORS support
- Username validation (GitHub username format enforcement)
- Aggregates owned, collaborated, and contributed repositories via GitHub GraphQL API v4
- 20 unit tests covering validation and repository processing logic

import { gqlResponseSchema, type GqlResponse } from "./graphqlResponse"
import type { Username } from "./username"

const API_URL = "https://api.github.com/graphql"

const QUERY = `
query($u: String!) {
  user(login: $u) {
    avatarUrl name bio createdAt
    followers { totalCount }
    following { totalCount }
    contributionsCollection {
      contributionCalendar { totalContributions }
      totalCommitContributions
      totalPullRequestContributions
      totalIssueContributions
      commitContributionsByRepository(maxRepositories: 100) {
        repository {
          name stargazerCount url pushedAt isPrivate isFork
          owner { login }
          languages(first: 10, orderBy: {field: SIZE, direction: DESC}) {
            edges { size node { name } }
          }
        }
        contributions(first: 1) {
          nodes {
            occurredAt
          }
        }
      }
    }
    repositories(
        first: 100,
        ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER],
        orderBy: {field: PUSHED_AT, direction: DESC},
        privacy: PRIVATE) {
      nodes {
        name stargazerCount url pushedAt isPrivate isFork
        owner { login }
        languages(first: 10, orderBy: {field: SIZE, direction: DESC}) {
          edges { size node { name } }
        }
      }
    }
    publicRepositories: repositories(
        first: 100,
        orderBy: {field: PUSHED_AT, direction: DESC},
        ownerAffiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER],
        privacy: PUBLIC) {
      nodes {
        name stargazerCount url pushedAt isPrivate isFork
        owner { login }
        languages(first: 10, orderBy: {field: SIZE, direction: DESC}) {
          edges { size node { name } }
        }
      }
    }
  }
}`

export type GitHubFetchError =
  | { kind: "auth" }
  | { kind: "rateLimit" }
  | { kind: "http"; status: number }

export type GitHubFetchOutcome =
  | { ok: true; response: GqlResponse }
  | { ok: false; error: GitHubFetchError }

export class GitHubClient {
  constructor(private readonly token: string) {}

  async fetch(user: Username): Promise<GitHubFetchOutcome> {
    const response = await fetch(API_URL, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${this.token}`,
        "Content-Type": "application/json",
        "User-Agent": "portfolio-worker/1.0",
      },
      body: JSON.stringify({ query: QUERY, variables: { u: user } }),
    })

    if (response.status === 401)
      return { ok: false, error: { kind: "auth" } }

    if (response.status === 403)
      return { ok: false, error: { kind: "rateLimit" } }

    if (response.status !== 200)
      return { ok: false, error: { kind: "http", status: response.status } }

    const json = await response.json()
    const parsed = gqlResponseSchema.safeParse(json)

    if (!parsed.success) {
      console.error("GitHub GraphQL response failed validation", parsed.error)
      return { ok: false, error: { kind: "http", status: response.status } }
    }

    return { ok: true, response: parsed.data }
  }
}

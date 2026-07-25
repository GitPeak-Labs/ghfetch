type FetchHandler = (url: string, init: RequestInit | undefined) => Response | Promise<Response>

export function installFetchMock(handler: FetchHandler): void {
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString()
    return handler(url, init)
  }) as typeof fetch
}

export function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  })
}

export const graphqlSuccessBody = {
  data: {
    user: {
      avatarUrl: "https://avatars.example/torvalds",
      name: "Linus Torvalds",
      bio: "Linux kernel",
      createdAt: "2011-09-03T15:26:22Z",
      followers: { totalCount: 100 },
      following: { totalCount: 1 },
      contributionsCollection: {
        contributionCalendar: { totalContributions: 500 },
        totalCommitContributions: 400,
        totalPullRequestContributions: 50,
        totalIssueContributions: 50,
        commitContributionsByRepository: [],
      },
      repositories: { nodes: [] },
      publicRepositories: {
        nodes: [
          {
            name: "linux",
            owner: { login: "torvalds" },
            stargazerCount: 100,
            url: "https://github.com/torvalds/linux",
            languages: { edges: [{ size: 1000, node: { name: "C" } }] },
            pushedAt: "2026-01-01T00:00:00Z",
            isPrivate: false,
            isFork: false,
          },
        ],
      },
    },
  },
}

export function installGitHubFetchMock(options?: {
  graphqlStatus?: number
  graphqlBody?: unknown
  contributors?: unknown[]
}): void {
  const graphqlStatus = options?.graphqlStatus ?? 200
  const graphqlBody = options?.graphqlBody ?? graphqlSuccessBody
  const contributors = options?.contributors ?? []

  installFetchMock((url) => {
    if (url.includes("api.github.com/graphql")) {
      if (graphqlStatus !== 200) return new Response(null, { status: graphqlStatus })
      return jsonResponse(graphqlBody)
    }

    if (url.includes("/contributors")) return jsonResponse(contributors)

    return new Response("not found", { status: 404 })
  })
}

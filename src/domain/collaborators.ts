import { z } from "zod"
import type { CollabRepo, Collaborator, InvolvedRepo } from "../shared/githubStats"

const MAX_COLLABORATORS = 10
const CONTRIBUTORS_TIMEOUT_MS = 5000

const contributorEntrySchema = z.object({
  login: z.string(),
  avatar_url: z.string().default(""),
  contributions: z.number().default(0),
  type: z.string().default(""),
})

type ContributorEntry = z.infer<typeof contributorEntrySchema>

async function fetchRepoContributors(
  token: string,
  owner: string,
  name: string
): Promise<ContributorEntry[]> {
  const url = `https://api.github.com/repos/${owner}/${name}/contributors?per_page=100&anon=false`

  let response: Response
  try {
    response = await fetch(url, {
      method: "GET",
      headers: {
        Authorization: `Bearer ${token}`,
        "User-Agent": "ghfetch-worker/1.0",
        Accept: "application/vnd.github+json",
      },
      signal: AbortSignal.timeout(CONTRIBUTORS_TIMEOUT_MS),
    })
  } catch (error) {
    console.error(`Failed to fetch contributors for ${owner}/${name}`, error)
    return []
  }

  if (response.status !== 200) return []

  const json = await response.json()
  const parsed = z.array(contributorEntrySchema).safeParse(json)

  if (!parsed.success) {
    console.error(`Unexpected contributors payload for ${owner}/${name}`, parsed.error)
    return []
  }

  return parsed.data
}

interface RepoRef {
  name: string
  owner: string
  url: string
  lastActivityAt: string
}

interface CollaboratorAccumulator {
  login: string
  avatarUrl: string
  commits: number
  repos: CollabRepo[]
}

export function aggregate(
  targetUser: string,
  repos: RepoRef[],
  perRepo: ContributorEntry[][]
): Collaborator[] {
  const aggregated = new Map<string, CollaboratorAccumulator>()

  repos.forEach((repo, index) => {
    const entries = perRepo[index] ?? []
    const seenThisRepo = new Set<string>()

    for (const entry of entries) {
      if (entry.type === "Bot" || entry.login.endsWith("[bot]")) continue
      if (entry.login.toLowerCase() === targetUser.toLowerCase()) continue

      const key = entry.login.toLowerCase()
      if (seenThisRepo.has(key)) continue
      seenThisRepo.add(key)

      const existing = aggregated.get(key) ?? {
        login: entry.login,
        avatarUrl: entry.avatar_url,
        commits: 0,
        repos: [],
      }
      existing.commits += entry.contributions
      existing.repos.push({
        name: repo.name,
        owner: repo.owner,
        url: repo.url,
        commits: entry.contributions,
        lastActivityAt: repo.lastActivityAt,
      })
      aggregated.set(key, existing)
    }
  })

  const list: Collaborator[] = Array.from(aggregated.values()).map((entry) => ({
    login: entry.login,
    avatarUrl: entry.avatarUrl,
    sharedRepos: entry.repos.length,
    commits: entry.commits,
    repos: entry.repos,
  }))

  list.sort((a, b) => b.sharedRepos - a.sharedRepos || b.commits - a.commits)

  return list.slice(0, MAX_COLLABORATORS)
}

export function isEligible(repo: InvolvedRepo, allowPrivate: boolean): boolean {
  return (!repo.isPrivate || allowPrivate) && !repo.isFork
}

export async function fetchCollaborators(
  token: string,
  targetUser: string,
  repos: InvolvedRepo[],
  allowPrivate: boolean
): Promise<Collaborator[]> {
  const eligible = repos.filter((repo) => isEligible(repo, allowPrivate))

  const perRepo = await Promise.all(
    eligible.map((repo) => fetchRepoContributors(token, repo.owner, repo.name))
  )

  const repoRefs: RepoRef[] = eligible.map((repo) => ({
    name: repo.name,
    owner: repo.owner,
    url: repo.url,
    lastActivityAt: repo.lastContributedAt,
  }))

  return aggregate(targetUser, repoRefs, perRepo)
}

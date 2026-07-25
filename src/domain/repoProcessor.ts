import type { GqlUser, Repo } from "./graphqlResponse"
import type {
  GitHubLanguage,
  GitHubStats,
  InvolvedRepo,
  MostStarredRepo,
} from "../shared/githubStats"

export interface ProcessedRepos {
  repoCount: number
  totalStars: number
  languages: Array<[string, number]>
  mostStarredRepo: MostStarredRepo | null
  involvedRepos: InvolvedRepo[]
}

export function processRepos(
  targetUser: string,
  privateRepos: Repo[],
  publicRepos: Repo[],
  contributed: Array<[Repo, string | null]>
): ProcessedRepos {
  const seen = new Set<string>()
  const langShares = new Map<string, number>()
  let totalStars = 0
  let top: MostStarredRepo | null = null
  let reposWithLangs = 0
  const involvedMap = new Map<string, InvolvedRepo>()

  const addInvolved = (repo: Repo, date: string): void => {
    const isOwned = repo.owner.login.toLowerCase() === targetUser.toLowerCase()
    const key = `${repo.owner.login.toLowerCase()}/${repo.name.toLowerCase()}`
    const primaryLanguage = repo.languages.edges[0]?.node.name ?? null

    const newItem: InvolvedRepo = {
      name: repo.name,
      owner: repo.owner.login,
      url: repo.url,
      lastContributedAt: date,
      stars: repo.stargazerCount,
      primaryLanguage,
      isOwned,
      isPrivate: repo.isPrivate,
      isFork: repo.isFork,
    }

    const existing = involvedMap.get(key)
    if (existing) {
      if (newItem.lastContributedAt > existing.lastContributedAt)
        existing.lastContributedAt = newItem.lastContributedAt
      return
    }

    involvedMap.set(key, newItem)
  }

  const accumulateStats = (repo: Repo): void => {
    totalStars += repo.stargazerCount

    const bestStars = top?.stars ?? 0
    const isTargetOwner = repo.owner.login.toLowerCase() === targetUser.toLowerCase()
    if (isTargetOwner && repo.stargazerCount > bestStars)
      top = { name: repo.name, stars: repo.stargazerCount, url: repo.url }

    const totalRepoBytes = repo.languages.edges.reduce((sum, edge) => sum + edge.size, 0)
    if (totalRepoBytes === 0) return

    reposWithLangs += 1
    for (const edge of repo.languages.edges) {
      const share = edge.size / totalRepoBytes
      langShares.set(edge.node.name, (langShares.get(edge.node.name) ?? 0) + share)
    }
  }

  for (const repo of privateRepos) {
    const keyName = repo.name.toLowerCase()
    if (seen.has(keyName)) continue
    seen.add(keyName)

    if (repo.pushedAt) addInvolved(repo, repo.pushedAt)
    accumulateStats(repo)
  }

  for (const repo of publicRepos) {
    const keyName = repo.name.toLowerCase()
    if (seen.has(keyName)) continue
    seen.add(keyName)

    if (repo.pushedAt) addInvolved(repo, repo.pushedAt)
    accumulateStats(repo)
  }

  for (const [repo, occurredAt] of contributed) {
    const keyName = repo.name.toLowerCase()
    if (!seen.has(keyName)) {
      seen.add(keyName)
      accumulateStats(repo)
    }

    const date = occurredAt ?? repo.pushedAt ?? ""
    if (date) addInvolved(repo, date)
  }

  const languages: Array<[string, number]> =
    reposWithLangs > 0
      ? Array.from(langShares.entries()).map(([name, totalShare]) => [
          name,
          totalShare / reposWithLangs,
        ])
      : []
  languages.sort((a, b) => b[1] - a[1])

  const involvedRepos = Array.from(involvedMap.values())
  involvedRepos.sort((a, b) => b.lastContributedAt.localeCompare(a.lastContributedAt))

  return {
    repoCount: seen.size,
    totalStars,
    languages,
    mostStarredRepo: top,
    involvedRepos: involvedRepos.slice(0, 15),
  }
}

export function buildStats(
  user: GqlUser,
  username: string,
  repoCount: number,
  totalStars: number,
  languageShares: Array<[string, number]>,
  mostStarredRepo: MostStarredRepo | null,
  involvedRepos: InvolvedRepo[]
): GitHubStats {
  const languages: GitHubLanguage[] = languageShares
    .map(([name, averageShare]) => ({ name, percentage: Math.round(averageShare * 100) }))
    .filter((language) => language.percentage > 0)

  return {
    totalRepos: repoCount,
    totalContributions: user.contributionsCollection.contributionCalendar.totalContributions,
    totalStars,
    followers: user.followers.totalCount,
    following: user.following.totalCount,
    totalCommits: user.contributionsCollection.totalCommitContributions,
    totalPrs: user.contributionsCollection.totalPullRequestContributions,
    totalIssues: user.contributionsCollection.totalIssueContributions,
    accountCreatedAt: user.createdAt,
    mostStarredRepo,
    avatarUrl: user.avatarUrl,
    displayName: user.name ?? username,
    bio: user.bio ?? "",
    languages,
    involvedRepos,
    collaborators: [],
  }
}

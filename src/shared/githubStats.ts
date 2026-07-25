export interface GitHubLanguage {
  name: string
  percentage: number
}

export interface MostStarredRepo {
  name: string
  stars: number
  url: string
}

export interface InvolvedRepo {
  name: string
  owner: string
  url: string
  lastContributedAt: string
  stars: number
  primaryLanguage: string | null
  isOwned: boolean
  isPrivate: boolean
  isFork: boolean
}

export interface CollabRepo {
  name: string
  owner: string
  url: string
  commits: number
  lastActivityAt: string
}

export interface Collaborator {
  login: string
  avatarUrl: string
  sharedRepos: number
  commits: number
  repos: CollabRepo[]
}

export interface GitHubStats {
  totalRepos: number
  totalContributions: number
  languages: GitHubLanguage[]
  totalStars: number
  followers: number
  following: number
  totalCommits: number
  totalPrs: number
  totalIssues: number
  accountCreatedAt: string
  mostStarredRepo: MostStarredRepo | null
  avatarUrl: string
  displayName: string
  bio: string
  involvedRepos: InvolvedRepo[]
  collaborators: Collaborator[]
}

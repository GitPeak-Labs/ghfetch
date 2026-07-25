import { z } from "zod"

const ownerSchema = z.object({
  login: z.string(),
})

const langNodeSchema = z.object({
  name: z.string(),
})

const langEdgeSchema = z.object({
  size: z.number(),
  node: langNodeSchema,
})

const langConnSchema = z.object({
  edges: z.array(langEdgeSchema),
})

export const repoSchema = z.object({
  name: z.string(),
  owner: ownerSchema,
  stargazerCount: z.number(),
  url: z.string(),
  languages: langConnSchema,
  pushedAt: z.string().nullable(),
  isPrivate: z.boolean(),
  isFork: z.boolean().default(false),
})

const contribNodeSchema = z.object({
  occurredAt: z.string(),
})

const contribConnSchema = z.object({
  nodes: z.array(contribNodeSchema),
})

const commitContribByRepoSchema = z.object({
  repository: repoSchema,
  contributions: contribConnSchema.nullable().optional(),
})

const calendarSchema = z.object({
  totalContributions: z.number(),
})

const contribCollSchema = z.object({
  contributionCalendar: calendarSchema,
  totalCommitContributions: z.number(),
  totalPullRequestContributions: z.number(),
  totalIssueContributions: z.number(),
  commitContributionsByRepository: z.array(commitContribByRepoSchema),
})

const countConnSchema = z.object({
  totalCount: z.number(),
})

const repoConnSchema = z.object({
  nodes: z.array(repoSchema),
})

export const gqlUserSchema = z.object({
  avatarUrl: z.string(),
  name: z.string().nullable(),
  bio: z.string().nullable(),
  createdAt: z.string(),
  followers: countConnSchema,
  following: countConnSchema,
  contributionsCollection: contribCollSchema,
  repositories: repoConnSchema,
  publicRepositories: repoConnSchema,
})

export const gqlResponseSchema = z.object({
  data: z
    .object({
      user: gqlUserSchema,
    })
    .nullable()
    .optional(),
})

export type Repo = z.infer<typeof repoSchema>
export type GqlUser = z.infer<typeof gqlUserSchema>
export type CommitContribByRepo = z.infer<typeof commitContribByRepoSchema>
export type GqlResponse = z.infer<typeof gqlResponseSchema>

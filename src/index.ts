import { Hono } from "hono"
import type { Context, Next } from "hono"
import { cors } from "hono/cors"
import { secureHeaders } from "hono/secure-headers"
import { cache } from "hono/cache"
import { zValidator } from "@hono/zod-validator"
import { z } from "zod"
import { fetchCollaborators } from "./domain/collaborators"
import { GitHubClient } from "./domain/githubClient"
import type { Repo } from "./domain/graphqlResponse"
import { defaultRateConfig, Limiter } from "./domain/rateLimit"
import { buildStats, processRepos } from "./domain/repoProcessor"
import { err, health, root, success } from "./domain/response"
import { usernameSchema } from "./domain/username"
import type { Username } from "./domain/username"
import type { GitHubStats } from "./shared/githubStats"

interface Bindings {
  RATE_LIMIT_KV: KVNamespace
  GITHUB_TOKEN: string
  PORTFOLIO_ORIGIN?: string
}

interface Variables {
  clientId: string
  isPortfolioRequest: boolean
}

type AppEnv = { Bindings: Bindings; Variables: Variables }

const CACHE_TTL_SECONDS = 300
const VERSION = "1.0.0"

const app = new Hono<AppEnv>()

app.use(
  "*",
  cors({
    origin: "*",
    allowMethods: ["GET", "OPTIONS"],
    allowHeaders: ["Content-Type"],
    maxAge: 86400,
  })
)

app.use(
  "*",
  secureHeaders({
    xFrameOptions: "DENY",
    referrerPolicy: "strict-origin-when-cross-origin",
    xXssProtection: "1; mode=block",
  })
)

app.get("/", () => root(VERSION))
app.get("/health", () => health(VERSION))

async function classifyRequest(c: Context<AppEnv>, next: Next): Promise<void> {
  const originHeader = c.req.header("Origin") ?? ""
  const refererHeader = c.req.header("Referer") ?? ""
  const portfolioDomain = c.env.PORTFOLIO_ORIGIN ?? "carlosranara.com"

  const isPortfolioRequest =
    originHeader.includes(portfolioDomain) ||
    refererHeader.includes(portfolioDomain) ||
    originHeader.includes("localhost:5173") ||
    refererHeader.includes("localhost:5173")

  c.set("isPortfolioRequest", isPortfolioRequest)
  c.set("clientId", c.req.header("CF-Connecting-IP") ?? "unknown")

  await next()
}

const statsQuerySchema = z.object({
  username: usernameSchema,
})

app.get(
  "/v1/stats",
  zValidator("query", statsQuerySchema, (result, _c) => {
    if (!result.success) return err(400, "Invalid username format")
  }),
  classifyRequest,
  cache({
    cacheName: "ghfetch",
    vary: "Accept-Encoding",
    keyGenerator: (c) => {
      const isPortfolioRequest = (c as Context<AppEnv>).get("isPortfolioRequest")
      return `${c.req.url}&_private=${isPortfolioRequest ? "1" : "0"}`
    },
  }),
  (c) => handleStats(c, c.req.valid("query").username)
)

app.notFound(() => err(404, "Not found"))

function parseCachedStats(raw: string): GitHubStats | null {
  try {
    return JSON.parse(raw) as GitHubStats
  } catch (error) {
    console.error("Failed to parse cached stats", error)
    return null
  }
}

async function handleStats(c: Context<AppEnv>, user: Username): Promise<Response> {
  const isPortfolioRequest = c.get("isPortfolioRequest")
  const clientId = c.get("clientId")

  const kvCacheKey = `cache:${user}:${isPortfolioRequest ? "private" : "public"}`
  const kv = c.env.RATE_LIMIT_KV ?? null

  if (kv) {
    const cachedText = await kv.get(kvCacheKey, "text")
    if (cachedText) {
      const stats = parseCachedStats(cachedText)
      if (stats) {
        console.log(`KV cache hit for ${user} (private_authorized: ${isPortfolioRequest})`)
        return success(stats, 10, 60, true)
      }
    }
  }

  const token = c.env.GITHUB_TOKEN ?? ""
  if (!token) return err(500, "Server configuration error")

  let remaining: number
  let reset: number

  if (kv) {
    const limiter = new Limiter(kv)

    const globalResult = await limiter.checkGlobals(clientId, user)
    c.executionCtx.waitUntil(globalResult.commit())

    if (globalResult.globalIpBlocked)
      return err(429, "Too many requests. Slow down.", { remaining: 0, reset: 60 })
    if (globalResult.usernameBlocked)
      return err(429, "Too many requests for this user. Try again in a minute.", {
        remaining: 0,
        reset: 60,
      })

    const rateOutcome = await limiter.checkWithInfo(clientId, user, defaultRateConfig)

    if (rateOutcome.kind === "alreadyBlocked")
      return err(429, "Rate limit exceeded. Try again in 5 minutes.", { remaining: 0, reset: 300 })

    if (rateOutcome.kind === "justBlocked") {
      c.executionCtx.waitUntil(rateOutcome.commit())
      return err(429, "Rate limit exceeded. Try again in 5 minutes.", { remaining: 0, reset: 300 })
    }

    c.executionCtx.waitUntil(rateOutcome.commit())
    remaining = rateOutcome.remaining
    reset = rateOutcome.reset
  } else {
    remaining = 10
    reset = 60
  }

  const gh = new GitHubClient(token)
  const fetchOutcome = await gh.fetch(user)

  if (!fetchOutcome.ok) {
    console.log("GitHub error:", fetchOutcome.error)
    if (fetchOutcome.error.kind === "rateLimit")
      return err(503, "GitHub API rate limit exceeded")
    return err(502, "Failed to fetch data from GitHub")
  }

  const gqlUser = fetchOutcome.response.data?.user
  if (!gqlUser) return err(404, "User not found")

  const privateRepos: Repo[] = isPortfolioRequest ? gqlUser.repositories.nodes : []

  const contributed: Array<[Repo, string | null]> =
    gqlUser.contributionsCollection.commitContributionsByRepository
      .filter((entry) => isPortfolioRequest || !entry.repository.isPrivate)
      .map((entry) => {
        const occurredAt = entry.contributions?.nodes[0]?.occurredAt ?? null
        return [entry.repository, occurredAt]
      })

  const processed = processRepos(
    user,
    privateRepos,
    gqlUser.publicRepositories.nodes,
    contributed
  )

  const collaborators = await fetchCollaborators(
    token,
    user,
    processed.involvedRepos,
    isPortfolioRequest
  )

  const stats = buildStats(
    gqlUser,
    user,
    processed.repoCount,
    processed.totalStars,
    processed.languages,
    processed.mostStarredRepo,
    processed.involvedRepos
  )
  stats.collaborators = collaborators

  if (kv) {
    c.executionCtx.waitUntil(
      (async () => {
        try {
          await kv.put(kvCacheKey, JSON.stringify(stats), { expirationTtl: CACHE_TTL_SECONDS })
          console.log(`Cached stats for ${user}`)
        } catch (error) {
          console.error(`Failed to write KV cache for ${user}`, error)
        }
      })()
    )
  }

  return success(stats, remaining, reset, false)
}

export default app

const GLOBAL_USERNAME_MAX = 60
const GLOBAL_IP_MAX = 100

export interface RateConfig {
  maxRequests: number
  windowSeconds: number
  blockSeconds: number
}

export const defaultRateConfig: RateConfig = {
  maxRequests: 10,
  windowSeconds: 60,
  blockSeconds: 300,
}

export interface GlobalCheckResult {
  globalIpBlocked: boolean
  usernameBlocked: boolean
  commit: () => Promise<void>
}

export type RateLimitOutcome =
  | { kind: "alreadyBlocked" }
  | { kind: "justBlocked"; retryAfter: number; commit: () => Promise<void> }
  | { kind: "allowed"; remaining: number; reset: number; commit: () => Promise<void> }

function parseCount(raw: string | null): number {
  if (raw === null) return 0
  const parsed = Number.parseInt(raw, 10)
  return Number.isNaN(parsed) ? 0 : parsed
}

async function put(kv: KVNamespace, key: string, value: string, ttl: number): Promise<void> {
  try {
    await kv.put(key, value, { expirationTtl: ttl })
  } catch (error) {
    console.error(`Failed to write rate limit key ${key}`, error)
  }
}

export class Limiter {
  constructor(private readonly kv: KVNamespace) {}

  async checkGlobals(clientId: string, username: string): Promise<GlobalCheckResult> {
    const now = Math.floor(Date.now() / 1000)
    const windowSeconds = 60
    const windowStart = Math.floor(now / windowSeconds) * windowSeconds
    const ttl = Math.max(windowSeconds - (now - windowStart), 60)

    const globalIpKey = `rl:gl:${clientId}:${windowStart}`
    const usernameKey = `rl:ugl:${username}:${windowStart}`

    const [globalIpRaw, usernameRaw] = await Promise.all([
      this.kv.get(globalIpKey, "text"),
      this.kv.get(usernameKey, "text"),
    ])

    const globalIpCount = parseCount(globalIpRaw)
    const globalIpBlocked = globalIpCount >= GLOBAL_IP_MAX
    if (globalIpBlocked) console.log(`Global IP limit: ${clientId}`)

    const usernameCount = parseCount(usernameRaw)
    const usernameBlocked = usernameCount >= GLOBAL_USERNAME_MAX
    if (usernameBlocked) console.log(`Global username limit: ${username}`)

    return {
      globalIpBlocked,
      usernameBlocked,
      commit: async () => {
        await Promise.all([
          put(this.kv, globalIpKey, String(globalIpCount + 1), ttl),
          put(this.kv, usernameKey, String(usernameCount + 1), ttl),
        ])
      },
    }
  }

  async checkWithInfo(
    clientId: string,
    username: string,
    config: RateConfig
  ): Promise<RateLimitOutcome> {
    const now = Math.floor(Date.now() / 1000)
    const windowStart = Math.floor(now / config.windowSeconds) * config.windowSeconds

    const key = `rl:${clientId}:${username}:${windowStart}`
    const blockKey = `block:${clientId}:${username}`

    const [blockRaw, countRaw] = await Promise.all([
      this.kv.get(blockKey, "text"),
      this.kv.get(key, "text"),
    ])

    if (blockRaw !== null) return { kind: "alreadyBlocked" }

    const count = parseCount(countRaw)

    if (count >= config.maxRequests) {
      console.log(`Blocked: ${clientId} for ${username}`)
      return {
        kind: "justBlocked",
        retryAfter: config.blockSeconds,
        commit: () => put(this.kv, blockKey, "blocked", config.blockSeconds),
      }
    }

    const newCount = count + 1
    const ttl = Math.max(config.windowSeconds - (now - windowStart), 60)

    if (newCount >= config.maxRequests * 0.8)
      console.log(`Warn: ${newCount}/${config.maxRequests} for ${clientId}`)

    return {
      kind: "allowed",
      remaining: config.maxRequests - newCount,
      reset: windowStart + config.windowSeconds,
      commit: () => put(this.kv, key, String(newCount), ttl),
    }
  }
}

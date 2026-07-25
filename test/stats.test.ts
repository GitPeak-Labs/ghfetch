import { beforeEach, describe, expect, test } from "bun:test"
import "./support/bootstrapCaches"
import app from "../src/index"
import { createFakeExecutionContext } from "./support/executionContext"
import { getFakeCacheKeys, installFakeCaches } from "./support/fakeCaches"
import { createFakeKv } from "./support/fakeKv"
import { installGitHubFetchMock } from "./support/fetchMock"

interface Env {
  RATE_LIMIT_KV: KVNamespace
  GITHUB_TOKEN: string
  PORTFOLIO_ORIGIN?: string
}

function makeEnv(overrides?: Partial<Env>): Env {
  return {
    RATE_LIMIT_KV: createFakeKv(),
    GITHUB_TOKEN: "test-github-token",
    PORTFOLIO_ORIGIN: "portfolio.example.com",
    ...overrides,
  }
}

async function flushMicrotasks(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0))
}

interface JsonEnvelope {
  ok: boolean
  error?: string
  name?: string
  data?: {
    displayName?: string
    totalRepos?: number
    totalStars?: number
  }
}

async function readJson(res: Response): Promise<JsonEnvelope> {
  return (await res.json()) as JsonEnvelope
}

beforeEach(() => {
  installFakeCaches()
})

describe("GET /", () => {
  test("returns API info", async () => {
    const res = await app.request("/", {}, makeEnv(), createFakeExecutionContext())
    expect(res.status).toBe(200)
    const body = await readJson(res)
    expect(body.name).toBe("ghfetch")
  })
})

describe("GET /health", () => {
  test("returns ok status and security headers", async () => {
    const res = await app.request("/health", {}, makeEnv(), createFakeExecutionContext())
    expect(res.status).toBe(200)
    expect(res.headers.get("X-Frame-Options")).toBe("DENY")
    expect(res.headers.get("X-Content-Type-Options")).toBe("nosniff")
    expect(res.headers.get("Referrer-Policy")).toBe("strict-origin-when-cross-origin")
    expect(res.headers.get("X-XSS-Protection")).toBe("1; mode=block")
  })
})

describe("unknown routes", () => {
  test("returns 404", async () => {
    const res = await app.request("/does-not-exist", {}, makeEnv(), createFakeExecutionContext())
    expect(res.status).toBe(404)
    expect(await readJson(res)).toEqual({ ok: false, error: "Not found" })
  })
})

describe("OPTIONS /v1/stats", () => {
  test("returns 204 with CORS headers", async () => {
    const res = await app.request(
      "/v1/stats",
      { method: "OPTIONS" },
      makeEnv(),
      createFakeExecutionContext()
    )
    expect(res.status).toBe(204)
    expect(res.headers.get("Access-Control-Allow-Origin")).toBe("*")
    expect(res.headers.get("Access-Control-Allow-Methods")).toContain("GET")
  })
})

describe("GET /v1/stats validation", () => {
  test("400 when username is missing", async () => {
    const res = await app.request("/v1/stats", {}, makeEnv(), createFakeExecutionContext())
    expect(res.status).toBe(400)
    expect(await readJson(res)).toEqual({ ok: false, error: "Invalid username format" })
  })

  test("400 when username is invalid", async () => {
    const res = await app.request(
      "/v1/stats?username=--bad--",
      {},
      makeEnv(),
      createFakeExecutionContext()
    )
    expect(res.status).toBe(400)
  })
})

describe("GET /v1/stats server configuration", () => {
  test("500 when GITHUB_TOKEN is missing", async () => {
    const res = await app.request(
      "/v1/stats?username=torvalds",
      {},
      makeEnv({ GITHUB_TOKEN: "" }),
      createFakeExecutionContext()
    )
    expect(res.status).toBe(500)
    expect(await readJson(res)).toEqual({ ok: false, error: "Server configuration error" })
  })
})

describe("GET /v1/stats rate limiting", () => {
  test("429 when the global IP limit is exceeded", async () => {
    const kv = createFakeKv()
    const clientId = "203.0.113.1"
    const windowStart = Math.floor(Date.now() / 1000 / 60) * 60
    await kv.put(`rl:gl:${clientId}:${windowStart}`, "100")

    const res = await app.request(
      "/v1/stats?username=someone",
      { headers: { "CF-Connecting-IP": clientId } },
      makeEnv({ RATE_LIMIT_KV: kv }),
      createFakeExecutionContext()
    )

    expect(res.status).toBe(429)
    expect((await readJson(res)).error).toBe("Too many requests. Slow down.")
  })

  test("429 when the global username limit is exceeded", async () => {
    const kv = createFakeKv()
    const username = "popularuser"
    const windowStart = Math.floor(Date.now() / 1000 / 60) * 60
    await kv.put(`rl:ugl:${username}:${windowStart}`, "60")

    const res = await app.request(
      `/v1/stats?username=${username}`,
      { headers: { "CF-Connecting-IP": "203.0.113.2" } },
      makeEnv({ RATE_LIMIT_KV: kv }),
      createFakeExecutionContext()
    )

    expect(res.status).toBe(429)
    expect((await readJson(res)).error).toBe(
      "Too many requests for this user. Try again in a minute."
    )
  })

  test("429 when the client is already blocked", async () => {
    const kv = createFakeKv()
    const clientId = "203.0.113.3"
    const username = "blockeduser"
    await kv.put(`block:${clientId}:${username}`, "blocked")

    const res = await app.request(
      `/v1/stats?username=${username}`,
      { headers: { "CF-Connecting-IP": clientId } },
      makeEnv({ RATE_LIMIT_KV: kv }),
      createFakeExecutionContext()
    )

    expect(res.status).toBe(429)
    expect((await readJson(res)).error).toBe("Rate limit exceeded. Try again in 5 minutes.")
  })
})

describe("GET /v1/stats GitHub error mapping", () => {
  test("503 when GitHub returns 403 (upstream rate limited)", async () => {
    installGitHubFetchMock({ graphqlStatus: 403 })
    const res = await app.request(
      "/v1/stats?username=newuser1",
      {},
      makeEnv(),
      createFakeExecutionContext()
    )
    expect(res.status).toBe(503)
  })

  test("502 when GitHub returns 401 (auth failed)", async () => {
    installGitHubFetchMock({ graphqlStatus: 401 })
    const res = await app.request(
      "/v1/stats?username=newuser2",
      {},
      makeEnv(),
      createFakeExecutionContext()
    )
    expect(res.status).toBe(502)
  })

  test("404 when GitHub has no user for that login", async () => {
    installGitHubFetchMock({ graphqlBody: { data: null } })
    const res = await app.request(
      "/v1/stats?username=nosuchuser",
      {},
      makeEnv(),
      createFakeExecutionContext()
    )
    expect(res.status).toBe(404)
  })
})

describe("GET /v1/stats happy path", () => {
  test("200 with expected envelope shape and headers", async () => {
    installGitHubFetchMock()
    const res = await app.request(
      "/v1/stats?username=torvalds",
      {},
      makeEnv(),
      createFakeExecutionContext()
    )

    expect(res.status).toBe(200)
    expect(res.headers.get("X-Cache")).toBe("MISS")
    expect(res.headers.get("X-RateLimit-Remaining")).toBe("9")
    expect(res.headers.get("X-RateLimit-Reset")).toBeTruthy()

    const body = await readJson(res)
    expect(body.ok).toBe(true)
    expect(body.data?.displayName).toBe("Linus Torvalds")
    expect(body.data?.totalRepos).toBe(1)
    expect(body.data?.totalStars).toBe(100)
  })

  test("second request hits the KV stats cache", async () => {
    installGitHubFetchMock()
    const env = makeEnv()

    const first = await app.request(
      "/v1/stats?username=torvalds",
      {},
      env,
      createFakeExecutionContext()
    )
    expect(first.status).toBe(200)
    expect(first.headers.get("X-Cache")).toBe("MISS")

    await flushMicrotasks()
    installFakeCaches() // reset the edge tier so this exercises the KV tier specifically

    const second = await app.request(
      "/v1/stats?username=torvalds",
      {},
      env,
      createFakeExecutionContext()
    )
    expect(second.status).toBe(200)
    expect(second.headers.get("X-Cache")).toBe("HIT")
  })

  test("second identical request does not hit a broken GitHub API", async () => {
    installGitHubFetchMock()
    const env = makeEnv()

    const first = await app.request(
      "/v1/stats?username=torvalds",
      {},
      env,
      createFakeExecutionContext()
    )
    expect(first.status).toBe(200)
    await flushMicrotasks()

    installGitHubFetchMock({ graphqlStatus: 500 })
    const second = await app.request(
      "/v1/stats?username=torvalds",
      {},
      env,
      createFakeExecutionContext()
    )
    expect(second.status).toBe(200)
  })

  test("edge cache key is partitioned by portfolio origin", async () => {
    installGitHubFetchMock()
    const env = makeEnv()

    await app.request("/v1/stats?username=torvalds", {}, env, createFakeExecutionContext())
    await app.request(
      "/v1/stats?username=torvalds",
      { headers: { Origin: "https://portfolio.example.com" } },
      env,
      createFakeExecutionContext()
    )
    await flushMicrotasks()

    const keys = getFakeCacheKeys("ghfetch")
    expect(keys.some((key) => key.includes("_private=0"))).toBe(true)
    expect(keys.some((key) => key.includes("_private=1"))).toBe(true)
  })
})

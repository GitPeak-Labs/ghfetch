// CORS and security headers are applied globally by Hono middleware (see index.ts),
// not per-response here.
function withCommonHeaders(headers: Headers): void {
  headers.set("Cache-Control", "public, max-age=900, stale-while-revalidate=120")
  headers.set("Vary", "Accept-Encoding")
  headers.set("Content-Type", "application/json")
}

function json(data: unknown, status: number, extraHeaders?: Record<string, string>): Response {
  const headers = new Headers()
  withCommonHeaders(headers)

  if (extraHeaders) {
    for (const [key, value] of Object.entries(extraHeaders)) headers.set(key, value)
  }

  return new Response(JSON.stringify(data), { status, headers })
}

export function success<T>(data: T, remaining: number, reset: number, cached: boolean): Response {
  return json({ ok: true, data }, 200, {
    "X-RateLimit-Remaining": String(remaining),
    "X-RateLimit-Reset": String(reset),
    "X-Cache": cached ? "HIT" : "MISS",
  })
}

export function err(
  status: number,
  message: string,
  rateInfo?: { remaining: number; reset: number }
): Response {
  const extraHeaders: Record<string, string> = {
    "Cache-Control": "no-store, no-cache, must-revalidate",
  }

  if (rateInfo) {
    extraHeaders["X-RateLimit-Remaining"] = String(rateInfo.remaining)
    extraHeaders["X-RateLimit-Reset"] = String(rateInfo.reset)
  }

  return json({ ok: false, error: message }, status, extraHeaders)
}

export function health(version: string): Response {
  return json({ status: "ok", version }, 200)
}

export function root(version: string): Response {
  return json(
    {
      name: "ghfetch",
      version,
      description: "GitHub stats API built with TypeScript on Cloudflare Workers",
      endpoints: {
        stats: "/v1/stats?username=<github-username>",
        health: "/health",
      },
      source: "https://github.com/AmaneKai/ghfetch",
    },
    200
  )
}

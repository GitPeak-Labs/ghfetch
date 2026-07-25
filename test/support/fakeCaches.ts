// Bun has no `caches` global (that's a Workers/browser Cache API thing), so
// hono/cache silently no-ops without this. Installed fresh per test so cache
// state never leaks across tests.
let currentStores: Map<string, Map<string, Response>> | null = null

export function installFakeCaches(): void {
  const stores = new Map<string, Map<string, Response>>()
  currentStores = stores

  const fakeCaches = {
    async open(name: string) {
      const store = stores.get(name) ?? new Map<string, Response>()
      stores.set(name, store)

      return {
        async match(key: string) {
          const cached = store.get(key)
          return cached ? cached.clone() : undefined
        },
        async put(key: string, response: Response) {
          store.set(key, response.clone())
        },
      }
    },
  }

  // @ts-expect-error test-only global polyfill, not the full CacheStorage interface
  globalThis.caches = fakeCaches
}

export function getFakeCacheKeys(cacheName: string): string[] {
  const store = currentStores?.get(cacheName)
  return store ? Array.from(store.keys()) : []
}

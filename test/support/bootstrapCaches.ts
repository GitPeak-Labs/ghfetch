// hono/cache checks `globalThis.caches` once, at module-load time (when the
// `cache({...})` middleware factory runs while src/index.ts is being imported),
// not per-request. Bun has no `caches` global, so the polyfill must exist
// before `../src/index` is ever imported, not just before each test runs.
// Importing this module first (for its side effect) guarantees that ordering.
import { installFakeCaches } from "./fakeCaches"

installFakeCaches()

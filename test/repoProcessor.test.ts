import { describe, expect, test } from "bun:test"
import { processRepos } from "../src/domain/repoProcessor"
import type { Repo } from "../src/domain/graphqlResponse"

function makeRepo(
  name: string,
  stars: number,
  owner: string,
  langs: Array<[string, number]>
): Repo {
  return {
    name,
    owner: { login: owner },
    stargazerCount: stars,
    url: `https://github.com/${owner}/${name}`,
    languages: {
      edges: langs.map(([langName, size]) => ({ size, node: { name: langName } })),
    },
    pushedAt: "2026-04-09T00:00:00Z",
    isPrivate: false,
    isFork: false,
  }
}

describe("processRepos", () => {
  test("counts unique repos", () => {
    const repos = [makeRepo("repo-a", 5, "user", []), makeRepo("repo-b", 3, "user", [])]
    const processed = processRepos("user", repos, [], [])
    expect(processed.repoCount).toBe(2)
  })

  test("deduplicates repos by name", () => {
    const privateRepos = [makeRepo("repo-a", 5, "user", [])]
    const publicRepos = [makeRepo("repo-a", 5, "user", [])]
    const processed = processRepos("user", privateRepos, publicRepos, [])
    expect(processed.repoCount).toBe(1)
  })

  test("sums stars", () => {
    const repos = [makeRepo("repo-a", 10, "user", []), makeRepo("repo-b", 20, "user", [])]
    const processed = processRepos("user", repos, [], [])
    expect(processed.totalStars).toBe(30)
  })

  test("finds most starred", () => {
    const repos = [
      makeRepo("low", 1, "user", []),
      makeRepo("high", 99, "user", []),
      makeRepo("mid", 50, "user", []),
    ]
    const processed = processRepos("user", repos, [], [])
    expect(processed.mostStarredRepo?.name).toBe("high")
  })

  test("top repo must be owned", () => {
    const owned = [makeRepo("owned", 10, "user", [])]
    const contributed: Array<[Repo, string | null]> = [
      [makeRepo("other", 100, "someone-else", []), null],
    ]
    const processed = processRepos("user", owned, [], contributed)

    expect(processed.totalStars).toBe(110)
    expect(processed.mostStarredRepo?.name).toBe("owned")
  })

  test("averages language shares across repos", () => {
    const repos = [
      makeRepo("repo-a", 0, "user", [
        ["Rust", 1000],
        ["C", 500],
      ]),
      makeRepo("repo-b", 0, "user", [["Rust", 500]]),
    ]
    const processed = processRepos("user", repos, [], [])
    const rust = processed.languages.find(([name]) => name === "Rust")
    const c = processed.languages.find(([name]) => name === "C")
    expect(Math.abs((rust?.[1] ?? 0) - 0.833)).toBeLessThan(0.01)
    expect(Math.abs((c?.[1] ?? 0) - 0.167)).toBeLessThan(0.01)
  })

  test("sorts languages by share descending", () => {
    const repos = [
      makeRepo("repo", 0, "user", [
        ["C", 100],
        ["Rust", 900],
        ["Python", 500],
      ]),
    ]
    const processed = processRepos("user", repos, [], [])
    expect(processed.languages[0]?.[0]).toBe("Rust")
    expect(processed.languages[1]?.[0]).toBe("Python")
    expect(processed.languages[2]?.[0]).toBe("C")
  })

  test("includes owned repos in involved", () => {
    const owned = [makeRepo("owned-repo", 10, "user", [])]
    const processed = processRepos("user", owned, [], [])

    expect(processed.involvedRepos.length).toBe(1)
    expect(processed.involvedRepos[0]?.name).toBe("owned-repo")
    expect(processed.involvedRepos[0]?.isOwned).toBe(true)
  })

  test("marks external repos as not owned", () => {
    const external = makeRepo("other", 100, "someone-else", [])
    const contributed: Array<[Repo, string | null]> = [[external, "2026-05-01T00:00:00Z"]]
    const processed = processRepos("user", [], [], contributed)

    expect(processed.involvedRepos.length).toBe(1)
    expect(processed.involvedRepos[0]?.owner).toBe("someone-else")
    expect(processed.involvedRepos[0]?.isOwned).toBe(false)
  })

  test("handles empty repos", () => {
    const processed = processRepos("user", [], [], [])
    expect(processed.repoCount).toBe(0)
    expect(processed.totalStars).toBe(0)
    expect(processed.languages).toEqual([])
    expect(processed.mostStarredRepo).toBeNull()
    expect(processed.involvedRepos).toEqual([])
  })

  // Rust's equivalent test (`no_overflow_on_large_stars`) checks u32 saturating_add
  // doesn't wrap. JS numbers are float64 with no such overflow risk at these
  // magnitudes, so this checks the sum is exact instead.
  test("sums large star counts without precision loss", () => {
    const max = 4294967295
    const repos = [makeRepo("a", max, "user", []), makeRepo("b", max, "user", [])]
    const processed = processRepos("user", repos, [], [])
    expect(processed.totalStars).toBe(max + max)
  })

  test("empty repos dont dilute language percentages", () => {
    const repos = [
      makeRepo("repo-a", 0, "user", [["Rust", 1000]]),
      makeRepo("repo-b", 0, "user", []),
    ]
    const processed = processRepos("user", repos, [], [])
    const rust = processed.languages.find(([name]) => name === "Rust")
    expect(Math.abs((rust?.[1] ?? 0) - 1.0)).toBeLessThan(0.01)
  })
})

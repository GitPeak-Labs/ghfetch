import { describe, expect, test } from "bun:test"
import { aggregate, fetchCollaborators, isEligible } from "../src/domain/collaborators"
import type { InvolvedRepo } from "../src/shared/githubStats"
import { installFetchMock, jsonResponse } from "./support/fetchMock"

function involvedRepo(name: string, isPrivate: boolean, isFork: boolean): InvolvedRepo {
  return {
    name,
    owner: "owner",
    url: `https://github.com/owner/${name}`,
    lastContributedAt: "2026-01-01T00:00:00Z",
    stars: 0,
    primaryLanguage: null,
    isOwned: true,
    isPrivate,
    isFork,
  }
}

function entry(login: string, contributions: number, accountType: string) {
  return {
    login,
    avatar_url: `https://avatars/${login}`,
    contributions,
    type: accountType,
  }
}

function repo(name: string) {
  return {
    name,
    owner: "owner",
    url: `https://github.com/owner/${name}`,
    lastActivityAt: "2026-01-01T00:00:00Z",
  }
}

describe("aggregate", () => {
  test("excludes target user", () => {
    const repos = [repo("r1")]
    const perRepo = [[entry("target", 10, "User"), entry("friend", 5, "User")]]
    const result = aggregate("target", repos, perRepo)
    expect(result.length).toBe(1)
    expect(result[0]?.login).toBe("friend")
  })

  test("excludes bots", () => {
    const repos = [repo("r1")]
    const perRepo = [[entry("dependabot[bot]", 50, "Bot"), entry("friend", 5, "User")]]
    const result = aggregate("target", repos, perRepo)
    expect(result.length).toBe(1)
    expect(result[0]?.login).toBe("friend")
  })

  test("counts shared repos and sums commits", () => {
    const repos = [repo("r1"), repo("r2")]
    const perRepo = [[entry("friend", 10, "User")], [entry("friend", 7, "User")]]
    const result = aggregate("target", repos, perRepo)
    expect(result.length).toBe(1)
    expect(result[0]?.sharedRepos).toBe(2)
    expect(result[0]?.commits).toBe(17)
    expect(result[0]?.repos.length).toBe(2)
    expect(result[0]?.repos[0]?.name).toBe("r1")
    expect(result[0]?.repos[0]?.commits).toBe(10)
    expect(result[0]?.repos[0]?.lastActivityAt).toBe("2026-01-01T00:00:00Z")
    expect(result[0]?.repos[1]?.name).toBe("r2")
    expect(result[0]?.repos[1]?.commits).toBe(7)
  })

  test("sorts by shared repos then commits", () => {
    const repos = [repo("r1"), repo("r2")]
    const perRepo = [
      [entry("a", 100, "User"), entry("b", 1, "User")],
      [entry("b", 1, "User")],
    ]
    const result = aggregate("target", repos, perRepo)
    expect(result[0]?.login).toBe("b")
    expect(result[1]?.login).toBe("a")
  })

  test("caps at max collaborators", () => {
    const repos = [repo("r1")]
    const entries = Array.from({ length: 20 }, (_, i) => entry(`user${i}`, i, "User"))
    const result = aggregate("target", repos, [entries])
    expect(result.length).toBe(10)
  })
})

describe("isEligible", () => {
  test("excludes forks regardless of privacy", () => {
    expect(isEligible(involvedRepo("fork-public", false, true), false)).toBe(false)
    expect(isEligible(involvedRepo("fork-public", false, true), true)).toBe(false)
    expect(isEligible(involvedRepo("fork-private", true, true), true)).toBe(false)
  })

  test("includes non-fork respecting privacy", () => {
    expect(isEligible(involvedRepo("normal", false, false), false)).toBe(true)
    expect(isEligible(involvedRepo("normal-private", true, false), false)).toBe(false)
    expect(isEligible(involvedRepo("normal-private", true, false), true)).toBe(true)
  })
})

describe("fetchCollaborators", () => {
  test("bounds each contributors request with an abort signal", async () => {
    let capturedSignal: AbortSignal | undefined

    installFetchMock((_url, init) => {
      capturedSignal = init?.signal as AbortSignal | undefined
      return jsonResponse([])
    })

    await fetchCollaborators("token", "target", [involvedRepo("r1", false, false)], false)

    expect(capturedSignal).toBeInstanceOf(AbortSignal)
  })

  test("treats a hung/timed-out contributors request as no contributors for that repo", async () => {
    installFetchMock(() => {
      throw new DOMException("The operation timed out.", "TimeoutError")
    })

    const result = await fetchCollaborators(
      "token",
      "target",
      [involvedRepo("r1", false, false)],
      false
    )

    expect(result).toEqual([])
  })

  test("a single slow repo doesn't block contributors found on the others", async () => {
    installFetchMock((url) => {
      if (url.includes("/owner/slow/")) throw new DOMException("Timed out", "TimeoutError")
      return jsonResponse([{ login: "friend", avatar_url: "", contributions: 3, type: "User" }])
    })

    const result = await fetchCollaborators(
      "token",
      "target",
      [involvedRepo("slow", false, false), involvedRepo("fast", false, false)],
      false
    )

    expect(result.length).toBe(1)
    expect(result[0]?.login).toBe("friend")
    expect(result[0]?.sharedRepos).toBe(1)
  })
})

import { describe, expect, test } from "bun:test"
import { parseUsername } from "../src/domain/username"

function isValid(raw: string): boolean {
  return parseUsername(raw) !== null
}

describe("parseUsername", () => {
  test("accepts simple username", () => {
    expect(isValid("torvalds")).toBe(true)
  })

  test("accepts username with hyphen", () => {
    expect(isValid("pring-nt")).toBe(true)
  })

  test("accepts alphanumeric", () => {
    expect(isValid("user123")).toBe(true)
  })

  test("rejects empty", () => {
    expect(isValid("")).toBe(false)
  })

  test("rejects too long", () => {
    expect(isValid("a".repeat(40))).toBe(false)
  })

  test("rejects leading hyphen", () => {
    expect(isValid("-username")).toBe(false)
  })

  test("rejects trailing hyphen", () => {
    expect(isValid("username-")).toBe(false)
  })

  test("rejects double hyphen", () => {
    expect(isValid("user--name")).toBe(false)
  })

  test("rejects special chars", () => {
    expect(isValid("user@name")).toBe(false)
    expect(isValid("user name")).toBe(false)
    expect(isValid("user.name")).toBe(false)
    expect(isValid("<script>")).toBe(false)
  })

  test("normalizes to lowercase", () => {
    expect(parseUsername("AmaneKai")).toBe(parseUsername("amanekai"))
  })

  test("accepts max length", () => {
    expect(isValid("a".repeat(39))).toBe(true)
  })

  test("rejects whitespace only", () => {
    expect(isValid("   ")).toBe(false)
  })
})

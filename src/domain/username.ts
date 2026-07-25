import { z } from "zod"

declare const usernameBrand: unique symbol

export type Username = string & { readonly [usernameBrand]: true }

// GitHub username rules in one anchored pattern: alnum/hyphen only, 1-39 chars,
// no leading/trailing hyphen, no consecutive hyphens.
const USERNAME_PATTERN = /^(?!-)(?!.*--)[a-zA-Z0-9-]{1,39}(?<!-)$/

export const usernameSchema = z
  .string()
  .transform((value) => value.trim())
  .pipe(z.string().regex(USERNAME_PATTERN, "Invalid username format"))
  .transform((value) => value.toLowerCase() as Username)

export function parseUsername(raw: string): Username | null {
  const result = usernameSchema.safeParse(raw)
  return result.success ? result.data : null
}

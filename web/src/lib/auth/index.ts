/**
 * Auth Provider
 * 
 * Gets the appropriate auth adapter based on configuration.
 */

import { getAuthType, type AuthAdapter } from "./adapter"
import { localAuth } from "./local"

/**
 * Get the configured auth adapter
 */
export function getAuthAdapter(): AuthAdapter {
  const authType = getAuthType()

  switch (authType) {
    case "local":
      return localAuth
    case "supabase":
      // Future: import and return Supabase adapter
      throw new Error("Supabase auth not implemented yet - use 'local' for self-hosted")
    default:
      return localAuth
  }
}

// Re-export types
export type { AuthAdapter, User } from "./adapter"
export { getAuthType } from "./adapter"

/**
 * Auth Adapter Pattern
 * 
 * Provides a unified interface for authentication that can switch
 * between Local (self-hosted) and Cloud (Supabase) implementations.
 */

export interface User {
  id: string
  email?: string
  name?: string
  isAdmin: boolean
}

export interface AuthAdapter {
  /**
   * Get the currently authenticated user
   */
  getUser(): Promise<User | null>

  /**
   * Sign in with credentials
   */
  signIn(credentials: { password?: string; token?: string }): Promise<User>

  /**
   * Sign out the current user
   */
  signOut(): Promise<void>

  /**
   * Check if the current session is valid
   */
  isAuthenticated(): Promise<boolean>
}

// Auth adapter implementation selector
export type AuthType = "local" | "supabase"

/**
 * Get the current auth type from environment
 */
export function getAuthType(): AuthType {
  return (process.env.NEXT_PUBLIC_AUTH_TYPE as AuthType) || "local"
}

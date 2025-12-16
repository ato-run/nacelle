/**
 * Local Auth Implementation
 * 
 * Simple password-based authentication for self-hosted instances.
 * Password is set via GUMBALL_ADMIN_PASSWORD environment variable.
 */

import { cookies } from "next/headers"
import type { AuthAdapter, User } from "./adapter"

const SESSION_COOKIE = "gumball_session"
const SESSION_DURATION = 60 * 60 * 24 * 7 // 7 days

// Simple session token generator
function generateSessionToken(): string {
  const array = new Uint8Array(32)
  crypto.getRandomValues(array)
  return Array.from(array, (b) => b.toString(16).padStart(2, "0")).join("")
}

// In-memory session store (for simplicity - production should use Redis/DB)
const sessions = new Map<string, { userId: string; expiresAt: number }>()

export class LocalAuthAdapter implements AuthAdapter {
  private adminPassword: string

  constructor() {
    this.adminPassword = process.env.GUMBALL_ADMIN_PASSWORD || "admin"
  }

  async getUser(): Promise<User | null> {
    const cookieStore = await cookies()
    const sessionToken = cookieStore.get(SESSION_COOKIE)?.value

    if (!sessionToken) {
      return null
    }

    const session = sessions.get(sessionToken)
    if (!session || session.expiresAt < Date.now()) {
      sessions.delete(sessionToken)
      return null
    }

    return {
      id: session.userId,
      name: "Admin",
      isAdmin: true,
    }
  }

  async signIn(credentials: { password?: string }): Promise<User> {
    const { password } = credentials

    if (password !== this.adminPassword) {
      throw new Error("Invalid password")
    }

    const token = generateSessionToken()
    const userId = "local-admin"

    sessions.set(token, {
      userId,
      expiresAt: Date.now() + SESSION_DURATION * 1000,
    })

    // Set cookie
    const cookieStore = await cookies()
    cookieStore.set(SESSION_COOKIE, token, {
      httpOnly: true,
      secure: process.env.NODE_ENV === "production",
      sameSite: "lax",
      maxAge: SESSION_DURATION,
    })

    return {
      id: userId,
      name: "Admin",
      isAdmin: true,
    }
  }

  async signOut(): Promise<void> {
    const cookieStore = await cookies()
    const sessionToken = cookieStore.get(SESSION_COOKIE)?.value

    if (sessionToken) {
      sessions.delete(sessionToken)
      cookieStore.delete(SESSION_COOKIE)
    }
  }

  async isAuthenticated(): Promise<boolean> {
    const user = await this.getUser()
    return user !== null
  }
}

// Singleton instance
export const localAuth = new LocalAuthAdapter()

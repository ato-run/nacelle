/**
 * Capsule API Client
 * 
 * Server-side API client for communicating with the Coordinator.
 * Uses HTTP/gRPC-Web or direct gRPC depending on environment.
 */

export interface Capsule {
  id: string
  name: string
  status: "running" | "stopped" | "starting" | "stopping" | "error"
  port: number | null
  vram: string
  image: string
  uptime: string | null
  version: string
  type: "inference" | "tool" | "app"
  createdAt: string
}

export interface CapsuleStats {
  activeCapsules: number
  cpuUsage: number
  memoryUsage: string
  requestsPerMinute: number
}

export interface CapsuleLogs {
  entries: Array<{
    timestamp: string
    level: "INFO" | "WARN" | "ERROR" | "DEBUG"
    message: string
  }>
}

const COORDINATOR_URL = process.env.COORDINATOR_URL || "http://127.0.0.1:8080"

/**
 * API Client class for Coordinator communication
 */
class CoordinatorClient {
  private baseUrl: string

  constructor(baseUrl: string = COORDINATOR_URL) {
    this.baseUrl = baseUrl
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`
    
    const response = await fetch(url, {
      ...options,
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
    })

    if (!response.ok) {
      throw new Error(`API Error: ${response.status} ${response.statusText}`)
    }

    return response.json()
  }

  // Capsule Operations
  async listCapsules(): Promise<Capsule[]> {
    return this.request<Capsule[]>("/api/v1/capsules")
  }

  async getCapsule(id: string): Promise<Capsule> {
    return this.request<Capsule>(`/api/v1/capsules/${id}`)
  }

  async deployCapsule(tomlContent: string): Promise<{ id: string; url: string }> {
    return this.request("/api/v1/capsules", {
      method: "POST",
      body: JSON.stringify({ toml: tomlContent }),
    })
  }

  async startCapsule(id: string): Promise<{ success: boolean }> {
    return this.request(`/api/v1/capsules/${id}/start`, {
      method: "POST",
    })
  }

  async stopCapsule(id: string): Promise<{ success: boolean }> {
    return this.request(`/api/v1/capsules/${id}/stop`, {
      method: "POST",
    })
  }

  async deleteCapsule(id: string): Promise<{ success: boolean }> {
    return this.request(`/api/v1/capsules/${id}`, {
      method: "DELETE",
    })
  }

  async getCapsuleLogs(id: string, limit = 100): Promise<CapsuleLogs> {
    return this.request<CapsuleLogs>(`/api/v1/capsules/${id}/logs?limit=${limit}`)
  }

  // System Stats
  async getStats(): Promise<CapsuleStats> {
    return this.request<CapsuleStats>("/api/v1/stats")
  }
}

// Singleton instance
export const coordinatorClient = new CoordinatorClient()

// Re-export for convenience
export { CoordinatorClient }

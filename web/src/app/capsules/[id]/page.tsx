import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import Link from "next/link"
import { ArrowLeft, PlayCircle, StopCircle, Terminal, Settings, Trash2 } from "lucide-react"

// Mock data - will come from API
const getCapsule = (id: string) => ({
  id,
  name: "Qwen3 8B",
  status: "running",
  port: 8081,
  vram: "6.2 GB",
  image: "ghcr.io/gumball/mlx-qwen3:latest",
  uptime: "2h 34m",
  version: "1.0.0",
  type: "inference",
  metadata: {
    description: "Qwen3 8B model optimized for Apple Silicon using MLX",
    author: "Gumball Team",
  },
  network: {
    egress_allow: ["huggingface.co:443", "cdn-lfs.huggingface.co:443"]
  },
  requirements: {
    vram_min: "6GB"
  },
  logs: [
    { timestamp: "2024-12-16T11:50:00Z", level: "INFO", message: "Server started on port 8081" },
    { timestamp: "2024-12-16T11:50:01Z", level: "INFO", message: "Model loaded successfully (6.2GB VRAM)" },
    { timestamp: "2024-12-16T11:52:30Z", level: "INFO", message: "Health check passed" },
  ]
})

export default async function CapsuleDetailPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const { id } = await params
  const capsule = getCapsule(id)

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Link href="/capsules">
            <Button variant="ghost" size="icon">
              <ArrowLeft className="size-4" />
            </Button>
          </Link>
          <div className="flex items-center gap-3">
            <div
              className={`size-3 rounded-full ${
                capsule.status === "running" ? "bg-green-500 animate-pulse" : "bg-gray-400"
              }`}
            />
            <div>
              <h2 className="text-2xl font-bold">{capsule.name}</h2>
              <p className="text-sm text-muted-foreground font-mono">{capsule.id}</p>
            </div>
          </div>
        </div>
        <div className="flex gap-2">
          {capsule.status === "running" ? (
            <Button variant="destructive">
              <StopCircle className="mr-2 size-4" />
              Stop
            </Button>
          ) : (
            <Button>
              <PlayCircle className="mr-2 size-4" />
              Start
            </Button>
          )}
        </div>
      </div>

      <div className="grid gap-6 md:grid-cols-3">
        {/* Main Info */}
        <Card className="md:col-span-2">
          <CardHeader>
            <CardTitle>Details</CardTitle>
            <CardDescription>{capsule.metadata.description}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <p className="text-muted-foreground">Image</p>
                <p className="font-mono">{capsule.image}</p>
              </div>
              <div>
                <p className="text-muted-foreground">Version</p>
                <p>{capsule.version}</p>
              </div>
              <div>
                <p className="text-muted-foreground">Type</p>
                <p className="capitalize">{capsule.type}</p>
              </div>
              <div>
                <p className="text-muted-foreground">Port</p>
                <p>{capsule.port}</p>
              </div>
            </div>

            <Separator />

            <div>
              <p className="text-sm text-muted-foreground mb-2">Egress Allow</p>
              <div className="flex flex-wrap gap-2">
                {capsule.network.egress_allow.map((rule) => (
                  <span key={rule} className="rounded-md bg-muted px-2 py-1 text-xs font-mono">
                    {rule}
                  </span>
                ))}
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Actions Sidebar */}
        <Card>
          <CardHeader>
            <CardTitle>Actions</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            <Button variant="outline" className="w-full justify-start">
              <Terminal className="mr-2 size-4" />
              View Logs
            </Button>
            <Button variant="outline" className="w-full justify-start">
              <Settings className="mr-2 size-4" />
              Configure
            </Button>
            <Button variant="outline" className="w-full justify-start text-red-500 hover:text-red-600">
              <Trash2 className="mr-2 size-4" />
              Delete
            </Button>
          </CardContent>
        </Card>
      </div>

      {/* Logs */}
      <Card>
        <CardHeader>
          <CardTitle>Recent Logs</CardTitle>
          <CardDescription>Last 100 log entries</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="rounded-lg bg-black p-4 font-mono text-xs text-green-400 space-y-1 max-h-64 overflow-auto">
            {capsule.logs.map((log, i) => (
              <div key={i} className="flex gap-2">
                <span className="text-gray-500">{log.timestamp}</span>
                <span className={log.level === "ERROR" ? "text-red-400" : "text-blue-400"}>
                  [{log.level}]
                </span>
                <span className="text-gray-300">{log.message}</span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import Link from "next/link"
import { Plus, PlayCircle, StopCircle, MoreVertical } from "lucide-react"

// Mock data - will be replaced with API calls
const capsules = [
  { 
    id: "qwen3-8b", 
    name: "Qwen3 8B", 
    status: "running", 
    port: 8081, 
    vram: "6.2 GB",
    image: "ghcr.io/gumball/mlx-qwen3:latest",
    uptime: "2h 34m"
  },
  { 
    id: "stable-diff", 
    name: "Stable Diffusion XL", 
    status: "running", 
    port: 8082, 
    vram: "4.1 GB",
    image: "ghcr.io/gumball/sdxl:latest",
    uptime: "1h 12m"
  },
  { 
    id: "whisper", 
    name: "Whisper Large v3", 
    status: "stopped", 
    port: null, 
    vram: "0 GB",
    image: "ghcr.io/gumball/whisper:latest",
    uptime: null
  },
  { 
    id: "llama3", 
    name: "Llama 3 8B", 
    status: "stopped", 
    port: null, 
    vram: "0 GB",
    image: "ghcr.io/gumball/llama3-8b:latest",
    uptime: null
  },
]

export default function CapsulesPage() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">Capsules</h2>
          <p className="text-muted-foreground">
            Manage your AI models and services
          </p>
        </div>
        <Button>
          <Plus className="mr-2 size-4" />
          Deploy Capsule
        </Button>
      </div>

      <div className="grid gap-4">
        {capsules.map((capsule) => (
          <Card key={capsule.id}>
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div
                    className={`size-3 rounded-full ${
                      capsule.status === "running" ? "bg-green-500 animate-pulse" : "bg-gray-400"
                    }`}
                  />
                  <div>
                    <CardTitle className="text-lg">
                      <Link 
                        href={`/capsules/${capsule.id}`}
                        className="hover:underline"
                      >
                        {capsule.name}
                      </Link>
                    </CardTitle>
                    <CardDescription className="font-mono text-xs">
                      {capsule.image}
                    </CardDescription>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  {capsule.status === "running" ? (
                    <Button variant="ghost" size="sm" className="text-red-500 hover:text-red-600">
                      <StopCircle className="mr-1 size-4" />
                      Stop
                    </Button>
                  ) : (
                    <Button variant="ghost" size="sm" className="text-green-500 hover:text-green-600">
                      <PlayCircle className="mr-1 size-4" />
                      Start
                    </Button>
                  )}
                  <Button variant="ghost" size="icon">
                    <MoreVertical className="size-4" />
                  </Button>
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-4 gap-4 text-sm">
                <div>
                  <p className="text-muted-foreground">Status</p>
                  <p className={capsule.status === "running" ? "text-green-500" : "text-gray-500"}>
                    {capsule.status}
                  </p>
                </div>
                <div>
                  <p className="text-muted-foreground">Port</p>
                  <p>{capsule.port ?? "—"}</p>
                </div>
                <div>
                  <p className="text-muted-foreground">VRAM</p>
                  <p>{capsule.vram}</p>
                </div>
                <div>
                  <p className="text-muted-foreground">Uptime</p>
                  <p>{capsule.uptime ?? "—"}</p>
                </div>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  )
}

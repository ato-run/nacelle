import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Activity, Package, Cpu, HardDrive } from "lucide-react"

// Mock data - will be replaced with real API calls
const stats = [
  { title: "Active Capsules", value: "3", icon: Package, trend: "+1" },
  { title: "CPU Usage", value: "42%", icon: Cpu, trend: "-5%" },
  { title: "Memory", value: "8.2 GB", icon: HardDrive, trend: "+0.5 GB" },
  { title: "Requests/min", value: "127", icon: Activity, trend: "+23" },
]

const capsules = [
  { id: "qwen3-8b", name: "Qwen3 8B", status: "running", port: 8081, vram: "6.2 GB" },
  { id: "stable-diff", name: "Stable Diffusion", status: "running", port: 8082, vram: "4.1 GB" },
  { id: "whisper", name: "Whisper Large", status: "stopped", port: null, vram: "0 GB" },
]

export default function DashboardPage() {
  return (
    <div className="space-y-6">
      {/* Stats Grid */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        {stats.map((stat) => (
          <Card key={stat.title}>
            <CardHeader className="flex flex-row items-center justify-between pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                {stat.title}
              </CardTitle>
              <stat.icon className="size-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{stat.value}</div>
              <p className="text-xs text-muted-foreground">{stat.trend} from last hour</p>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Capsule List */}
      <Card>
        <CardHeader>
          <CardTitle>Active Capsules</CardTitle>
          <CardDescription>Manage your deployed AI models and services</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {capsules.map((capsule) => (
              <div
                key={capsule.id}
                className="flex items-center justify-between rounded-lg border p-4"
              >
                <div className="flex items-center gap-4">
                  <div
                    className={`size-3 rounded-full ${
                      capsule.status === "running" ? "bg-green-500" : "bg-gray-400"
                    }`}
                  />
                  <div>
                    <p className="font-medium">{capsule.name}</p>
                    <p className="text-sm text-muted-foreground">
                      {capsule.status === "running"
                        ? `Port ${capsule.port} • ${capsule.vram} VRAM`
                        : "Not running"}
                    </p>
                  </div>
                </div>
                <div className="flex gap-2">
                  {capsule.status === "running" ? (
                    <button className="rounded-md bg-red-500/10 px-3 py-1 text-sm text-red-500 hover:bg-red-500/20">
                      Stop
                    </button>
                  ) : (
                    <button className="rounded-md bg-green-500/10 px-3 py-1 text-sm text-green-500 hover:bg-green-500/20">
                      Start
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

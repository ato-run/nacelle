import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

export default function SettingsPage() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Settings</h2>
        <p className="text-muted-foreground">Configure your Gumball instance</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>General</CardTitle>
          <CardDescription>Basic configuration</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-2">
            <label className="text-sm font-medium">Instance Name</label>
            <Input defaultValue="My Personal Cloud" />
          </div>
          <div className="grid gap-2">
            <label className="text-sm font-medium">API Base URL</label>
            <Input defaultValue="http://localhost:50051" disabled />
            <p className="text-xs text-muted-foreground">Coordinator gRPC endpoint</p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Security</CardTitle>
          <CardDescription>Authentication and access control</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-2">
            <label className="text-sm font-medium">Admin Password</label>
            <Input type="password" placeholder="••••••••" />
          </div>
          <Button>Update Password</Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>About</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-sm space-y-1">
            <p><span className="text-muted-foreground">Version:</span> 1.0.0</p>
            <p><span className="text-muted-foreground">ADEP Protocol:</span> v1.0.0</p>
            <p><span className="text-muted-foreground">Cold Start:</span> 175µs</p>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

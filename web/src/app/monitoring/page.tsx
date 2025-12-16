import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

export default function MonitoringPage() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Monitoring</h2>
        <p className="text-muted-foreground">Real-time system metrics</p>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>CPU Usage</CardTitle>
            <CardDescription>Last 5 minutes</CardDescription>
          </CardHeader>
          <CardContent className="h-48 flex items-center justify-center text-muted-foreground">
            Chart placeholder
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Memory Usage</CardTitle>
            <CardDescription>Last 5 minutes</CardDescription>
          </CardHeader>
          <CardContent className="h-48 flex items-center justify-center text-muted-foreground">
            Chart placeholder
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>VRAM Usage</CardTitle>
            <CardDescription>Per capsule breakdown</CardDescription>
          </CardHeader>
          <CardContent className="h-48 flex items-center justify-center text-muted-foreground">
            Chart placeholder
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Request Rate</CardTitle>
            <CardDescription>Requests per second</CardDescription>
          </CardHeader>
          <CardContent className="h-48 flex items-center justify-center text-muted-foreground">
            Chart placeholder
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

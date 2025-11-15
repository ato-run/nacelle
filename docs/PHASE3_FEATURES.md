# Phase 3 Features: Log Streaming and Metrics Monitoring

This document describes the Phase 3 features implemented for Capsuled: log streaming and metrics monitoring.

## Overview

Phase 3 (Week 7-8) implements the following functionality:
- **Log Collection & Streaming**: Real-time container log access via WebSocket
- **Prometheus Metrics**: Custom metrics for monitoring capsule and GPU resources
- **Health Checks**: Enhanced readiness and liveness probes for Kubernetes

---

## 1. Log Collection & Streaming

### Engine: Log Collector

The Engine collects container logs using inotify-based file watching.

**Location**: `engine/src/logs/collector.rs`

**Features**:
- Real-time log file monitoring using `notify` crate
- Support for stdout/stderr streams
- Historical log retrieval with tail option
- Automatic log rotation handling

**Usage Example**:
```rust
use capsuled_engine::logs::LogCollector;
use std::path::PathBuf;

let collector = LogCollector::new();

// Start collecting logs for a capsule
let log_path = PathBuf::from("/var/log/capsules/capsule-123.log");
let mut stream = collector.start_collecting(
    "capsule-123".to_string(),
    log_path
)?;

// Receive log entries in real-time
while let Some(entry) = stream.next().await {
    println!("[{}] {}: {}", entry.timestamp, entry.stream.to_string(), entry.line);
}

// Stop collecting when done
collector.stop_collecting("capsule-123")?;
```

### Client: WebSocket Streaming API

The Client provides a WebSocket endpoint for streaming logs to clients.

**Location**: `client/pkg/api/logs_handler.go`

**Endpoint**: `GET /api/v1/capsules/:id/logs`

**Query Parameters**:
- `follow` (boolean): Enable follow mode for real-time streaming (default: false)
- `tail` (integer): Number of historical lines to retrieve (default: 100)

**Example Request**:
```bash
# Get last 50 log lines
curl -N -H "Upgrade: websocket" \
  -H "Connection: Upgrade" \
  "ws://localhost:8080/api/v1/capsules/capsule-123/logs?tail=50"

# Follow mode (streaming)
curl -N -H "Upgrade: websocket" \
  -H "Connection: Upgrade" \
  "ws://localhost:8080/api/v1/capsules/capsule-123/logs?follow=true&tail=10"
```

**WebSocket Message Format** (JSON):
```json
{
  "timestamp": 1700000000,
  "stream": "stdout",
  "line": "Container started successfully"
}
```

---

## 2. Prometheus Metrics

### Engine: Metrics Collector

The Engine exposes Prometheus-compatible metrics for monitoring.

**Location**: `engine/src/metrics/prometheus_metrics.rs`

**Custom Metrics**:

| Metric Name | Type | Description | Labels |
|-------------|------|-------------|--------|
| `capsuled_engine_capsule_count` | IntGauge | Total number of capsules | - |
| `capsuled_engine_gpu_vram_total_bytes` | Gauge | Total GPU VRAM in bytes | - |
| `capsuled_engine_gpu_vram_used_bytes` | Gauge | Used GPU VRAM in bytes | - |
| `capsuled_engine_gpu_vram_available_bytes` | Gauge | Available GPU VRAM in bytes | - |
| `capsuled_engine_container_cpu_usage` | GaugeVec | CPU usage per container | capsule_id |
| `capsuled_engine_capsule_status` | GaugeVec | Status of capsules (1=active) | capsule_id, status |

**Usage Example**:
```rust
use capsuled_engine::metrics::MetricsCollector;
use std::sync::Arc;

let collector = Arc::new(MetricsCollector::new()?);

// Update metrics
collector.set_capsule_count(5);
collector.set_gpu_vram_metrics(
    8_589_934_592.0,  // 8GB total
    4_294_967_296.0,  // 4GB used
    4_294_967_296.0   // 4GB available
);
collector.set_container_cpu_usage("capsule-123", 75.5);
collector.set_capsule_status("capsule-123", "running", 1.0);

// Gather metrics for Prometheus
let metrics_text = collector.gather()?;
println!("{}", metrics_text);
```

### Engine: HTTP Server with Metrics Endpoint

**Location**: `engine/src/http_server.rs`

**Endpoint**: `GET /metrics`

**Example Request**:
```bash
curl http://localhost:9090/metrics
```

**Example Response**:
```
# HELP capsuled_engine_capsule_count Total number of capsules
# TYPE capsuled_engine_capsule_count gauge
capsuled_engine_capsule_count 5

# HELP capsuled_engine_gpu_vram_total_bytes Total GPU VRAM in bytes
# TYPE capsuled_engine_gpu_vram_total_bytes gauge
capsuled_engine_gpu_vram_total_bytes 8589934592

# HELP capsuled_engine_container_cpu_usage CPU usage per container
# TYPE capsuled_engine_container_cpu_usage gauge
capsuled_engine_container_cpu_usage{capsule_id="capsule-123"} 75.5
```

### Prometheus Configuration

Add the Engine to your Prometheus scrape config:

```yaml
scrape_configs:
  - job_name: 'capsuled-engine'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 30s
```

---

## 3. Health Check Endpoints

### Client: Enhanced Health Checks

**Location**: `client/pkg/api/health_handler.go`

The Client provides enhanced health checks with dependency monitoring.

**Endpoints**:

#### General Health: `GET /health`
Returns overall health status and uptime.

```bash
curl http://localhost:8080/health
```

Response:
```json
{
  "status": "healthy",
  "uptime": "2h15m30s",
  "timestamp": "2025-11-15T21:45:00Z",
  "version": "0.1.0"
}
```

#### Readiness Probe: `GET /ready`
Checks if the service is ready to accept requests. Includes dependency checks.

```bash
curl http://localhost:8080/ready
```

Response (all healthy):
```json
{
  "status": "ready",
  "uptime": "2h15m30s",
  "timestamp": "2025-11-15T21:45:00Z",
  "version": "0.1.0",
  "dependencies": {
    "database": {"status": "healthy"},
    "grpc": {"status": "healthy"}
  }
}
```

Response (unhealthy dependency) - HTTP 503:
```json
{
  "status": "not ready",
  "uptime": "2h15m30s",
  "timestamp": "2025-11-15T21:45:00Z",
  "version": "0.1.0",
  "dependencies": {
    "database": {"status": "healthy"},
    "grpc": {
      "status": "unhealthy",
      "message": "connection refused"
    }
  }
}
```

#### Liveness Probe: `GET /live`
Simple check to verify the service is alive.

```bash
curl http://localhost:8080/live
```

Response:
```
alive
```

**Adding Custom Health Checkers**:

```go
import (
    "context"
    "github.com/onescluster/coordinator/pkg/api"
)

// Implement HealthChecker interface
type DatabaseChecker struct {
    db *sql.DB
}

func (c *DatabaseChecker) Check(ctx context.Context) error {
    return c.db.PingContext(ctx)
}

// Add to health handler
handler := api.NewHealthHandler()
handler.AddChecker("database", &DatabaseChecker{db: myDB})
```

### Engine: Health Check Endpoints

**Location**: `engine/src/http_server.rs`

The Engine provides similar health check endpoints for monitoring.

**Endpoints**:
- `GET /health` - General health status (JSON)
- `GET /ready` - Readiness probe (JSON)
- `GET /live` - Liveness probe (text: "alive")

---

## Kubernetes Integration

### Deployment Configuration

Example Kubernetes deployment with health checks and metrics:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: capsuled-client
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: client
        image: capsuled/client:latest
        ports:
        - name: http
          containerPort: 8080
        livenessProbe:
          httpGet:
            path: /live
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
---
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: capsuled-engine
spec:
  template:
    spec:
      containers:
      - name: engine
        image: capsuled/engine:latest
        ports:
        - name: grpc
          containerPort: 50051
        - name: metrics
          containerPort: 9090
        livenessProbe:
          httpGet:
            path: /live
            port: 9090
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /ready
            port: 9090
          initialDelaySeconds: 5
          periodSeconds: 10
```

### ServiceMonitor for Prometheus Operator

```yaml
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: capsuled-engine
spec:
  selector:
    matchLabels:
      app: capsuled-engine
  endpoints:
  - port: metrics
    interval: 30s
    path: /metrics
```

---

## Testing

### Unit Tests

All features include comprehensive unit tests:

**Engine Tests**:
```bash
cd engine
cargo test logs::collector  # Log collector tests (4 tests)
cargo test metrics::prometheus_metrics  # Metrics tests (8 tests)
cargo test http_server  # HTTP server tests (7 tests)
```

**Client Tests**:
```bash
cd client
go test ./pkg/api -run TestLogs  # Log handler tests (7 tests)
go test ./pkg/api -run TestHealth  # Health handler tests (6 tests)
```

### Integration Testing

Example integration test for log streaming:

```go
func TestLogStreamingIntegration(t *testing.T) {
    // Start client server
    server := startTestServer()
    defer server.Close()
    
    // Connect via WebSocket
    wsURL := "ws://" + server.Addr + "/api/v1/capsules/test-123/logs?follow=true"
    conn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
    require.NoError(t, err)
    defer conn.Close()
    
    // Receive log messages
    for i := 0; i < 10; i++ {
        _, message, err := conn.ReadMessage()
        require.NoError(t, err)
        
        var entry LogEntry
        err = json.Unmarshal(message, &entry)
        require.NoError(t, err)
        
        assert.NotZero(t, entry.Timestamp)
        assert.NotEmpty(t, entry.Line)
    }
}
```

---

## Performance Considerations

### Log Collection
- **Buffer Size**: WebSocket channel buffer is 1000 entries
- **File Watching**: Uses efficient inotify-based watching (Linux)
- **Memory**: Bounded by channel buffer, old entries dropped if full

### Metrics
- **Collection**: Metrics are stored in-memory with minimal overhead
- **Scraping**: Text encoding is fast, typical response < 1ms
- **Labels**: Use bounded label cardinality to avoid memory issues

### Health Checks
- **Timeout**: 5-second timeout for dependency checks
- **Caching**: Consider caching health check results for high-frequency probes
- **Graceful Degradation**: Service continues if optional dependencies fail

---

## Troubleshooting

### Log Streaming Issues

**Problem**: No logs appearing in stream
- Check if log file exists at expected path
- Verify file permissions for Engine process
- Check WebSocket connection is established (status 101)

**Problem**: Logs appear delayed
- Verify file watching is working (check Engine logs)
- Consider reducing buffer size if experiencing delays
- Check for disk I/O bottlenecks

### Metrics Issues

**Problem**: Metrics endpoint returns 404
- Ensure Engine HTTP server is running (check logs)
- Verify correct port (default: 9090)
- Check firewall rules

**Problem**: Metrics show zeros
- Verify MetricsCollector is being updated
- Check if capsules are actually running
- Review metric update logic in code

### Health Check Issues

**Problem**: Readiness probe fails
- Check individual dependency status in JSON response
- Verify network connectivity to dependencies
- Review dependency checker implementation
- Check timeout settings (default: 5 seconds)

---

## Future Enhancements

Phase 3+ potential improvements:
- Log aggregation to Loki/Elasticsearch
- Additional metrics (network I/O, disk usage)
- Grafana dashboard templates
- Alert manager integration
- Log filtering and search
- Metric retention policies

---

## References

- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
- [Kubernetes Health Checks](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/)
- [WebSocket Protocol](https://datatracker.ietf.org/doc/html/rfc6455)
- [TODO.md Phase 3](../TODO.md#phase-3-week-7-9)

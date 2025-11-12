# Coordinator Quick Start Guide

## Prerequisites

1. **Go 1.21+** installed
2. **rqlite** cluster running (for production setup)

## Development Setup

### 1. Install rqlite (for local development)

```bash
# macOS
brew install rqlite

# Linux
curl -L https://github.com/rqlite/rqlite/releases/download/v7.21.4/rqlite-v7.21.4-linux-amd64.tar.gz -o rqlite.tar.gz
tar xvfz rqlite.tar.gz
cd rqlite-v7.21.4-linux-amd64
```

### 2. Start rqlite

```bash
# Single node (development)
rqlited -node-id 1 ~/node.1

# The rqlite HTTP API will be available at http://localhost:4001
```

### 3. Build the Coordinator

```bash
cd capsuled/client
go build -o bin/capsuled-client ./cmd/client
```

### 4. Create Configuration

```bash
# Copy example config
cp config.yaml.example config.yaml

# Edit with your settings
# For development, the defaults work with a local rqlite instance
```

### 5. Run the Coordinator

```bash
./bin/capsuled-client -config config.yaml
```

## Expected Output

```
Capsuled Coordinator starting...
Version: 1.0.0
Loading configuration from config.yaml
Generated node ID: 01HK5XXXXXXXXXXXXXXXXXXXX
Connecting to rqlite cluster...
Connected to rqlite successfully
Initializing database schema...
Schema initialized successfully
Initializing state manager...
State manager initialized: 0 nodes, 0 capsules, 0 resource entries
Initializing cluster state...
Cluster state initialized
Registering node in cluster...
Registering new node: 01HK5XXXXXXXXXXXXXXXXXXXX
Performing health check...
Health check passed - Stats: map[active_nodes:1 has_master:false master_node_id:<nil> running_capsules:0 total_capsules:0 total_nodes:1]
Cluster state: map[active_nodes:1 has_master:false running_capsules:0 total_capsules:0 total_nodes:1]
Coordinator initialized successfully
Node: 01HK5XXXXXXXXXXXXXXXXXXXX (coordinator-1)
Address: 0.0.0.0:8080
```

## Testing

### Run Unit Tests

```bash
# All tests
go test ./...

# Specific package
go test -v ./pkg/db/
go test -v ./pkg/config/

# With coverage
go test -cover ./...
```

### Verify Database

```bash
# Connect to rqlite CLI
rqlite -H localhost:4001

# Check tables
.tables

# Query nodes
SELECT * FROM nodes;

# Query metadata
SELECT * FROM cluster_metadata;

# Exit
.exit
```

## Configuration Options

### Minimal Config (Development)

```yaml
coordinator:
  headscale_name: "coordinator-1"

rqlite:
  addresses:
    - "http://localhost:4001"
```

### Production Config

```yaml
coordinator:
  node_id: "01HK5XXXXXXXXXXXXXXXXXXXX"  # Optional: auto-generated if empty
  address: "192.168.1.10:8080"
  headscale_name: "coordinator-prod-1"

rqlite:
  addresses:
    - "http://rqlite-1:4001"
    - "http://rqlite-2:4001"
    - "http://rqlite-3:4001"
  max_retries: 5
  retry_delay: 3
  timeout: 15

cluster:
  gossip_bind_addr: "0.0.0.0:7946"
  peers:
    - "192.168.1.11:7946"
    - "192.168.1.12:7946"
  heartbeat_interval: 5
  node_timeout: 30

headscale:
  api_url: "http://headscale:8080"
  api_key: "${HEADSCALE_API_KEY}"
  timeout: 10

api:
  listen_addr: "0.0.0.0:8080"
  tls_enabled: true
  tls_cert: "/etc/capsuled/tls/cert.pem"
  tls_key: "/etc/capsuled/tls/key.pem"

logging:
  level: "info"
  format: "json"
```

## Troubleshooting

### Cannot connect to rqlite

**Error**: `Failed to connect to rqlite: failed to connect after 3 attempts`

**Solution**:
1. Ensure rqlite is running: `ps aux | grep rqlite`
2. Check rqlite is listening: `curl http://localhost:4001/status`
3. Verify addresses in config.yaml match your rqlite setup

### Schema initialization failed

**Error**: `Schema initialization failed: failed to execute schema`

**Solution**:
- This usually means the schema already exists, which is fine
- The coordinator will verify the existing schema
- If verification fails, check rqlite logs for corruption

### Node registration failed

**Error**: `Failed to register node: failed to create node`

**Solution**:
1. Check rqlite is writable: `rqlite -H localhost:4001 "SELECT 1"`
2. Verify the node_id is unique (or let it auto-generate)
3. Check rqlite cluster has a leader

## Multi-Node Setup

To run multiple coordinators (preparation for Step 2):

### Node 1
```yaml
# config-node1.yaml
coordinator:
  address: "192.168.1.10:8080"
  headscale_name: "coordinator-1"

rqlite:
  addresses:
    - "http://192.168.1.10:4001"
```

```bash
./bin/capsuled-client -config config-node1.yaml
```

### Node 2
```yaml
# config-node2.yaml
coordinator:
  address: "192.168.1.11:8080"
  headscale_name: "coordinator-2"

rqlite:
  addresses:
    - "http://192.168.1.10:4001"  # Same rqlite cluster
```

```bash
./bin/capsuled-client -config config-node2.yaml
```

Both nodes will register in the same cluster and see each other's state.

## Next Steps

After verifying Step 1 works:

1. **Step 2**: Implement clustering with memberlist
   - Gossip protocol for node discovery
   - Master election based on ULID
   - Heartbeat mechanism

2. **Step 3**: Complete Agent (Rust) implementation
   - gRPC service endpoints
   - Wasmtime integration
   - Storage and container management

3. **Step 4**: Integration testing
   - Master failover tests
   - Resource allocation tests
   - External API mocking

## Useful Commands

```bash
# Build
go build -o bin/capsuled-client ./cmd/client

# Test
go test ./...

# Format code
go fmt ./...

# Lint (requires golangci-lint)
golangci-lint run

# Clean
rm -rf bin/
go clean -cache

# Check rqlite status
curl http://localhost:4001/status | jq

# View rqlite cluster info
curl http://localhost:4001/nodes | jq
```

## Support

For issues or questions:
- Check `implementation_plan.md` for design details
- Check `MIGRATION_SUMMARY.md` for implementation status
- Review `pkg/db/` package documentation

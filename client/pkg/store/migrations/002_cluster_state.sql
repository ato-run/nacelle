-- Cluster State Schema
-- Added in v0.3.0 to replace rqlite-based cluster management

-- Local node configuration
CREATE TABLE IF NOT EXISTS local_node (
    id TEXT PRIMARY KEY DEFAULT 'self',
    node_id TEXT NOT NULL,
    hostname TEXT NOT NULL,
    tailnet_ip TEXT,
    is_online INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Known nodes in the cluster (cached from discovery)
CREATE TABLE IF NOT EXISTS known_nodes (
    id TEXT PRIMARY KEY,
    hostname TEXT NOT NULL,
    tailnet_ip TEXT,
    status TEXT NOT NULL DEFAULT 'unknown',
    last_seen INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Index for node status
CREATE INDEX IF NOT EXISTS idx_known_nodes_status ON known_nodes(status);

-- Router decisions log (for debugging and analytics)
CREATE TABLE IF NOT EXISTS route_decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    capsule_name TEXT NOT NULL,
    decision TEXT NOT NULL, -- 'local' or 'cloud'
    reason TEXT,
    vram_usage_percent REAL,
    timestamp INTEGER NOT NULL
);

-- Index for route decision queries
CREATE INDEX IF NOT EXISTS idx_route_decisions_timestamp ON route_decisions(timestamp);
CREATE INDEX IF NOT EXISTS idx_route_decisions_capsule ON route_decisions(capsule_name);

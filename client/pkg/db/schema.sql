-- Schema for Capsuled Coordinator State in rqlite
-- This schema defines the cluster-wide state managed by the Coordinator

-- Cluster nodes table - tracks all known nodes in the cluster
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,              -- Node unique identifier (ULID)
    address TEXT NOT NULL,            -- Node network address (IP:PORT)
    headscale_name TEXT NOT NULL,     -- Node name in headscale
    status TEXT NOT NULL,             -- Node status: 'active', 'inactive', 'failed'
    is_master INTEGER NOT NULL DEFAULT 0, -- 1 if this is the current master, 0 otherwise
    last_seen INTEGER NOT NULL,       -- Unix timestamp of last heartbeat
    created_at INTEGER NOT NULL,      -- Unix timestamp of node registration
    updated_at INTEGER NOT NULL       -- Unix timestamp of last update
);

-- Create index on status for faster queries
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);
CREATE INDEX IF NOT EXISTS idx_nodes_is_master ON nodes(is_master);

-- Capsules table - tracks all deployed capsules across the cluster
CREATE TABLE IF NOT EXISTS capsules (
    id TEXT PRIMARY KEY,              -- Capsule unique identifier (ULID)
    name TEXT NOT NULL,               -- Human-readable capsule name
    node_id TEXT NOT NULL,            -- Node where this capsule is deployed
    manifest TEXT NOT NULL,           -- JSON manifest of the capsule (adep.json)
    status TEXT NOT NULL,             -- Capsule status: 'pending', 'running', 'stopped', 'failed'
    storage_path TEXT,                -- Path to capsule storage on node
    bundle_path TEXT,                 -- Path to OCI bundle on node
    network_config TEXT,              -- JSON network configuration
    created_at INTEGER NOT NULL,      -- Unix timestamp of capsule creation
    updated_at INTEGER NOT NULL,      -- Unix timestamp of last status update
    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Create indexes for faster queries
CREATE INDEX IF NOT EXISTS idx_capsules_node_id ON capsules(node_id);
CREATE INDEX IF NOT EXISTS idx_capsules_status ON capsules(status);
CREATE INDEX IF NOT EXISTS idx_capsules_name ON capsules(name);

-- Resources table - tracks resource allocations per node
CREATE TABLE IF NOT EXISTS node_resources (
    node_id TEXT PRIMARY KEY,         -- Node ID (same as nodes.id)
    cpu_total INTEGER NOT NULL,       -- Total CPU cores (millicores)
    cpu_allocated INTEGER NOT NULL DEFAULT 0, -- Allocated CPU cores
    memory_total INTEGER NOT NULL,    -- Total memory in bytes
    memory_allocated INTEGER NOT NULL DEFAULT 0, -- Allocated memory in bytes
    storage_total INTEGER NOT NULL,   -- Total storage in bytes
    storage_allocated INTEGER NOT NULL DEFAULT 0, -- Allocated storage in bytes
    updated_at INTEGER NOT NULL,      -- Unix timestamp of last update
    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Capsule resources table - tracks resource usage per capsule
CREATE TABLE IF NOT EXISTS capsule_resources (
    capsule_id TEXT PRIMARY KEY,      -- Capsule ID (same as capsules.id)
    cpu_request INTEGER NOT NULL,     -- Requested CPU cores (millicores)
    memory_request INTEGER NOT NULL,  -- Requested memory in bytes
    storage_request INTEGER NOT NULL, -- Requested storage in bytes
    FOREIGN KEY (capsule_id) REFERENCES capsules(id) ON DELETE CASCADE
);

-- Master election history - tracks master election events
CREATE TABLE IF NOT EXISTS master_elections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id TEXT NOT NULL,            -- Node that became master
    elected_at INTEGER NOT NULL,      -- Unix timestamp of election
    reason TEXT NOT NULL,             -- Reason for election: 'startup', 'failover', 'manual'
    quorum_size INTEGER NOT NULL,     -- Number of nodes in quorum at election time
    FOREIGN KEY (node_id) REFERENCES nodes(id)
);

-- Create index on election time for history queries
CREATE INDEX IF NOT EXISTS idx_master_elections_elected_at ON master_elections(elected_at);

-- Cluster metadata - stores cluster-wide configuration and state
CREATE TABLE IF NOT EXISTS cluster_metadata (
    key TEXT PRIMARY KEY,             -- Configuration key
    value TEXT NOT NULL,              -- Configuration value (JSON for complex types)
    updated_at INTEGER NOT NULL       -- Unix timestamp of last update
);

-- Insert initial cluster metadata
INSERT OR IGNORE INTO cluster_metadata (key, value, updated_at)
VALUES
    ('cluster_version', '1.0.0', strftime('%s', 'now')),
    ('cluster_name', 'capsuled-cluster', strftime('%s', 'now')),
    ('initialized_at', strftime('%s', 'now'), strftime('%s', 'now'));

-- Schema for Capsuled Coordinator State in rqlite
-- This schema defines the cluster-wide state managed by the Coordinator

-- Cluster nodes table - tracks all known nodes in the cluster
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,              -- Node unique identifier (ULID)
    address TEXT NOT NULL,            -- Node network address (IP:PORT)
    headscale_name TEXT NOT NULL,     -- Node name in headscale
    tailnet_ip TEXT,                  -- Node Tailnet IP address
    status TEXT NOT NULL,             -- Node status: 'active', 'inactive', 'failed'
    is_master INTEGER NOT NULL DEFAULT 0, -- 1 if this is the current master, 0 otherwise
    last_seen INTEGER NOT NULL,       -- Unix timestamp of last heartbeat
    created_at INTEGER NOT NULL,      -- Unix timestamp of node registration
    updated_at INTEGER NOT NULL       -- Unix timestamp of last update
);

-- Create index on status for faster queries
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);
CREATE INDEX IF NOT EXISTS idx_nodes_is_master ON nodes(is_master);
CREATE INDEX IF NOT EXISTS idx_nodes_tailnet_ip ON nodes(tailnet_ip);

-- Capsules table - tracks all deployed capsules across the cluster
CREATE TABLE IF NOT EXISTS capsules (
    id TEXT PRIMARY KEY,              -- Capsule unique identifier (ULID)
    name TEXT NOT NULL,               -- Human-readable capsule name
    node_id TEXT NOT NULL,            -- Node where this capsule is deployed
    runtime_name TEXT,                -- Runtime name
    manifest TEXT NOT NULL,           -- JSON manifest of the capsule (adep.json)
    status TEXT NOT NULL,             -- Capsule status: 'pending', 'running', 'stopped', 'failed'
    port INTEGER,                     -- Exposed port
    access_url TEXT,                  -- Access URL
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

-- Create node_gpus table to track individual GPU resources
CREATE TABLE IF NOT EXISTS node_gpus (
    id TEXT PRIMARY KEY,              -- GPU UUID
    node_id TEXT NOT NULL,            -- Node ID
    gpu_index INTEGER NOT NULL,       -- GPU index (0-based)
    name TEXT NOT NULL,               -- GPU name
    total_vram_bytes INTEGER NOT NULL,
    used_vram_bytes INTEGER NOT NULL DEFAULT 0, -- Tracked usage
    updated_at INTEGER NOT NULL,      -- Unix timestamp of last update
    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

-- Create index for faster lookups by node
CREATE INDEX IF NOT EXISTS idx_node_gpus_node_id ON node_gpus(node_id);

-- Node workloads table - tracks workloads reported by agents
CREATE TABLE IF NOT EXISTS node_workloads (
    node_id TEXT NOT NULL,            -- Node ID
    workload_id TEXT NOT NULL,        -- Workload ID
    name TEXT,                        -- Workload name
    reserved_vram_bytes INTEGER,      -- Reserved VRAM
    observed_vram_bytes INTEGER,      -- Observed VRAM usage
    pid INTEGER,                      -- Process ID
    phase TEXT,                       -- Workload phase
    updated_at INTEGER NOT NULL,      -- Unix timestamp of last update
    PRIMARY KEY (node_id, workload_id),
    FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_node_workloads_node_id ON node_workloads(node_id);

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
    ('cluster_version', '1.0.0', strftime('%s', 'now')),
    ('cluster_name', 'capsuled-cluster', strftime('%s', 'now')),
    ('initialized_at', strftime('%s', 'now'), strftime('%s', 'now'));

-- Migration: Add tailnet_ip to nodes
-- Note: SQLite ignores ADD COLUMN if it already exists (in newer versions) or throws error.
-- For safety in this script, we assume this might be run on existing DB.
-- However, standard SQL scripts usually don't mix CREATE and ALTER for the same table unless for migration.
-- I will add it as a separate block.

-- Add tailnet_ip column if not exists (requires application logic to handle "if not exists" or just ignore error)
-- ALTER TABLE nodes ADD COLUMN tailnet_ip TEXT;
-- CREATE INDEX IF NOT EXISTS idx_nodes_tailnet_ip ON nodes(tailnet_ip);

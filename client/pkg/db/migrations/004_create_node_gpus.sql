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

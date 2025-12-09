-- Capsule Registry Schema
-- Replaces rqlite in Gumball v0.3.0

-- Installed capsules table
CREATE TABLE IF NOT EXISTS capsules (
    id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    type TEXT NOT NULL DEFAULT 'inference',
    manifest_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'stopped',
    installed_at INTEGER NOT NULL,
    last_used_at INTEGER
);

-- Index for status queries
CREATE INDEX IF NOT EXISTS idx_capsules_status ON capsules(status);

-- Running processes table
CREATE TABLE IF NOT EXISTS processes (
    capsule_id TEXT PRIMARY KEY,
    pid INTEGER NOT NULL,
    port INTEGER,
    started_at INTEGER NOT NULL,
    FOREIGN KEY (capsule_id) REFERENCES capsules(id) ON DELETE CASCADE
);

-- Hardware snapshots for monitoring
CREATE TABLE IF NOT EXISTS hardware_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    total_vram_gb REAL NOT NULL,
    available_vram_gb REAL NOT NULL,
    total_ram_gb REAL NOT NULL,
    available_ram_gb REAL NOT NULL,
    cpu_usage_percent REAL NOT NULL
);

-- Index for time-based queries
CREATE INDEX IF NOT EXISTS idx_hardware_timestamp ON hardware_snapshots(timestamp);

-- Deployed Capsules Schema
-- Added for Phase 4-C: Coordinator state persistence

-- Tracks capsules deployed via the Coordinator
CREATE TABLE IF NOT EXISTS deployed_capsules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    port INTEGER,
    created_at INTEGER NOT NULL
);

-- Index for status queries
CREATE INDEX IF NOT EXISTS idx_deployed_capsules_status ON deployed_capsules(status);

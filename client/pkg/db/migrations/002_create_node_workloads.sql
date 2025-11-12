-- Migration: create node_workloads table to track Agent-observed workloads
-- Phase 5: Reconciliation loop groundwork
-- Date: 2025-11-11

CREATE TABLE IF NOT EXISTS node_workloads (
    node_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    name TEXT NOT NULL,
    reserved_vram_bytes INTEGER NOT NULL DEFAULT 0,
    phase TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(node_id, workload_id),
    FOREIGN KEY(node_id) REFERENCES nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_node_workloads_phase ON node_workloads(phase);
CREATE INDEX IF NOT EXISTS idx_node_workloads_updated_at ON node_workloads(updated_at);

INSERT OR REPLACE INTO cluster_metadata (key, value, updated_at)
VALUES ('node_workloads_schema_version', '002', strftime('%s', 'now'));

-- Migration: Add GPU hardware tracking columns to nodes table
-- Week 3: Coordinator GPU-Aware Infrastructure
-- Date: 2025-11-11

-- Add GPU-related columns to existing nodes table
-- These columns track hardware capabilities reported by Agents (Week 1)

ALTER TABLE nodes ADD COLUMN total_vram_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE nodes ADD COLUMN used_vram_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE nodes ADD COLUMN cuda_driver_version TEXT DEFAULT '';

-- Create index for GPU-capable nodes query
-- Used by scheduler to quickly find nodes with available VRAM
CREATE INDEX IF NOT EXISTS idx_nodes_gpu_available ON nodes(total_vram_bytes, used_vram_bytes) WHERE total_vram_bytes > 0;

-- Create index for VRAM availability sorting
-- Helps scheduler find nodes with most available VRAM first (BestFit strategy)
CREATE INDEX IF NOT EXISTS idx_nodes_vram_free ON nodes((total_vram_bytes - used_vram_bytes)) WHERE total_vram_bytes > 0;

-- Update cluster metadata to track migration
INSERT OR REPLACE INTO cluster_metadata (key, value, updated_at)
VALUES ('gpu_support_enabled', 'true', strftime('%s', 'now'));

INSERT OR REPLACE INTO cluster_metadata (key, value, updated_at)
VALUES ('gpu_migration_version', '001', strftime('%s', 'now'));

ALTER TABLE node_workloads ADD COLUMN observed_vram_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE node_workloads ADD COLUMN pid INTEGER;

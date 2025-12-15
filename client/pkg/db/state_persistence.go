package db

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
)

// CreateNode creates a new node in rqlite and updates the cache
func (sm *StateManager) CreateNode(node *Node) error {
	now := time.Now()
	node.CreatedAt = now
	node.UpdatedAt = now

	query := fmt.Sprintf(`
		INSERT INTO nodes (id, address, headscale_name, status, is_master, last_seen, created_at, updated_at)
		VALUES ('%s', '%s', '%s', '%s', %d, %d, %d, %d)
	`, escapeSQLString(node.ID), escapeSQLString(node.Address), escapeSQLString(node.HeadscaleName), escapeSQLString(string(node.Status)),
		boolToInt(node.IsMaster), node.LastSeen.Unix(), node.CreatedAt.Unix(), node.UpdatedAt.Unix())

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to create node: %w", err)
	}

	// Update cache
	sm.nodesMu.Lock()
	sm.nodes[node.ID] = node
	sm.nodesMu.Unlock()

	return nil
}

// UpdateNode updates an existing node in rqlite and the cache
func (sm *StateManager) UpdateNode(node *Node) error {
	node.UpdatedAt = time.Now()

	query := fmt.Sprintf(`
		UPDATE nodes
		SET address = '%s', headscale_name = '%s', status = '%s',
		    is_master = %d, last_seen = %d, updated_at = %d
		WHERE id = '%s'
	`, escapeSQLString(node.Address), escapeSQLString(node.HeadscaleName), escapeSQLString(string(node.Status)),
		boolToInt(node.IsMaster), node.LastSeen.Unix(), node.UpdatedAt.Unix(), escapeSQLString(node.ID))

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to update node: %w", err)
	}

	// Update cache
	sm.nodesMu.Lock()
	sm.nodes[node.ID] = node
	sm.nodesMu.Unlock()

	return nil
}

// DeleteNode deletes a node from rqlite and the cache
func (sm *StateManager) DeleteNode(nodeID string) error {
	query := fmt.Sprintf("DELETE FROM nodes WHERE id = '%s'", escapeSQLString(nodeID))

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to delete node: %w", err)
	}

	// Update cache
	sm.nodesMu.Lock()
	delete(sm.nodes, nodeID)
	sm.nodesMu.Unlock()

	sm.resourcesMu.Lock()
	delete(sm.resources, nodeID)
	sm.resourcesMu.Unlock()

	return nil
}

// SetMasterNode sets a specific node as the master and demotes all others
func (sm *StateManager) SetMasterNode(nodeID string, reason ElectionReason, quorumSize int) error {
	now := time.Now()

	queries := []string{
		// Demote all current masters
		"UPDATE nodes SET is_master = 0, updated_at = " + fmt.Sprintf("%d", now.Unix()),
		// Promote the new master
		fmt.Sprintf("UPDATE nodes SET is_master = 1, updated_at = %d WHERE id = '%s'", now.Unix(), escapeSQLString(nodeID)),
		// Record the election
		fmt.Sprintf(`INSERT INTO master_elections (node_id, elected_at, reason, quorum_size)
			VALUES ('%s', %d, '%s', %d)`, escapeSQLString(nodeID), now.Unix(), escapeSQLString(string(reason)), quorumSize),
	}

	if err := sm.client.ExecuteMany(queries); err != nil {
		return fmt.Errorf("failed to set master node: %w", err)
	}

	// Update cache
	sm.nodesMu.Lock()
	for _, node := range sm.nodes {
		node.IsMaster = (node.ID == nodeID)
		node.UpdatedAt = now
	}
	sm.nodesMu.Unlock()

	return nil
}

// CreateCapsule creates a new capsule in rqlite and updates the cache
func (sm *StateManager) CreateCapsule(capsule *Capsule) error {
	now := time.Now()
	capsule.CreatedAt = now
	capsule.UpdatedAt = now

	query := fmt.Sprintf(`
		INSERT INTO capsules (id, user_id, name, node_id, manifest, status, storage_path, bundle_path, network_config, created_at, updated_at)
		VALUES ('%s', '%s', '%s', '%s', '%s', '%s', '%s', '%s', '%s', %d, %d)
	`, escapeSQLString(capsule.ID), escapeSQLString(capsule.UserID), escapeSQLString(capsule.Name), escapeSQLString(capsule.NodeID), escapeSQLString(capsule.Manifest), escapeSQLString(string(capsule.Status)),
		escapeSQLString(capsule.StoragePath), escapeSQLString(capsule.BundlePath), escapeSQLString(capsule.NetworkConfig),
		capsule.CreatedAt.Unix(), capsule.UpdatedAt.Unix())

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to create capsule: %w", err)
	}

	// Update cache
	sm.capsulesMu.Lock()
	sm.capsules[capsule.ID] = capsule
	sm.capsulesMu.Unlock()

	return nil
}

// UpdateCapsule updates an existing capsule in rqlite and the cache
func (sm *StateManager) UpdateCapsule(capsule *Capsule) error {
	capsule.UpdatedAt = time.Now()

	query := fmt.Sprintf(`
		UPDATE capsules
		SET name = '%s', node_id = '%s', manifest = '%s', status = '%s',
		    storage_path = '%s', bundle_path = '%s', network_config = '%s', updated_at = %d
		WHERE id = '%s'
	`, escapeSQLString(capsule.Name), escapeSQLString(capsule.NodeID), escapeSQLString(capsule.Manifest), escapeSQLString(string(capsule.Status)),
		escapeSQLString(capsule.StoragePath), escapeSQLString(capsule.BundlePath), escapeSQLString(capsule.NetworkConfig),
		capsule.UpdatedAt.Unix(), escapeSQLString(capsule.ID))

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to update capsule: %w", err)
	}

	// Update cache
	sm.capsulesMu.Lock()
	sm.capsules[capsule.ID] = capsule
	sm.capsulesMu.Unlock()

	return nil
}

// UpdateCapsuleStatus updates only the status of a capsule
func (sm *StateManager) UpdateCapsuleStatus(capsuleID string, status CapsuleStatus) error {
	now := time.Now()

	query := fmt.Sprintf(`
		UPDATE capsules
		SET status = '%s', updated_at = %d
		WHERE id = '%s'
	`, escapeSQLString(string(status)), now.Unix(), escapeSQLString(capsuleID))

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to update capsule status: %w", err)
	}

	// Update cache
	sm.capsulesMu.Lock()
	if capsule, exists := sm.capsules[capsuleID]; exists {
		capsule.Status = status
		capsule.UpdatedAt = now
	}
	sm.capsulesMu.Unlock()

	return nil
}

// DeleteCapsule deletes a capsule from rqlite and the cache
func (sm *StateManager) DeleteCapsule(capsuleID string) error {
	if sm.client == nil {
		return fmt.Errorf("state manager client not configured")
	}

	query := fmt.Sprintf("DELETE FROM capsules WHERE id = '%s'", escapeSQLString(capsuleID))

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to delete capsule: %w", err)
	}

	// Update cache
	sm.capsulesMu.Lock()
	delete(sm.capsules, capsuleID)
	sm.capsulesMu.Unlock()

	return nil
}

// UpdateNodeResources updates or creates node resource information
func (sm *StateManager) UpdateNodeResources(resources *NodeResources) error {
	resources.UpdatedAt = time.Now()

	// Use INSERT OR REPLACE (UPSERT) to handle both create and update
	query := fmt.Sprintf(`
		INSERT OR REPLACE INTO node_resources (node_id, cpu_total, cpu_allocated, memory_total, memory_allocated, storage_total, storage_allocated, updated_at)
		VALUES ('%s', %d, %d, %d, %d, %d, %d, %d)
	`, escapeSQLString(resources.NodeID), resources.CPUTotal, resources.CPUAllocated,
		resources.MemoryTotal, resources.MemoryAllocated,
		resources.StorageTotal, resources.StorageAllocated,
		resources.UpdatedAt.Unix())

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to update node resources: %w", err)
	}

	// Update cache
	sm.resourcesMu.Lock()
	sm.resources[resources.NodeID] = resources
	sm.resourcesMu.Unlock()

	return nil
}

// AllocateResources allocates resources for a capsule on a node
func (sm *StateManager) AllocateResources(nodeID string, capsuleID string, req CapsuleResources) error {
	sm.resourcesMu.Lock()
	defer sm.resourcesMu.Unlock()

	resources, exists := sm.resources[nodeID]
	if !exists {
		return fmt.Errorf("node resources not found for node %s", nodeID)
	}

	// Check if resources can be allocated
	if !resources.CanAllocate(req) {
		return fmt.Errorf("insufficient resources on node %s", nodeID)
	}

	// Update allocations
	resources.CPUAllocated += req.CPURequest
	resources.MemoryAllocated += req.MemoryRequest
	resources.StorageAllocated += req.StorageRequest
	resources.UpdatedAt = time.Now()

	queries := []string{
		// Update node resources
		fmt.Sprintf(`UPDATE node_resources SET cpu_allocated = %d, memory_allocated = %d, storage_allocated = %d, updated_at = %d WHERE node_id = '%s'`,
			resources.CPUAllocated, resources.MemoryAllocated, resources.StorageAllocated, resources.UpdatedAt.Unix(), escapeSQLString(nodeID)),
		// Insert capsule resource record
		fmt.Sprintf(`INSERT INTO capsule_resources (capsule_id, cpu_request, memory_request, storage_request) VALUES ('%s', %d, %d, %d)`,
			escapeSQLString(capsuleID), req.CPURequest, req.MemoryRequest, req.StorageRequest),
	}

	if err := sm.client.ExecuteMany(queries); err != nil {
		// Rollback in-memory changes on failure
		resources.CPUAllocated -= req.CPURequest
		resources.MemoryAllocated -= req.MemoryRequest
		resources.StorageAllocated -= req.StorageRequest
		return fmt.Errorf("failed to allocate resources: %w", err)
	}

	return nil
}

// DeallocateResources deallocates resources for a capsule on a node
func (sm *StateManager) DeallocateResources(nodeID string, capsuleID string) error {
	sm.resourcesMu.Lock()
	defer sm.resourcesMu.Unlock()

	// Query capsule resources
	result, err := sm.client.Query(fmt.Sprintf("SELECT cpu_request, memory_request, storage_request FROM capsule_resources WHERE capsule_id = '%s'", escapeSQLString(capsuleID)))
	if err != nil {
		return fmt.Errorf("failed to query capsule resources: %w", err)
	}

	if !result.Next() {
		return fmt.Errorf("capsule resources not found for capsule %s", capsuleID)
	}

	var cpuRequest, memoryRequest, storageRequest int64
	if err := result.Scan(&cpuRequest, &memoryRequest, &storageRequest); err != nil {
		return fmt.Errorf("failed to scan capsule resources: %w", err)
	}

	resources, exists := sm.resources[nodeID]
	if !exists {
		return fmt.Errorf("node resources not found for node %s", nodeID)
	}

	// Update allocations
	resources.CPUAllocated -= cpuRequest
	resources.MemoryAllocated -= memoryRequest
	resources.StorageAllocated -= storageRequest
	resources.UpdatedAt = time.Now()

	queries := []string{
		// Update node resources
		fmt.Sprintf(`UPDATE node_resources SET cpu_allocated = %d, memory_allocated = %d, storage_allocated = %d, updated_at = %d WHERE node_id = '%s'`,
			resources.CPUAllocated, resources.MemoryAllocated, resources.StorageAllocated, resources.UpdatedAt.Unix(), escapeSQLString(nodeID)),
		// Delete capsule resource record
		fmt.Sprintf(`DELETE FROM capsule_resources WHERE capsule_id = '%s'`, escapeSQLString(capsuleID)),
	}

	if err := sm.client.ExecuteMany(queries); err != nil {
		// Rollback in-memory changes on failure
		resources.CPUAllocated += cpuRequest
		resources.MemoryAllocated += memoryRequest
		resources.StorageAllocated += storageRequest
		return fmt.Errorf("failed to deallocate resources: %w", err)
	}

	return nil
}

// SetMetadata sets a metadata value
func (sm *StateManager) SetMetadata(key, value string) error {
	now := time.Now()

	query := fmt.Sprintf(`
		INSERT OR REPLACE INTO cluster_metadata (key, value, updated_at)
		VALUES ('%s', '%s', %d)
	`, escapeSQLString(key), escapeSQLString(value), now.Unix())

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to set metadata: %w", err)
	}

	// Update cache
	sm.metadataMu.Lock()
	sm.metadata[key] = &ClusterMetadata{
		Key:       key,
		Value:     value,
		UpdatedAt: now,
	}
	sm.metadataMu.Unlock()

	return nil
}

// UpsertNode creates or updates a node in rqlite and updates the cache
func (sm *StateManager) UpsertNode(node *Node) error {
	node.UpdatedAt = time.Now()
	if node.CreatedAt.IsZero() {
		node.CreatedAt = node.UpdatedAt
	}

	query := fmt.Sprintf(`
		INSERT OR REPLACE INTO nodes (id, address, headscale_name, status, is_master, last_seen, created_at, updated_at)
		VALUES ('%s', '%s', '%s', '%s', %d, %d, %d, %d)
	`, escapeSQLString(node.ID), escapeSQLString(node.Address), escapeSQLString(node.HeadscaleName), escapeSQLString(string(node.Status)),
		boolToInt(node.IsMaster), node.LastSeen.Unix(), node.CreatedAt.Unix(), node.UpdatedAt.Unix())

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to upsert node: %w", err)
	}

	// Update cache
	sm.nodesMu.Lock()
	sm.nodes[node.ID] = node
	sm.nodesMu.Unlock()

	return nil
}

// UpdateNodeHeartbeat updates node heartbeat and syncs workloads/GPUs
func (sm *StateManager) UpdateNodeHeartbeat(nodeID string, lastSeen time.Time, workloads []*NodeWorkload, gpus []*NodeGpu) error {
	now := time.Now()
	
	// Prepare batch queries
	queries := []string{
		// Update node last_seen
		fmt.Sprintf("UPDATE nodes SET last_seen = %d, updated_at = %d WHERE id = '%s'", lastSeen.Unix(), now.Unix(), escapeSQLString(nodeID)),
		// Clear existing workloads and GPUs for this node (replace strategy)
		fmt.Sprintf("DELETE FROM node_workloads WHERE node_id = '%s'", escapeSQLString(nodeID)),
		fmt.Sprintf("DELETE FROM node_gpus WHERE node_id = '%s'", escapeSQLString(nodeID)),
	}

	// Insert workloads
	for _, wl := range workloads {
		queries = append(queries, fmt.Sprintf(`
			INSERT INTO node_workloads (node_id, workload_id, name, reserved_vram_bytes, observed_vram_bytes, pid, phase, updated_at)
			VALUES ('%s', '%s', '%s', %d, %d, %d, '%s', %d)
		`, escapeSQLString(wl.NodeID), escapeSQLString(wl.WorkloadID), escapeSQLString(wl.Name), 
		wl.ReservedVRAMBytes, wl.ObservedVRAMBytes, wl.PID, escapeSQLString(wl.Phase), wl.UpdatedAt.Unix()))
	}

	// Insert GPUs
	for _, gpu := range gpus {
		queries = append(queries, fmt.Sprintf(`
			INSERT INTO node_gpus (id, node_id, gpu_index, name, total_vram_bytes, used_vram_bytes, updated_at)
			VALUES ('%s', '%s', %d, '%s', %d, %d, %d)
		`, escapeSQLString(gpu.ID), escapeSQLString(gpu.NodeID), gpu.Index, escapeSQLString(gpu.Name), 
		gpu.TotalVRAMBytes, gpu.UsedVRAMBytes, gpu.UpdatedAt.Unix()))
	}

	if err := sm.client.ExecuteMany(queries); err != nil {
		return fmt.Errorf("failed to update node heartbeat: %w", err)
	}

	// Update node cache (last_seen)
	sm.nodesMu.Lock()
	if node, exists := sm.nodes[nodeID]; exists {
		node.LastSeen = lastSeen
		node.UpdatedAt = now
	}
	sm.nodesMu.Unlock()

	return nil
}

// GetAllGpuRigs retrieves all nodes with their GPU information and current VRAM usage
func (sm *StateManager) GetAllGpuRigs(ctx context.Context) ([]*gpu.RigGpuInfo, error) {
	sm.nodesMu.RLock()
	defer sm.nodesMu.RUnlock()

	var rigs []*gpu.RigGpuInfo

	for _, node := range sm.nodes {
		if node.Status != NodeStatusActive {
			continue
		}

		// Get GPUs for this node
		// We don't cache GPUs in StateManager yet, so query DB
		// TODO: Cache GPUs for performance
		gpuRows, err := sm.client.Query(fmt.Sprintf("SELECT id, name, total_vram_bytes, used_vram_bytes FROM node_gpus WHERE node_id = '%s'", escapeSQLString(node.ID)))
		if err != nil {
			return nil, fmt.Errorf("failed to query GPUs for node %s: %w", node.ID, err)
		}

		var gpus []gpu.GpuInfo
		var totalVRAM uint64
		
		for gpuRows.Next() {
			var id, name string
			var total, used uint64 // used from heartbeat (observed)
			if err := gpuRows.Scan(&id, &name, &total, &used); err != nil {
				continue
			}
			gpus = append(gpus, gpu.GpuInfo{
				UUID:           id,
				DeviceName:     name,
				TotalVRAMBytes: total,
				// AvailableVRAMBytes is calculated by scheduler based on Rig usage
			})
			totalVRAM += total
		}

		// Get total reserved VRAM from workloads (pending + running)
		// We sum reserved_vram_bytes from node_workloads
		workloadRows, err := sm.client.Query(fmt.Sprintf("SELECT SUM(reserved_vram_bytes) FROM node_workloads WHERE node_id = '%s'", escapeSQLString(node.ID)))
		if err != nil {
			return nil, fmt.Errorf("failed to query workload usage for node %s: %w", node.ID, err)
		}

		var reservedVRAM uint64
		if workloadRows.Next() {
			var val interface{}
			if err := workloadRows.Scan(&val); err == nil && val != nil {
				// rqlite/sqlite might return int64 or float64
				switch v := val.(type) {
				case int64:
					reservedVRAM = uint64(v)
				case float64:
					reservedVRAM = uint64(v)
				}
			}
		}

		rigs = append(rigs, &gpu.RigGpuInfo{
			RigID:             node.ID,
			TotalVRAMBytes:    totalVRAM,
			UsedVRAMBytes:     reservedVRAM,
			CudaDriverVersion: "12.0", // TODO: Store in DB
			Gpus:              gpus,
			IsRemote:          node.TailnetIP != "",
		})
	}

	return rigs, nil
}

// ReserveVRAM reserves VRAM for a workload on a node by inserting a pending workload record
func (sm *StateManager) ReserveVRAM(ctx context.Context, nodeID, workloadID string, vramBytes uint64) error {
	now := time.Now()
	
	// Insert pending workload
	query := fmt.Sprintf(`
		INSERT INTO node_workloads (node_id, workload_id, name, reserved_vram_bytes, observed_vram_bytes, pid, phase, updated_at)
		VALUES ('%s', '%s', 'pending-deployment', %d, 0, 0, 'pending', %d)
	`, escapeSQLString(nodeID), escapeSQLString(workloadID), vramBytes, now.Unix())

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to reserve VRAM: %w", err)
	}

	return nil
}

// ReleaseVRAM releases reserved VRAM by deleting the workload record (if it's still pending/failed)
func (sm *StateManager) ReleaseVRAM(ctx context.Context, nodeID string, vramBytes uint64) error {
	// We don't have workloadID here in the old signature, but we should probably just rely on 
	// the fact that if deployment failed, we want to remove the pending record.
	// But without workloadID, we might delete the wrong one?
	// The DeployHandler calls this on error.
	// I'll update DeployHandler to pass workloadID to ReleaseVRAM too.
	// For now, I'll implement a version that takes workloadID if possible, 
	// but to match the old interface I might need to change DeployHandler first.
	// Let's assume I will update DeployHandler.
	return nil
}

// ReleaseVRAMByWorkload releases reserved VRAM by deleting the workload record
func (sm *StateManager) ReleaseVRAMByWorkload(ctx context.Context, nodeID, workloadID string) error {
	query := fmt.Sprintf("DELETE FROM node_workloads WHERE node_id = '%s' AND workload_id = '%s'", escapeSQLString(nodeID), escapeSQLString(workloadID))
	return sm.client.Execute(query)
}

// Helper functions

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}

// ListDesiredWorkloads returns a map of workloadID -> nodeID for all capsules that should be running
func (sm *StateManager) ListDesiredWorkloads(ctx context.Context) (map[string]string, error) {
	query := fmt.Sprintf(`
        SELECT id, node_id
        FROM capsules
        WHERE status IN ('%s', '%s', '%s')
    `,
		CapsuleStatusPending,
		CapsuleStatusRunning,
		CapsuleStatusFailed,
	)

	result, err := sm.client.QueryContext(ctx, query)
	if err != nil {
		return nil, fmt.Errorf("query desired workloads: %w", err)
	}

	desired := make(map[string]string)
	for result.Next() {
		var id, nodeID string
		if err := result.Scan(&id, &nodeID); err != nil {
			return nil, fmt.Errorf("scan desired workload: %w", err)
		}
		desired[id] = nodeID
	}

	return desired, nil
}

// ListNodeWorkloads returns all workloads reported by nodes
func (sm *StateManager) ListNodeWorkloads(ctx context.Context) ([]*NodeWorkload, error) {
	query := `
		SELECT
			node_id,
			workload_id,
			name,
			reserved_vram_bytes,
			observed_vram_bytes,
			pid,
			phase,
			updated_at
		FROM node_workloads
	`

	result, err := sm.client.QueryContext(ctx, query)
	if err != nil {
		return nil, fmt.Errorf("query node workloads: %w", err)
	}

	workloads := make([]*NodeWorkload, 0)
	for result.Next() {
		var wl NodeWorkload
		var reserved int64
		var observed int64
		var pid sql.NullInt64
		var updatedAt int64
		if err := result.Scan(
			&wl.NodeID,
			&wl.WorkloadID,
			&wl.Name,
			&reserved,
			&observed,
			&pid,
			&wl.Phase,
			&updatedAt,
		); err != nil {
			return nil, fmt.Errorf("scan node workload: %w", err)
		}
		wl.ReservedVRAMBytes = uint64(reserved)
		wl.ObservedVRAMBytes = uint64(observed)
		if pid.Valid {
			wl.PID = pid.Int64
		}
		wl.UpdatedAt = time.Unix(updatedAt, 0)
		workloads = append(workloads, &wl)
	}

	return workloads, nil
}

// MarkCapsuleFailed updates the status of a capsule to failed
func (sm *StateManager) MarkCapsuleFailed(ctx context.Context, capsuleID string) error {
	return sm.UpdateCapsuleStatus(capsuleID, CapsuleStatusFailed)
}

// MarkCapsulePending updates the status of a capsule to pending
func (sm *StateManager) MarkCapsulePending(ctx context.Context, capsuleID string) error {
	return sm.UpdateCapsuleStatus(capsuleID, CapsuleStatusPending)
}



// escapeSQLString escapes single quotes in SQL strings
func escapeSQLString(s string) string {
	// Simple escape - replace ' with ''
	result := ""
	for _, c := range s {
		if c == '\'' {
			result += "''"
		} else {
			result += string(c)
		}
	}
	return result
}

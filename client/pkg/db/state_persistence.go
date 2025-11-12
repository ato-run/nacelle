package db

import (
	"fmt"
	"time"
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
		INSERT INTO capsules (id, name, node_id, manifest, status, storage_path, bundle_path, network_config, created_at, updated_at)
		VALUES ('%s', '%s', '%s', '%s', '%s', '%s', '%s', '%s', %d, %d)
	`, escapeSQLString(capsule.ID), escapeSQLString(capsule.Name), escapeSQLString(capsule.NodeID), escapeSQLString(capsule.Manifest), escapeSQLString(string(capsule.Status)),
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

// Helper functions

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
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

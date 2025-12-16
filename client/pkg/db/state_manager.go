package db

import (
	"encoding/json"
	"fmt"
	"log"
	"sort"
	"sync"
	"time"
)

// StateManager manages the cluster state with in-memory caching
type StateManager struct {
	client *Client

	// In-memory caches for fast reads
	nodes     map[string]*Node            // Key: node ID
	capsules  map[string]*Capsule         // Key: capsule ID
	resources map[string]*NodeResources   // Key: node ID
	metadata  map[string]*ClusterMetadata // Key: metadata key
	apps      map[string]*App             // Key: app ID

	// Mutexes for thread-safe access
	nodesMu     sync.RWMutex
	capsulesMu  sync.RWMutex
	resourcesMu sync.RWMutex
	metadataMu  sync.RWMutex
	appsMu      sync.RWMutex
}

// NewStateManager creates a new state manager with the given rqlite client
func NewStateManager(client *Client) *StateManager {
	return &StateManager{
		client:    client,
		nodes:     make(map[string]*Node),
		capsules:  make(map[string]*Capsule),
		resources: make(map[string]*NodeResources),
		metadata:  make(map[string]*ClusterMetadata),
		apps:      make(map[string]*App),
	}
}

// Initialize loads the complete cluster state from rqlite into memory
func (sm *StateManager) Initialize() error {
	log.Println("Initializing state manager: loading cluster state from rqlite...")

	// Load nodes
	if err := sm.loadNodes(); err != nil {
		return fmt.Errorf("failed to load nodes: %w", err)
	}

	// Load capsules
	if err := sm.loadCapsules(); err != nil {
		return fmt.Errorf("failed to load capsules: %w", err)
	}

	// Load resources
	if err := sm.loadResources(); err != nil {
		return fmt.Errorf("failed to load resources: %w", err)
	}

	// Load metadata
	if err := sm.loadMetadata(); err != nil {
		return fmt.Errorf("failed to load metadata: %w", err)
	}

	if err := sm.loadApps(); err != nil {
		return fmt.Errorf("failed to load apps: %w", err)
	}

	log.Printf("State manager initialized: %d nodes, %d capsules, %d resource entries",
		len(sm.nodes), len(sm.capsules), len(sm.resources))

	return nil
}

// loadNodes loads all nodes from rqlite into memory
func (sm *StateManager) loadNodes() error {
	if sm.client == nil {
		return fmt.Errorf("state manager client is nil")
	}
	result, err := sm.client.Query("SELECT id, address, headscale_name, status, is_master, last_seen, created_at, updated_at FROM nodes")
	if err != nil {
		return err
	}

	sm.nodesMu.Lock()
	defer sm.nodesMu.Unlock()

	// Clear existing cache
	sm.nodes = make(map[string]*Node)

	// Track errors for validation
	var errorCount int
	var successCount int
	const maxErrorThreshold = 10 // Max consecutive errors before aborting

	// Parse results
	for result.Next() {
		var node Node
		var isMaster int
		var lastSeenUnix, createdAtUnix, updatedAtUnix int64

		err := result.Scan(&node.ID, &node.Address, &node.HeadscaleName, &node.Status,
			&isMaster, &lastSeenUnix, &createdAtUnix, &updatedAtUnix)
		if err != nil {
			errorCount++
			log.Printf("Warning: failed to scan node row: %v", err)

			// If too many consecutive errors, abort to prevent data corruption
			if errorCount >= maxErrorThreshold && successCount == 0 {
				return fmt.Errorf("too many scan errors (%d), aborting node load", errorCount)
			}
			continue
		}

		successCount++
		node.IsMaster = isMaster == 1
		node.LastSeen = time.Unix(lastSeenUnix, 0)
		node.CreatedAt = time.Unix(createdAtUnix, 0)
		node.UpdatedAt = time.Unix(updatedAtUnix, 0)

		sm.nodes[node.ID] = &node
	}

	if errorCount > 0 {
		log.Printf("Loaded %d nodes with %d errors", successCount, errorCount)
	}

	return nil
}

// loadCapsules loads all capsules from rqlite into memory
func (sm *StateManager) loadCapsules() error {
	result, err := sm.client.Query("SELECT id, user_id, name, node_id, manifest, status, storage_path, bundle_path, network_config, created_at, updated_at FROM capsules")
	if err != nil {
		return err
	}

	sm.capsulesMu.Lock()
	defer sm.capsulesMu.Unlock()

	// Clear existing cache
	sm.capsules = make(map[string]*Capsule)

	// Track errors for validation
	var errorCount int
	var successCount int
	const maxErrorThreshold = 10

	// Parse results
	for result.Next() {
		var capsule Capsule
		var createdAtUnix, updatedAtUnix int64

		err := result.Scan(&capsule.ID, &capsule.UserID, &capsule.Name, &capsule.NodeID, &capsule.Manifest,
			&capsule.Status, &capsule.StoragePath, &capsule.BundlePath, &capsule.NetworkConfig,
			&createdAtUnix, &updatedAtUnix)
		if err != nil {
			errorCount++
			log.Printf("Warning: failed to scan capsule row: %v", err)

			if errorCount >= maxErrorThreshold && successCount == 0 {
				return fmt.Errorf("too many scan errors (%d), aborting capsule load", errorCount)
			}
			continue
		}

		successCount++
		capsule.CreatedAt = time.Unix(createdAtUnix, 0)
		capsule.UpdatedAt = time.Unix(updatedAtUnix, 0)

		sm.capsules[capsule.ID] = &capsule
	}

	if errorCount > 0 {
		log.Printf("Loaded %d capsules with %d errors", successCount, errorCount)
	}

	return nil
}

// loadResources loads all node resources from rqlite into memory
func (sm *StateManager) loadResources() error {
	result, err := sm.client.Query("SELECT node_id, cpu_total, cpu_allocated, memory_total, memory_allocated, storage_total, storage_allocated, updated_at FROM node_resources")
	if err != nil {
		return err
	}

	sm.resourcesMu.Lock()
	defer sm.resourcesMu.Unlock()

	// Clear existing cache
	sm.resources = make(map[string]*NodeResources)

	// Track errors for validation
	var errorCount int
	var successCount int
	const maxErrorThreshold = 10

	// Parse results
	for result.Next() {
		var res NodeResources
		var updatedAtUnix int64

		err := result.Scan(&res.NodeID, &res.CPUTotal, &res.CPUAllocated,
			&res.MemoryTotal, &res.MemoryAllocated, &res.StorageTotal,
			&res.StorageAllocated, &updatedAtUnix)
		if err != nil {
			errorCount++
			log.Printf("Warning: failed to scan resource row: %v", err)

			if errorCount >= maxErrorThreshold && successCount == 0 {
				return fmt.Errorf("too many scan errors (%d), aborting resource load", errorCount)
			}
			continue
		}

		successCount++
		res.UpdatedAt = time.Unix(updatedAtUnix, 0)

		sm.resources[res.NodeID] = &res
	}

	if errorCount > 0 {
		log.Printf("Loaded %d resources with %d errors", successCount, errorCount)
	}

	return nil
}

// loadMetadata loads all cluster metadata from rqlite into memory
func (sm *StateManager) loadMetadata() error {
	result, err := sm.client.Query("SELECT key, value, updated_at FROM cluster_metadata")
	if err != nil {
		return err
	}

	sm.metadataMu.Lock()
	defer sm.metadataMu.Unlock()

	// Clear existing cache
	sm.metadata = make(map[string]*ClusterMetadata)

	// Track errors for validation
	var errorCount int
	var successCount int
	const maxErrorThreshold = 10

	// Parse results
	for result.Next() {
		var meta ClusterMetadata
		var updatedAtUnix int64

		err := result.Scan(&meta.Key, &meta.Value, &updatedAtUnix)
		if err != nil {
			errorCount++
			log.Printf("Warning: failed to scan metadata row: %v", err)

			if errorCount >= maxErrorThreshold && successCount == 0 {
				return fmt.Errorf("too many scan errors (%d), aborting metadata load", errorCount)
			}
			continue
		}

		successCount++
		meta.UpdatedAt = time.Unix(updatedAtUnix, 0)

		sm.metadata[meta.Key] = &meta
	}

	if errorCount > 0 {
		log.Printf("Loaded %d metadata entries with %d errors", successCount, errorCount)
	}

	return nil
}

// loadApps loads all apps from the DB into the cache
func (sm *StateManager) loadApps() error {
	if sm.client == nil {
		return fmt.Errorf("state manager client is nil")
	}

	rows, err := sm.client.Query("SELECT id, name, description, image, version, category, icon_url, created_at, updated_at FROM apps")
	if err != nil {
		return fmt.Errorf("failed to list apps: %w", err)
	}

	sm.appsMu.Lock()
	defer sm.appsMu.Unlock()

	// Clear existing cache
	sm.apps = make(map[string]*App)

	for rows.Next() {
		var app App
		// Assuming App struct fields match the query order and types
		if err := rows.Scan(&app.ID, &app.Name, &app.Description, &app.Image, &app.Version, &app.Category, &app.IconURL, &app.CreatedAt, &app.UpdatedAt); err != nil {
			return fmt.Errorf("failed to scan app: %w", err)
		}
		sm.apps[app.ID] = &app
	}

	return nil
}

// Refresh reloads the entire state from rqlite
func (sm *StateManager) Refresh() error {
	return sm.Initialize()
}

// GetAllApps returns all cached apps
func (sm *StateManager) GetAllApps() []*App {
	sm.appsMu.RLock()
	defer sm.appsMu.RUnlock()

	apps := make([]*App, 0, len(sm.apps))
	for _, app := range sm.apps {
		apps = append(apps, app)
	}

	// Sort by name
	sort.Slice(apps, func(i, j int) bool {
		return apps[i].Name < apps[j].Name
	})

	return apps
}

// GetNode retrieves a node by ID from the cache
func (sm *StateManager) GetNode(id string) (*Node, bool) {
	sm.nodesMu.RLock()
	defer sm.nodesMu.RUnlock()

	node, exists := sm.nodes[id]
	if !exists {
		return nil, false
	}

	// Return a copy to prevent external modifications
	nodeCopy := *node
	return &nodeCopy, true
}

// GetAllNodes returns all nodes from the cache
func (sm *StateManager) GetAllNodes() []*Node {
	sm.nodesMu.RLock()
	defer sm.nodesMu.RUnlock()

	nodes := make([]*Node, 0, len(sm.nodes))
	for _, node := range sm.nodes {
		nodeCopy := *node
		nodes = append(nodes, &nodeCopy)
	}

	return nodes
}

// GetActiveNodes returns all active nodes from the cache
func (sm *StateManager) GetActiveNodes() []*Node {
	sm.nodesMu.RLock()
	defer sm.nodesMu.RUnlock()

	nodes := make([]*Node, 0)
	for _, node := range sm.nodes {
		if node.Status == NodeStatusActive {
			nodeCopy := *node
			nodes = append(nodes, &nodeCopy)
		}
	}

	return nodes
}

// GetMasterNode returns the current master node from the cache
func (sm *StateManager) GetMasterNode() (*Node, bool) {
	sm.nodesMu.RLock()
	defer sm.nodesMu.RUnlock()

	for _, node := range sm.nodes {
		if node.IsMaster {
			nodeCopy := *node
			return &nodeCopy, true
		}
	}

	return nil, false
}

// GetCapsule retrieves a capsule by ID from the cache
func (sm *StateManager) GetCapsule(id string) (*Capsule, bool) {
	sm.capsulesMu.RLock()
	defer sm.capsulesMu.RUnlock()

	capsule, exists := sm.capsules[id]
	if !exists {
		return nil, false
	}

	capsuleCopy := *capsule
	return &capsuleCopy, true
}

// SetCapsuleInCache inserts/updates a capsule in the in-memory cache.
// This is useful for tests and for components that already have the latest capsule state.
func (sm *StateManager) SetCapsuleInCache(capsule *Capsule) {
	sm.capsulesMu.Lock()
	defer sm.capsulesMu.Unlock()

	if sm.capsules == nil {
		sm.capsules = make(map[string]*Capsule)
	}
	sm.capsules[capsule.ID] = capsule
}

// GetAllCapsules returns all capsules from the cache
func (sm *StateManager) GetAllCapsules() []*Capsule {
	sm.capsulesMu.RLock()
	defer sm.capsulesMu.RUnlock()

	capsules := make([]*Capsule, 0, len(sm.capsules))
	for _, capsule := range sm.capsules {
		capsuleCopy := *capsule
		capsules = append(capsules, &capsuleCopy)
	}

	return capsules
}

// GetCapsulesByNode returns all capsules for a specific node
func (sm *StateManager) GetCapsulesByNode(nodeID string) []*Capsule {
	sm.capsulesMu.RLock()
	defer sm.capsulesMu.RUnlock()

	capsules := make([]*Capsule, 0)
	for _, capsule := range sm.capsules {
		if capsule.NodeID == nodeID {
			capsuleCopy := *capsule
			capsules = append(capsules, &capsuleCopy)
		}
	}

	return capsules
}

// GetActiveCapsuleCount returns the number of active capsules for a user
func (sm *StateManager) GetActiveCapsuleCount(userID string) int {
	sm.capsulesMu.RLock()
	defer sm.capsulesMu.RUnlock()

	count := 0
	for _, capsule := range sm.capsules {
		if capsule.UserID == userID && (capsule.Status == CapsuleStatusPending || capsule.Status == CapsuleStatusRunning) {
			count++
		}
	}
	return count
}

// GetNodeResources retrieves resource information for a node
func (sm *StateManager) GetNodeResources(nodeID string) (*NodeResources, bool) {
	sm.resourcesMu.RLock()
	defer sm.resourcesMu.RUnlock()

	res, exists := sm.resources[nodeID]
	if !exists {
		return nil, false
	}

	resCopy := *res
	return &resCopy, true
}

// GetMetadata retrieves a metadata value by key
func (sm *StateManager) GetMetadata(key string) (string, bool) {
	sm.metadataMu.RLock()
	defer sm.metadataMu.RUnlock()

	meta, exists := sm.metadata[key]
	if !exists {
		return "", false
	}

	return meta.Value, true
}

// Stats returns statistics about the current state
func (sm *StateManager) Stats() map[string]interface{} {
	sm.nodesMu.RLock()
	nodeCount := len(sm.nodes)
	activeNodeCount := 0
	for _, node := range sm.nodes {
		if node.Status == NodeStatusActive {
			activeNodeCount++
		}
	}
	sm.nodesMu.RUnlock()

	sm.capsulesMu.RLock()
	capsuleCount := len(sm.capsules)
	runningCapsuleCount := 0
	for _, capsule := range sm.capsules {
		if capsule.Status == CapsuleStatusRunning {
			runningCapsuleCount++
		}
	}
	sm.capsulesMu.RUnlock()

	masterNode, hasMaster := sm.GetMasterNode()

	stats := map[string]interface{}{
		"total_nodes":      nodeCount,
		"active_nodes":     activeNodeCount,
		"total_capsules":   capsuleCount,
		"running_capsules": runningCapsuleCount,
		"has_master":       hasMaster,
	}

	if hasMaster {
		stats["master_node_id"] = masterNode.ID
	}

	return stats
}

// MarshalJSON provides JSON serialization for debugging
func (sm *StateManager) MarshalJSON() ([]byte, error) {
	return json.Marshal(sm.Stats())
}

// GetClient returns the underlying rqlite client
func (sm *StateManager) GetClient() *Client {
	return sm.client
}

// ExecuteRaw forwards a write query directly to the underlying rqlite client.
// This is primarily used for administrative operations such as recording election history.
func (sm *StateManager) ExecuteRaw(query string, args ...interface{}) error {
	if sm.client == nil {
		return fmt.Errorf("state manager client is not initialized")
	}
	return sm.client.Execute(query, args...)
}

// SetMaster sets the given node as master and clears master status from other nodes
func (sm *StateManager) SetMaster(masterID string) error {
	sm.nodesMu.Lock()
	defer sm.nodesMu.Unlock()

	// Clear master status from all nodes
	for _, node := range sm.nodes {
		if node.IsMaster && node.ID != masterID {
			node.IsMaster = false
			node.UpdatedAt = time.Now()
			// Update in database
			query := fmt.Sprintf(`
				UPDATE nodes SET is_master = 0, updated_at = %d WHERE id = '%s'
			`, node.UpdatedAt.Unix(), escapeSQLString(node.ID))
			if err := sm.client.Execute(query); err != nil {
				return fmt.Errorf("failed to clear master status for node %s: %w", node.ID, err)
			}
		}
	}

	// Set the new master
	masterNode, exists := sm.nodes[masterID]
	if !exists {
		return fmt.Errorf("node %s not found", masterID)
	}

	masterNode.IsMaster = true
	masterNode.UpdatedAt = time.Now()

	// Update in database
	query := fmt.Sprintf(`
		UPDATE nodes SET is_master = 1, updated_at = %d WHERE id = '%s'
	`, masterNode.UpdatedAt.Unix(), escapeSQLString(masterID))

	if err := sm.client.Execute(query); err != nil {
		return fmt.Errorf("failed to set master status for node %s: %w", masterID, err)
	}

	return nil
}

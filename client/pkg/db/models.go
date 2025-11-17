package db

import (
	"time"
)

// NodeStatus represents the status of a node in the cluster
type NodeStatus string

const (
	NodeStatusActive   NodeStatus = "active"
	NodeStatusInactive NodeStatus = "inactive"
	NodeStatusFailed   NodeStatus = "failed"
)

// Node represents a cluster node
type Node struct {
	ID            string     `json:"id"`             // ULID
	Address       string     `json:"address"`        // IP:PORT
	HeadscaleName string     `json:"headscale_name"` // Node name in headscale
	Status        NodeStatus `json:"status"`         // Current status
	IsMaster      bool       `json:"is_master"`      // True if this is the master
	LastSeen      time.Time  `json:"last_seen"`      // Last heartbeat time
	CreatedAt     time.Time  `json:"created_at"`     // Registration time
	UpdatedAt     time.Time  `json:"updated_at"`     // Last update time
}

// CapsuleStatus represents the status of a capsule
type CapsuleStatus string

const (
	CapsuleStatusPending CapsuleStatus = "pending"
	CapsuleStatusRunning CapsuleStatus = "running"
	CapsuleStatusStopped CapsuleStatus = "stopped"
	CapsuleStatusFailed  CapsuleStatus = "failed"
)

// Capsule represents a deployed capsule in the cluster
type Capsule struct {
	ID            string        `json:"id"`             // ULID
	Name          string        `json:"name"`           // Human-readable name
	NodeID        string        `json:"node_id"`        // Node where deployed
	Manifest      string        `json:"manifest"`       // JSON manifest (adep.json)
	Status        CapsuleStatus `json:"status"`         // Current status
	StoragePath   string        `json:"storage_path"`   // Storage path on node
	BundlePath    string        `json:"bundle_path"`    // OCI bundle path on node
	NetworkConfig string        `json:"network_config"` // JSON network config
	CreatedAt     time.Time     `json:"created_at"`     // Creation time
	UpdatedAt     time.Time     `json:"updated_at"`     // Last update time
}

// NodeResources represents resource allocations for a node
type NodeResources struct {
	NodeID           string    `json:"node_id"`
	CPUTotal         int64     `json:"cpu_total"`         // Total CPU in millicores
	CPUAllocated     int64     `json:"cpu_allocated"`     // Allocated CPU in millicores
	MemoryTotal      int64     `json:"memory_total"`      // Total memory in bytes
	MemoryAllocated  int64     `json:"memory_allocated"`  // Allocated memory in bytes
	StorageTotal     int64     `json:"storage_total"`     // Total storage in bytes
	StorageAllocated int64     `json:"storage_allocated"` // Allocated storage in bytes
	UpdatedAt        time.Time `json:"updated_at"`        // Last update time
}

// CapsuleResources represents resource requests for a capsule
type CapsuleResources struct {
	CapsuleID      string `json:"capsule_id"`
	CPURequest     int64  `json:"cpu_request"`     // Requested CPU in millicores
	MemoryRequest  int64  `json:"memory_request"`  // Requested memory in bytes
	StorageRequest int64  `json:"storage_request"` // Requested storage in bytes
}

// ElectionReason represents why a master election occurred
type ElectionReason string

const (
	ElectionReasonStartup  ElectionReason = "startup"
	ElectionReasonFailover ElectionReason = "failover"
	ElectionReasonManual   ElectionReason = "manual"
)

// MasterElection represents a master election event
type MasterElection struct {
	ID         int64          `json:"id"`
	NodeID     string         `json:"node_id"`     // Node that became master
	ElectedAt  time.Time      `json:"elected_at"`  // Election time
	Reason     ElectionReason `json:"reason"`      // Election reason
	QuorumSize int            `json:"quorum_size"` // Quorum size at election time
}

// ClusterMetadata represents cluster-wide configuration
type ClusterMetadata struct {
	Key       string    `json:"key"`
	Value     string    `json:"value"`
	UpdatedAt time.Time `json:"updated_at"`
}

// ResourceAllocation represents available resources on a node
type ResourceAllocation struct {
	CPUAvailable     int64 `json:"cpu_available"`
	MemoryAvailable  int64 `json:"memory_available"`
	StorageAvailable int64 `json:"storage_available"`
}

// CalculateAvailableResources calculates available resources from node resources
func (nr *NodeResources) CalculateAvailableResources() ResourceAllocation {
	return ResourceAllocation{
		CPUAvailable:     nr.CPUTotal - nr.CPUAllocated,
		MemoryAvailable:  nr.MemoryTotal - nr.MemoryAllocated,
		StorageAvailable: nr.StorageTotal - nr.StorageAllocated,
	}
}

// CanAllocate checks if a node can allocate the requested resources
func (nr *NodeResources) CanAllocate(req CapsuleResources) bool {
	available := nr.CalculateAvailableResources()
	return available.CPUAvailable >= req.CPURequest &&
		available.MemoryAvailable >= req.MemoryRequest &&
		available.StorageAvailable >= req.StorageRequest
}

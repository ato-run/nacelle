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
	TailnetIP     string     `json:"tailnet_ip"`     // Node Tailnet IP address
	Status        NodeStatus `json:"status"`         // Current status
	IsMaster      bool       `json:"is_master"`      // True if this is the master
	LastSeen      time.Time  `json:"last_seen"`      // Last heartbeat time
	CreatedAt     time.Time  `json:"created_at"`     // Registration time
	UpdatedAt     time.Time  `json:"updated_at"`     // Last update time
}

// NodeWorkload represents a workload reported by a node
type NodeWorkload struct {
	NodeID            string    `json:"node_id"`
	WorkloadID        string    `json:"workload_id"`
	Name              string    `json:"name"`
	ReservedVRAMBytes uint64    `json:"reserved_vram_bytes"`
	ObservedVRAMBytes uint64    `json:"observed_vram_bytes"`
	PID               int64     `json:"pid"`
	Phase             string    `json:"phase"`
	UpdatedAt         time.Time `json:"updated_at"`
}

// NodeGpu represents a GPU on a node
type NodeGpu struct {
	ID             string    `json:"id"` // UUID
	NodeID         string    `json:"node_id"`
	Index          int       `json:"index"`
	Name           string    `json:"name"`
	TotalVRAMBytes uint64    `json:"total_vram_bytes"`
	UsedVRAMBytes  uint64    `json:"used_vram_bytes"`
	UpdatedAt      time.Time `json:"updated_at"`
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
	UserID        string        `json:"user_id"`        // Owner User ID
	Name          string        `json:"name"`           // Human-readable name
	NodeID        string        `json:"node_id"`        // Node where deployed
	RuntimeName   string        `json:"runtime_name"`   // Runtime name (e.g. python, node)
	Manifest      string        `json:"manifest"`       // JSON manifest (adep.json)
	Status        CapsuleStatus `json:"status"`         // Current status
	Port          int           `json:"port"`           // Exposed port
	AccessURL     string        `json:"access_url"`     // Access URL
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

// Runtime represents a runtime and its metadata
type Runtime struct {
	ID            string    `json:"id" db:"id"`
	Name          string    `json:"name" db:"name"`
	Type          string    `json:"type" db:"type"`
	Description   string    `json:"description" db:"description"`
	LatestVersion string    `json:"latest_version" db:"latest_version"`
	CreatedAt     time.Time `json:"created_at" db:"created_at"`
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

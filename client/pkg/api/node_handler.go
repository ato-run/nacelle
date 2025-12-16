package api

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"

	"github.com/onescluster/coordinator/pkg/db"
)

// NodeHandler handles Node (Rig) operations
// NodeHandler handles Node (Rig) operations
type NodeHandler struct {
	StateManager *db.StateManager
}

// NewNodeHandler creates a new node handler
func NewNodeHandler(stateManager *db.StateManager) *NodeHandler {
	return &NodeHandler{
		StateManager: stateManager,
	}
}

// NodeInfo represents node information for API response
type NodeInfo struct {
	RigID            string    `json:"rig_id"`
	Status           string    `json:"status"`
	TotalVRAM        uint64    `json:"total_vram_bytes"`
	AllocatedVRAM    uint64    `json:"allocated_vram_bytes"`
	AvailableVRAM    uint64    `json:"available_vram_bytes"`
	GPUs             []GPUInfo `json:"gpus"`
	CudaVersion      string    `json:"cuda_version,omitempty"`
	DriverVersion    string    `json:"driver_version,omitempty"`
	RunningWorkloads int       `json:"running_workloads"`
}

// GPUInfo represents GPU information for API response
type GPUInfo struct {
	Index       uint32 `json:"index"`
	UUID        string `json:"uuid"`
	Model       string `json:"model"`
	VRAMBytes   uint64 `json:"vram_bytes"`
	UsedVRAM    uint64 `json:"used_vram_bytes,omitempty"`
	Temperature uint32 `json:"temperature_celsius,omitempty"`
}

// HandleListNodes lists all nodes (Rigs) in the cluster
// GET /api/v1/nodes
func (h *NodeHandler) HandleListNodes(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	ctx := r.Context()

	// Query all GPU Rigs from StateManager
	rigs, err := h.StateManager.GetAllGpuRigs(ctx)
	if err != nil {
		http.Error(w, fmt.Sprintf("Failed to query nodes: %v", err), http.StatusInternalServerError)
		return
	}

	// Convert to API response format
	nodes := make([]NodeInfo, 0, len(rigs))
	for _, rig := range rigs {
		// Note: RigGpuInfo doesn't have detailed GPU list yet
		// This will be added in Phase 2 when we expand GPU monitoring
		gpus := []GPUInfo{} // Empty for now
		for _, g := range rig.Gpus {
			gpus = append(gpus, GPUInfo{
				Index:     uint32(0), // TODO: Fix index
				UUID:      g.UUID,
				Model:     g.DeviceName,
				VRAMBytes: g.TotalVRAMBytes,
				UsedVRAM:  0, // TODO: Per-GPU usage
			})
		}

		node := NodeInfo{
			RigID:            rig.RigID,
			Status:           "active", // TODO: Determine from last_seen timestamp
			TotalVRAM:        rig.TotalVRAMBytes,
			AllocatedVRAM:    rig.UsedVRAMBytes,
			AvailableVRAM:    rig.AvailableVRAMBytes(),
			GPUs:             gpus,
			CudaVersion:      rig.CudaDriverVersion,
			DriverVersion:    "", // Not in RigGpuInfo yet
			RunningWorkloads: 0,  // TODO: Count from workload store
		}
		nodes = append(nodes, node)
	}

	response := map[string]interface{}{
		"nodes": nodes,
		"total": len(nodes),
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	if err := json.NewEncoder(w).Encode(response); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
}

// HandleGetNode retrieves a specific node by ID
// GET /api/v1/nodes/:id
func (h *NodeHandler) HandleGetNode(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Extract node ID from URL path
	// This is a placeholder - in production, use a proper router like gorilla/mux
	http.Error(w, "Not implemented yet", http.StatusNotImplemented)
}

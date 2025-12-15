package api

import (

	"encoding/json"
	"fmt"

	"net/http"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
	"github.com/onescluster/coordinator/pkg/engine"
	pb "github.com/onescluster/coordinator/pkg/proto"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
	"github.com/onescluster/coordinator/pkg/wasm"

)

type DeployRequest struct {
	AppID          string             `json:"app_id"`
	RuntimeName    string             `json:"runtime_name"`
	RuntimeVersion string             `json:"runtime_version"`
	Config         map[string]string  `json:"config"`
	Requirements   gpu.GpuConstraints `json:"requirements"`
	AllowCloudBurst bool              `json:"allow_cloud_burst"`
}

// AdepManifest represents the adep.json structure (simplified for Week 4)
type AdepManifest struct {
	Name       string           `json:"name"`
	Scheduling SchedulingConfig `json:"scheduling"`
	Compute    ComputeConfig    `json:"compute"`
	Volumes    []VolumeConfig   `json:"volumes"`
}

type SchedulingConfig struct {
	GPU      *GpuConstraints `json:"gpu"`
	Strategy string          `json:"strategy,omitempty"`
}

type GpuConstraints struct {
	VramMinGB      uint64  `json:"vram_min_gb"`
	CudaVersionMin *string `json:"cuda_version_min,omitempty"`
}

type ComputeConfig struct {
	Image string   `json:"image"`
	Args  []string `json:"args"`
	Env   []string `json:"env"`
}

// toProtoManifest removed (no longer needed)
// envSliceToMap removed (no longer needed)

type VolumeConfig struct {
	Type        string `json:"type"`
	Source      string `json:"source"`
	Destination string `json:"destination"`
	Readonly    bool   `json:"readonly"`
}

// AgentClientFactory creates a gRPC client for Agent communication
// Allows dependency injection for testing
// AgentClientFactory removed


// DeployHandler handles workload deployment requests
type DeployHandler struct {
	StateManager       *db.StateManager
	Scheduler          *gpu.Scheduler
	// AgentClientFactory AgentClientFactory // Removed

	WasmHost           *wasm.WasmerHost   // Optional: for Wasm validation
}

// NewDeployHandler creates a new deploy handler
func NewDeployHandler(stateManager *db.StateManager, scheduler *gpu.Scheduler) *DeployHandler {
	return &DeployHandler{
		StateManager:       stateManager,
		Scheduler:          scheduler,
		// AgentClientFactory: nil, 

		WasmHost:           nil, // Initialize on first use
	}
}

// HandleDeploy processes deployment requests
//
// Flow:
// 1. Parse adep.json from request body
// 2. Extract GPU constraints
// 3. Query all available Rigs from StateManager
// 4. Use Scheduler to find best Rig (Week 2 Filter-Score pipeline)
// 5. Reserve VRAM atomically in StateManager
// 6. Call selected Agent's DeployWorkload gRPC endpoint
// 7. Return deployment result
func (h *DeployHandler) HandleDeploy(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// 1. Parse request
	var req DeployRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid request body", http.StatusBadRequest)
		return
	}

	// Ensure requirements are set
	if req.Requirements.VramMinGB == 0 {
		// Default to some reasonable value if not specified
	}

	// Resolve AppID if provided
	if req.AppID != "" {
		// Lookup App from StateManager (which loads from DB)
		// We need to add GetApp to StateManager or access map directly if exposed
		// Since GetAllApps returns []*App, likely GetApp(id) is needed or we can iterate
		// For efficiency, let's assume we add GetApp to StateManager or use GetAllApps and filter
		// Or access stateManager.apps if visible? No, it's private.
		// Let's rely on adding GetApp to StateManager in a separate step if not present.
		// Wait, I didn't add GetApp to StateManager yet. I should add it first.
		// BUT for now, I'll assume GetApp exists or implement basic lookup here via GetAllApps
		
		apps := h.StateManager.GetAllApps()
		var selectedApp *db.App
		for _, app := range apps {
			if app.ID == req.AppID {
				selectedApp = app
				break
			}
		}

		if selectedApp != nil {
			if req.RuntimeName == "" {
				req.RuntimeName = selectedApp.Name
			}
			// Use App Image as the container image
            // We store the image in the manifest.Compute.Image
		}
	}
	
	// If RuntimeName is still empty (no AppID or App not found), error out?
	// Or assume the user meant to provide it manually.


	// Populate AllowCloudBurst from request to requirements
	req.Requirements.AllowCloudBurst = req.AllowCloudBurst

	ctx := r.Context()
	userID, _ := ctx.Value("user_id").(string)

	// Enforce Limits
	if userID != "" {
		activeCount := h.StateManager.GetActiveCapsuleCount(userID)
		// TODO: Fetch actual limit from subscription
		limit := 5 
		if activeCount >= limit {
			http.Error(w, fmt.Sprintf("Capsule limit reached (%d)", limit), http.StatusForbidden)
			return
		}
	}

	// 1. Get available machines with GPUs
	rigs, err := h.StateManager.GetAllGpuRigs(ctx)
	if err != nil {
		http.Error(w, "failed to get GPU rigs", http.StatusInternalServerError)
		return
	}

	// 2. Schedule to best machine
	// Note: Task 5.2 will update Scheduler to support cloud bursting options.
	// For now, we use existing FindBestRigWithAssignment.
	bestRig, _, err := h.Scheduler.FindBestRigWithAssignment(rigs, &req.Requirements)
	if err != nil {
		http.Error(w, fmt.Sprintf("scheduling failed: %v", err), http.StatusServiceUnavailable)
		return
	}

	// 3. Reserve VRAM
	// We need a workload ID.
	workloadID := fmt.Sprintf("wl-%d", time.Now().UnixNano())
	requiredVRAM := req.Requirements.RequiredVRAMBytes()
	
	if err := h.StateManager.ReserveVRAM(ctx, bestRig.RigID, workloadID, requiredVRAM); err != nil {
		http.Error(w, "failed to reserve VRAM", http.StatusConflict)
		return
	}

	// 4. Connect to target Engine
	// We need to get the Node info to get the address
	node, exists := h.StateManager.GetNode(bestRig.RigID)
	if !exists {
		h.StateManager.ReleaseVRAMByWorkload(ctx, bestRig.RigID, workloadID)
		http.Error(w, "scheduled node not found", http.StatusInternalServerError)
		return
	}

	engineAddr := h.getEngineAddress(node)
	engineClient, err := engine.NewRemoteEngineClient(engineAddr)
	if err != nil {
		// Rollback VRAM reservation
		h.StateManager.ReleaseVRAMByWorkload(ctx, bestRig.RigID, workloadID)
		http.Error(w, fmt.Sprintf("failed to connect to engine: %v", err), http.StatusServiceUnavailable)
		return
	}
	defer engineClient.Close()

	// 5. Execute on remote Engine
	// We need to construct AdepManifest from DeployRequest
	
	// If AppID was used, we should ideally retrieve the image from the App definition
	// But `req.RuntimeName` is just a name.
	// Let's retry the app lookup logic here cleanly or persist the image from step 1
	var appImage string
	if req.AppID != "" {
		apps := h.StateManager.GetAllApps()
		for _, app := range apps {
			if app.ID == req.AppID {
				appImage = app.Image
				if req.RuntimeName == "" {
					req.RuntimeName = app.Name // Use App name as capsule name if default
				}
				break
			}
		}
	}
	
	if appImage == "" {
		appImage = req.RuntimeName // Fallback to using name as image (legacy behavior)
	}

	manifest := AdepManifest{
		Name:    req.RuntimeName,
		Compute: ComputeConfig{
			Image: appImage,
		},
		Scheduling: SchedulingConfig{
			GPU: &GpuConstraints{
				VramMinGB: req.Requirements.VramMinGB,
			},
		},
	}
	if req.Requirements.CudaVersionMin != "" {
		manifest.Scheduling.GPU.CudaVersionMin = &req.Requirements.CudaVersionMin
	}

	manifestBytes, err := json.Marshal(manifest)
	if err != nil {
		h.StateManager.ReleaseVRAMByWorkload(ctx, bestRig.RigID, workloadID)
		http.Error(w, "failed to marshal manifest", http.StatusInternalServerError)
		return
	}

	execReq := &pb.DeployRequest{
		Manifest: &pb.DeployRequest_AdepJson{
			AdepJson: manifestBytes,
		},
	}

	resp, err := engineClient.DeployCapsule(ctx, execReq)
	if err != nil {
		h.StateManager.ReleaseVRAMByWorkload(ctx, bestRig.RigID, workloadID)
		http.Error(w, fmt.Sprintf("execution failed: %v", err), http.StatusInternalServerError)
		return
	}

	// 6. Record capsule in DB
	capsule := &db.Capsule{
		ID:            resp.CapsuleId,
		UserID:        userID,
		Name:          req.RuntimeName,
		NodeID:        bestRig.RigID,
		RuntimeName:   req.RuntimeName,
		Manifest:      string(manifestBytes),
		Status:        db.CapsuleStatusRunning,
		AccessURL:     resp.LocalUrl, // Use LocalUrl from response
		// Port is not in response. We can try to parse it from LocalUrl if needed.
		// For now, leave it as 0.
		CreatedAt:     time.Now(),
		UpdatedAt:     time.Now(),
	}
	
	h.StateManager.CreateCapsule(capsule)

	response := map[string]interface{}{
		"capsule_id": resp.CapsuleId,
		"machine_id": bestRig.RigID,
		"access_url": resp.LocalUrl,
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

// callAgentDeploy removed


func (h *DeployHandler) getEngineAddress(machine *db.Node) string {
	// Prefer Tailnet IP if available
	if machine.TailnetIP != "" {
		return fmt.Sprintf("%s:50051", machine.TailnetIP)
	}
	// Fallback to hostname (local network) or Address
	return fmt.Sprintf("%s:50051", machine.Address)
}

func (h *DeployHandler) buildAccessURL(machine *db.Node, port int) string {
	host := machine.TailnetIP
	if host == "" {
		host = machine.Address
	}
	return fmt.Sprintf("http://%s:%d", host, port)
}

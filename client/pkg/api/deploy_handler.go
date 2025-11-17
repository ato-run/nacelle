package api

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"strings"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
	pb "github.com/onescluster/coordinator/pkg/proto"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
	"github.com/onescluster/coordinator/pkg/wasm"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

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

// toProtoManifest converts the HTTP-level manifest into the strongly typed proto message.
func toProtoManifest(m *AdepManifest) (*pb.AdePManifest, error) {
	if m == nil {
		return nil, fmt.Errorf("manifest is nil")
	}

	gpuConfig := &pb.GpuConstraints{}
	if m.Scheduling.GPU != nil {
		gpuConfig.VramMinGb = m.Scheduling.GPU.VramMinGB
		if m.Scheduling.GPU.CudaVersionMin != nil {
			gpuConfig.CudaVersionMin = *m.Scheduling.GPU.CudaVersionMin
		}
	}

	scheduling := &pb.SchedulingConfig{
		Gpu:      gpuConfig,
		Strategy: m.Scheduling.Strategy,
	}

	compute := &pb.ComputeConfig{
		Image: m.Compute.Image,
		Args:  append([]string{}, m.Compute.Args...),
		Env:   envSliceToMap(m.Compute.Env),
	}

	volumes := make([]*pb.Volume, 0, len(m.Volumes))
	for _, v := range m.Volumes {
		volumes = append(volumes, &pb.Volume{
			Type:        v.Type,
			Source:      v.Source,
			Destination: v.Destination,
			Readonly:    v.Readonly,
		})
	}

	return &pb.AdePManifest{
		Name:       m.Name,
		Scheduling: scheduling,
		Compute:    compute,
		Volumes:    volumes,
	}, nil
}

func envSliceToMap(env []string) map[string]string {
	if len(env) == 0 {
		return nil
	}
	result := make(map[string]string, len(env))
	for _, kv := range env {
		if kv == "" {
			continue
		}
		parts := strings.SplitN(kv, "=", 2)
		if len(parts) == 2 {
			result[parts[0]] = parts[1]
		}
	}
	return result
}

type VolumeConfig struct {
	Type        string `json:"type"`
	Source      string `json:"source"`
	Destination string `json:"destination"`
	Readonly    bool   `json:"readonly"`
}

// AgentClientFactory creates a gRPC client for Agent communication
// Allows dependency injection for testing
type AgentClientFactory func(ctx context.Context, rigID string) (pb.CoordinatorClient, func() error, error)

// DeployHandler handles workload deployment requests
type DeployHandler struct {
	NodeStore          *db.NodeStore
	Scheduler          *gpu.Scheduler
	AgentClientFactory AgentClientFactory // Optional: for testing (nil = use default)
	WasmHost           *wasm.WasmerHost   // Optional: for Wasm validation
}

// NewDeployHandler creates a new deploy handler
func NewDeployHandler(nodeStore *db.NodeStore, scheduler *gpu.Scheduler) *DeployHandler {
	return &DeployHandler{
		NodeStore:          nodeStore,
		Scheduler:          scheduler,
		AgentClientFactory: nil, // Use default (localhost:50051)
		WasmHost:           nil, // Initialize on first use
	}
}

// HandleDeploy processes deployment requests
//
// Flow:
// 1. Parse adep.json from request body
// 2. Extract GPU constraints
// 3. Query all available Rigs from NodeStore
// 4. Use Scheduler to find best Rig (Week 2 Filter-Score pipeline)
// 5. Reserve VRAM atomically in NodeStore
// 6. Call selected Agent's DeployWorkload gRPC endpoint
// 7. Return deployment result
func (h *DeployHandler) HandleDeploy(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	log.Println("🚀 Received deployment request")

	// 1. Parse adep.json
	body, err := io.ReadAll(r.Body)
	if err != nil {
		log.Printf("❌ Failed to read request body: %v", err)
		http.Error(w, "Failed to read request body", http.StatusBadRequest)
		return
	}
	defer r.Body.Close()

	// 1.5 Validate manifest with Wasm (if available)
	if h.WasmHost != nil {
		valid, err := h.WasmHost.ValidateManifest(body)
		if err != nil {
			log.Printf("⚠️ Wasm validation error: %v", err)
			// Don't fail on Wasm error, just log and continue
		} else if !valid {
			log.Printf("❌ Manifest validation failed (Wasm)")
			http.Error(w, "Invalid manifest: name and version are required", http.StatusBadRequest)
			return
		} else {
			log.Println("✅ Manifest validation passed (Wasm)")
		}
	}

	var manifest AdepManifest
	if err := json.Unmarshal(body, &manifest); err != nil {
		log.Printf("❌ Failed to parse adep.json: %v", err)
		http.Error(w, "Invalid adep.json format", http.StatusBadRequest)
		return
	}

	log.Printf("  Workload: %s", manifest.Name)
	log.Printf("  Image: %s", manifest.Compute.Image)

	// 2. Extract GPU constraints
	if manifest.Scheduling.GPU == nil {
		log.Println("❌ No GPU constraints specified")
		http.Error(w, "GPU constraints required", http.StatusBadRequest)
		return
	}

	constraints := &gpu.GpuConstraints{
		VramMinGB:      manifest.Scheduling.GPU.VramMinGB,
		CudaVersionMin: "",
	}
	if manifest.Scheduling.GPU.CudaVersionMin != nil {
		constraints.CudaVersionMin = *manifest.Scheduling.GPU.CudaVersionMin
	}

	log.Printf("  Required VRAM: %d GB", constraints.VramMinGB)
	if constraints.CudaVersionMin != "" {
		log.Printf("  Min CUDA Version: %s", constraints.CudaVersionMin)
	}

	// 3. Query all available Rigs
	ctx := r.Context()
	allRigs, err := h.NodeStore.GetAllGpuRigs(ctx)
	if err != nil {
		log.Printf("❌ Failed to query Rigs: %v", err)
		http.Error(w, "Database error", http.StatusInternalServerError)
		return
	}

	if len(allRigs) == 0 {
		log.Println("❌ No Rigs available in cluster")
		http.Error(w, "No Rigs available", http.StatusServiceUnavailable)
		return
	}

	log.Printf("  Available Rigs: %d", len(allRigs))

	// 4. Use Scheduler to find best Rig
	bestRig, err := h.Scheduler.FindBestRig(allRigs, constraints)
	if err != nil {
		log.Printf("❌ Scheduling failed: %v", err)
		http.Error(w, fmt.Sprintf("Scheduling failed: %v", err), http.StatusServiceUnavailable)
		return
	}

	log.Printf("✅ Scheduled to Rig: %s", bestRig.RigID)
	log.Printf("  VRAM: %.2f GB available / %.2f GB total",
		float64(bestRig.AvailableVRAMBytes())/(1024*1024*1024),
		float64(bestRig.TotalVRAMBytes)/(1024*1024*1024))

	// 5. Reserve VRAM atomically
	requiredVRAM := constraints.VramMinGB * 1024 * 1024 * 1024
	if err := h.NodeStore.ReserveVRAM(ctx, bestRig.RigID, requiredVRAM); err != nil {
		log.Printf("❌ VRAM reservation failed: %v", err)
		http.Error(w, "VRAM reservation failed", http.StatusConflict)
		return
	}

	log.Printf("  Reserved %d GB VRAM on Rig %s", constraints.VramMinGB, bestRig.RigID)

	// 6. Call Agent's DeployWorkload gRPC endpoint
	manifestJSONBytes, _ := json.Marshal(manifest)
	deployResp, err := h.callAgentDeploy(ctx, bestRig.RigID, string(manifestJSONBytes), &manifest)
	if err != nil {
		log.Printf("❌ Agent deployment failed: %v", err)
		// Rollback VRAM reservation
		if rollbackErr := h.NodeStore.ReleaseVRAM(ctx, bestRig.RigID, requiredVRAM); rollbackErr != nil {
			log.Printf("⚠️ VRAM rollback failed: %v", rollbackErr)
		}
		http.Error(w, fmt.Sprintf("Agent deployment failed: %v", err), http.StatusInternalServerError)
		return
	}

	// 7. Return success response
	log.Printf("✅ Deployment successful on Rig: %s", bestRig.RigID)

	response := map[string]interface{}{
		"success": deployResp.Success,
		"message": deployResp.Message,
		"rig_id":  bestRig.RigID,
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(response)
}

// callAgentDeploy calls the selected Agent's DeployWorkload gRPC endpoint
func (h *DeployHandler) callAgentDeploy(ctx context.Context, rigID string, manifestJSON string, manifest *AdepManifest) (*pb.DeployWorkloadResponse, error) {
	var client pb.CoordinatorClient
	var closeFunc func() error

	// Use factory if provided (for testing), otherwise use default
	if h.AgentClientFactory != nil {
		var err error
		client, closeFunc, err = h.AgentClientFactory(ctx, rigID)
		if err != nil {
			return nil, fmt.Errorf("failed to create Agent client: %w", err)
		}
		if closeFunc != nil {
			defer closeFunc()
		}
	} else {
		// Default: connect to localhost:50051
		// In production, this would use service discovery to find the Agent's address
		agentAddr := fmt.Sprintf("localhost:50051") // TODO: Service discovery in production

		log.Printf("  Connecting to Agent at %s", agentAddr)

		// Create gRPC connection with timeout
		ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
		defer cancel()

		conn, err := grpc.DialContext(ctx, agentAddr,
			grpc.WithTransportCredentials(insecure.NewCredentials()),
			grpc.WithBlock())
		if err != nil {
			return nil, fmt.Errorf("failed to connect to Agent: %w", err)
		}
		defer conn.Close()

		client = pb.NewCoordinatorClient(conn)
	}

	// Call DeployWorkload RPC
	manifestProto, err := toProtoManifest(manifest)
	if err != nil {
		return nil, fmt.Errorf("failed to convert manifest: %w", err)
	}

	req := &pb.DeployWorkloadRequest{
		WorkloadId:   manifest.Name,
		Manifest:     manifestProto,
		ManifestJson: manifestJSON,
	}

	resp, err := client.DeployWorkload(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("DeployWorkload RPC failed: %w", err)
	}

	return resp, nil
}

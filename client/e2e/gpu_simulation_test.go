package e2e

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/api"
	"github.com/onescluster/coordinator/pkg/db"
	pb "github.com/onescluster/coordinator/pkg/proto"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/test/bufconn"

	_ "modernc.org/sqlite"
)

// Test network topology constants
// These represent the simulated network configuration for GPU-enabled Rigs
const (
	testNodeAddr1 = "192.168.1.10" // Rig A: 2x RTX 4090
	testNodeAddr2 = "192.168.1.11" // Rig B: 1x RTX 4090
	testNodeAddr3 = "192.168.1.12" // Rig C: 4x A100
)

// mockAgentServer simulates an Agent's gRPC server for Week 4 E2E test
type mockAgentServer struct {
	pb.UnimplementedCoordinatorServer
	deployRequests []*pb.DeployWorkloadRequest // Track all deploy requests received
}

func (m *mockAgentServer) DeployWorkload(ctx context.Context, req *pb.DeployWorkloadRequest) (*pb.DeployWorkloadResponse, error) {
	m.deployRequests = append(m.deployRequests, req)

	// Parse manifest to verify it's valid JSON
	var manifest map[string]interface{}
	if err := json.Unmarshal([]byte(req.ManifestJson), &manifest); err != nil {
		return &pb.DeployWorkloadResponse{
			Success: false,
			Message: "Invalid manifest JSON",
		}, nil
	}

	return &pb.DeployWorkloadResponse{
		Success: true,
		Message: "Workload deployed successfully (mock)",
	}, nil
}

// TestGpuSimulationE2E verifies the complete GPU-aware deployment flow
//
// This test simulates the full Week 1-4 integration:
// 1. Week 1: Mock GPU hardware data is seeded in database
// 2. Week 2: Scheduler finds best Rig using Filter-Score pipeline
// 3. Week 3: OCI spec is generated (validated by Agent mock)
// 4. Week 4: VRAM is reserved and DeployWorkload RPC is called
func TestGpuSimulationE2E(t *testing.T) {
	// --- Setup: Database and NodeStore ---
	testDB, err := sql.Open("sqlite", ":memory:")
	require.NoError(t, err)
	defer testDB.Close()

	// Create nodes table
	_, err = testDB.Exec(`
		CREATE TABLE nodes (
			id TEXT PRIMARY KEY,
			address TEXT,
			headscale_name TEXT,
			status TEXT,
			is_master INTEGER,
			last_seen INTEGER,
			created_at INTEGER,
			updated_at INTEGER,
			total_vram_bytes INTEGER NOT NULL DEFAULT 0,
			used_vram_bytes INTEGER NOT NULL DEFAULT 0,
			cuda_driver_version TEXT DEFAULT ''
		)
	`)
	require.NoError(t, err)

	nodeStore := db.NewNodeStore(testDB)

	// --- Setup: Seed mock GPU Rigs (Week 1 hardware reports) ---
	now := time.Now().Unix()

	// Rig A: 2x RTX 4090 (96 GB total, 40 GB used)
	_, err = testDB.Exec(`
		INSERT INTO nodes (id, address, headscale_name, status, is_master, last_seen, created_at, updated_at, total_vram_bytes, used_vram_bytes, cuda_driver_version)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`, "rig-a", testNodeAddr1, "rig-a", "active", 0, now, now, now, 96*1024*1024*1024, 40*1024*1024*1024, "12.2")
	require.NoError(t, err)

	// Rig B: 1x RTX 4090 (48 GB total, 10 GB used)
	_, err = testDB.Exec(`
		INSERT INTO nodes (id, address, headscale_name, status, is_master, last_seen, created_at, updated_at, total_vram_bytes, used_vram_bytes, cuda_driver_version)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`, "rig-b", testNodeAddr2, "rig-b", "active", 0, now, now, now, 48*1024*1024*1024, 10*1024*1024*1024, "12.2")
	require.NoError(t, err)

	// Rig C: 4x A100 (320 GB total, 300 GB used - nearly full)
	_, err = testDB.Exec(`
		INSERT INTO nodes (id, address, headscale_name, status, is_master, last_seen, created_at, updated_at, total_vram_bytes, used_vram_bytes, cuda_driver_version)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
	`, "rig-c", testNodeAddr3, "rig-c", "active", 0, now, now, now, 320*1024*1024*1024, 300*1024*1024*1024, "12.2")
	require.NoError(t, err)

	// --- Setup: Mock Agent gRPC Server ---
	mockAgent := &mockAgentServer{
		deployRequests: make([]*pb.DeployWorkloadRequest, 0),
	}

	// Use bufconn for in-memory gRPC connection
	listener := bufconn.Listen(1024 * 1024)
	grpcServer := grpc.NewServer()
	pb.RegisterCoordinatorServer(grpcServer, mockAgent)

	go func() {
		if err := grpcServer.Serve(listener); err != nil {
			t.Errorf("gRPC server failed: %v", err)
		}
	}()
	defer grpcServer.Stop()

	// Override Agent address in deploy handler to use bufconn
	// (In production, this would use service discovery)

	// --- Setup: Coordinator Deploy Handler ---
	scheduler := gpu.NewScheduler()
	deployHandler := api.NewDeployHandler(nodeStore, scheduler)

	// Inject mock Agent client factory
	deployHandler.AgentClientFactory = func(ctx context.Context, rigID string) (pb.CoordinatorClient, func() error, error) {
		conn, err := grpc.DialContext(ctx, "",
			grpc.WithContextDialer(func(context.Context, string) (net.Conn, error) {
				return listener.Dial()
			}),
			grpc.WithTransportCredentials(insecure.NewCredentials()))
		if err != nil {
			return nil, nil, err
		}
		client := pb.NewCoordinatorClient(conn)
		return client, conn.Close, nil
	}

	// --- Test: Submit adep.json for LLaMA-3 inference ---
	manifest := map[string]interface{}{
		"name": "llama3-inference",
		"scheduling": map[string]interface{}{
			"gpu": map[string]interface{}{
				"vram_min_gb":      30,
				"cuda_version_min": "12.0",
			},
		},
		"compute": map[string]interface{}{
			"image": "vllm/vllm-openai:latest",
			"args":  []string{"--model", "/models/llama-3-70b.gguf"},
			"env":   []string{"VLLM_LOGGING_LEVEL=INFO"},
		},
		"volumes": []map[string]interface{}{
			{
				"type":        "bind",
				"source":      "/mnt/models/llama-3-70b.gguf",
				"destination": "/models/llama-3-70b.gguf",
				"readonly":    true,
			},
		},
	}

	manifestJSON, err := json.Marshal(manifest)
	require.NoError(t, err)

	// Create HTTP request
	req := httptest.NewRequest(http.MethodPost, "/deploy", bytes.NewReader(manifestJSON))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	// Execute deployment
	deployHandler.HandleDeploy(w, req)

	// --- Assertions: HTTP Response ---
	resp := w.Result()
	assert.Equal(t, http.StatusOK, resp.StatusCode, "Deployment should succeed")

	body, err := io.ReadAll(resp.Body)
	require.NoError(t, err)

	var deployResp map[string]interface{}
	err = json.Unmarshal(body, &deployResp)
	require.NoError(t, err)

	assert.True(t, deployResp["success"].(bool), "Deployment should be successful")
	rigID := deployResp["rig_id"].(string)

	// --- Assertions: Scheduler Decision (Week 2 verification) ---
	// Expected: Rig B should be selected (BestFit strategy)
	//
	// Before deployment:
	// - Rig A: 56 GB available / 96 GB total = 58.3% free
	// - Rig B: 38 GB available / 48 GB total = 79.2% free
	// - Rig C: 20 GB available / 320 GB total = 6.25% free (insufficient for 30GB)
	//
	// After deployment (30 GB request):
	// - Rig A: 26 GB available / 96 GB total = 27.1% free → 72.9% utilization
	// - Rig B: 8 GB available / 48 GB total = 16.7% free → 83.3% utilization ✓ BEST FIT
	//
	// BestFit (MostAllocated) prefers Rig B because it achieves higher utilization (83.3% > 72.9%)
	assert.Equal(t, "rig-b", rigID, "Scheduler should select Rig B (BestFit strategy)")

	// --- Assertions: VRAM Reservation (Week 4 verification) ---
	ctx := context.Background()
	rigs, err := nodeStore.GetAllGpuRigs(ctx)
	require.NoError(t, err)

	var rigB *gpu.RigGpuInfo
	for _, rig := range rigs {
		if rig.RigID == "rig-b" {
			rigB = rig
			break
		}
	}
	require.NotNil(t, rigB, "Rig B should exist in database")

	expectedUsedVRAM := (10 + 30) * 1024 * 1024 * 1024 // 10 GB initial + 30 GB reserved = 40 GB
	assert.Equal(t, uint64(expectedUsedVRAM), rigB.UsedVRAMBytes,
		"Rig B should have 40 GB VRAM used (10 GB initial + 30 GB reserved)")

	// --- Assertions: Agent DeployWorkload RPC (Week 3-4 verification) ---
	assert.Equal(t, 1, len(mockAgent.deployRequests),
		"Agent should receive exactly 1 deployment request")

	if len(mockAgent.deployRequests) > 0 {
		deployReq := mockAgent.deployRequests[0]

		// Verify manifest JSON is valid
		var receivedManifest map[string]interface{}
		err = json.Unmarshal([]byte(deployReq.ManifestJson), &receivedManifest)
		require.NoError(t, err, "Agent should receive valid manifest JSON")

		// Verify manifest content
		assert.Equal(t, "llama3-inference", receivedManifest["name"],
			"Agent should receive correct workload name")
		computeMap := receivedManifest["compute"].(map[string]interface{})
		assert.Equal(t, "vllm/vllm-openai:latest", computeMap["image"],
			"Agent should receive correct container image")
	}

	t.Log("✅ Week 4 E2E simulation test passed!")
	t.Logf("  Selected Rig: %s", rigID)
	t.Logf("  VRAM Reserved: 30 GB")
	t.Logf("  Total Used on Rig B: %.2f GB / %.2f GB",
		float64(rigB.UsedVRAMBytes)/(1024*1024*1024),
		float64(rigB.TotalVRAMBytes)/(1024*1024*1024))
	t.Logf("  Agent received %d deployment request(s)", len(mockAgent.deployRequests))
}

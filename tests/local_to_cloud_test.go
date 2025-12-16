package e2e

import (
"context"
"fmt"
"testing"
"time"

"github.com/onescluster/coordinator/pkg/capsule"
"github.com/onescluster/coordinator/pkg/cloud"
"github.com/onescluster/coordinator/pkg/hardware"
"github.com/onescluster/coordinator/pkg/router"
"github.com/onescluster/coordinator/pkg/store"
"github.com/stretchr/testify/assert"
"github.com/stretchr/testify/require"
)

// TestLocalToCloudFallback tests automatic routing from local to cloud
// when VRAM is insufficient.
func TestLocalToCloudFallback(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping E2E test in short mode")
	}

	ctx := context.Background()

	// Setup: Create in-memory store
	tempDB := t.TempDir() + "/test.db"
	capsuleStore, err := store.NewSQLiteStore(tempDB)
	require.NoError(t, err)
	defer capsuleStore.Close()

	// Setup: Hardware monitor
	monitor := hardware.NewDarwinMonitor()

	// Setup: Cloud client (using test endpoint)
	cloudClient := cloud.NewClient(cloud.WithBaseURL("http://localhost:8000"))

	// Setup: Router
	r := router.NewRouter(capsuleStore, monitor)

	// Test Capsule: Heavy inference (requires 80GB VRAM)
	heavyCapsule := &capsule.Manifest{
		SchemaVersion: "1.0",
		Name:          "vllm-qwen3-72b",
		Version:       "1.0.0",
		Type:          "inference",
		Metadata: capsule.Metadata{
			DisplayName: "Qwen3 72B (vLLM)",
			Description: "Large language model",
		},
		Requirements: capsule.Requirements{
			Platform:   []string{"linux-amd64"},
			VRAMMin:    "80GB",
			Disk:       "150GB",
		},
		Execution: capsule.Execution{
			Runtime:    "docker",
			Entrypoint: "vllm",
			Port:       8000,
		},
		Routing: &capsule.Routing{
			Weight:           "heavy",
			FallbackToCloud:  true,
			CloudCapsule:     "vllm-qwen3-72b",
		},
	}

	// Store capsule
	err = capsuleStore.Insert(heavyCapsule)
	require.NoError(t, err)

	t.Run("Heavy capsule routes to cloud", func(t *testing.T) {
decision, err := r.Decide(ctx, "vllm-qwen3-72b")
require.NoError(t, err)
assert.Equal(t, router.RouteCloud, decision)
})

	t.Run("Light capsule with insufficient VRAM falls back to cloud", func(t *testing.T) {
// Create a light capsule that requires more VRAM than available
lightCapsule := &capsule.Manifest{
			SchemaVersion: "1.0",
			Name:          "test-light-high-vram",
			Version:       "1.0.0",
			Type:          "inference",
			Requirements: capsule.Requirements{
				Platform:   []string{"darwin-arm64"},
				VRAMMin:    "100GB", // More than Mac has
			},
			Execution: capsule.Execution{
				Runtime:    "python-uv",
				Entrypoint: "server.py",
				Port:       8081,
			},
			Routing: &capsule.Routing{
				Weight:           "light",
				FallbackToCloud:  true,
				CloudCapsule:     "cloud-backup",
			},
		}

		err := capsuleStore.Insert(lightCapsule)
		require.NoError(t, err)

		decision, err := r.Decide(ctx, "test-light-high-vram")
		require.NoError(t, err)
		assert.Equal(t, router.RouteCloud, decision)
	})

	t.Run("Light capsule with sufficient VRAM routes locally", func(t *testing.T) {
// Create a capsule with low requirements
lowVramCapsule := &capsule.Manifest{
			SchemaVersion: "1.0",
			Name:          "test-light-low-vram",
			Version:       "1.0.0",
			Type:          "inference",
			Requirements: capsule.Requirements{
				Platform:   []string{"darwin-arm64"},
				VRAMMin:    "2GB", // Should fit on most Macs
			},
			Execution: capsule.Execution{
				Runtime:    "python-uv",
				Entrypoint: "server.py",
				Port:       8081,
			},
			Routing: &capsule.Routing{
				Weight:           "light",
				FallbackToCloud:  true,
			},
		}

		err := capsuleStore.Insert(lowVramCapsule)
		require.NoError(t, err)

		decision, err := r.Decide(ctx, "test-light-low-vram")
		require.NoError(t, err)
		assert.Equal(t, router.RouteLocal, decision)
	})

	t.Run("No fallback option returns error when resources insufficient", func(t *testing.T) {
noFallbackCapsule := &capsule.Manifest{
			SchemaVersion: "1.0",
			Name:          "test-no-fallback",
			Version:       "1.0.0",
			Type:          "inference",
			Requirements: capsule.Requirements{
				Platform:   []string{"darwin-arm64"},
				VRAMMin:    "100GB",
			},
			Execution: capsule.Execution{
				Runtime:    "python-uv",
				Entrypoint: "server.py",
				Port:       8081,
			},
			Routing: &capsule.Routing{
				Weight:           "light",
				FallbackToCloud:  false, // No fallback
			},
		}

		err := capsuleStore.Insert(noFallbackCapsule)
		require.NoError(t, err)

		_, err = r.Decide(ctx, "test-no-fallback")
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "insufficient")
	})
}

// TestCloudEndpointHealthCheck tests cloud endpoint availability monitoring
func TestCloudEndpointHealthCheck(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping E2E test in short mode")
	}

	t.Run("Healthy endpoint returns success", func(t *testing.T) {
// Mock healthy endpoint
client := cloud.NewClient(cloud.WithBaseURL("http://localhost:8000"))

ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()

		healthy, latency, err := client.Health(ctx)
		
		// If no server is running, this is expected to fail
		if err != nil {
			t.Skip("Cloud endpoint not available (expected in CI)")
			return
		}

		assert.True(t, healthy)
		assert.Greater(t, latency, time.Duration(0))
		t.Logf("Endpoint latency: %v", latency)
	})

	t.Run("Unreachable endpoint returns error", func(t *testing.T) {
// Invalid endpoint
client := cloud.NewClient(cloud.WithBaseURL("http://localhost:9999"))

ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()

		healthy, _, err := client.Health(ctx)
		
		assert.False(t, healthy)
		assert.Error(t, err)
	})
}

// TestEndpointDiscovery tests automatic discovery of cloud endpoints
func TestEndpointDiscovery(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping E2E test in short mode")
	}

	// Create discovery service
	discoveryURL := "http://localhost:8080/v1/endpoints"
	discovery := router.NewEndpointDiscovery(discoveryURL, 10*time.Second)

	t.Run("Initial discovery", func(t *testing.T) {
ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()

		err := discovery.DiscoverOnce(ctx)
		
		// May fail if discovery service not running
		if err != nil {
			t.Logf("Discovery failed (expected if service not running): %v", err)
		}
	})

	t.Run("Background discovery", func(t *testing.T) {
ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()

		done := make(chan error, 1)
		go func() {
			done <- discovery.Start(ctx)
		}()

		// Wait a bit for discovery attempts
		time.Sleep(3 * time.Second)

		cancel()
		err := <-done
		
		// Context cancellation is expected
		if err != nil && err != context.Canceled {
			t.Logf("Discovery stopped with error: %v", err)
		}
	})
}

// TestRouterStateTracking tests route decision tracking
func TestRouterStateTracking(t *testing.T) {
	ctx := context.Background()

	// Setup
	tempDB := t.TempDir() + "/test.db"
	capsuleStore, err := store.NewSQLiteStore(tempDB)
	require.NoError(t, err)
	defer capsuleStore.Close()

	monitor := hardware.NewDarwinMonitor()
	r := router.NewRouter(capsuleStore, monitor)

	// Add test capsule
	testCapsule := &capsule.Manifest{
		SchemaVersion: "1.0",
		Name:          "test-tracking",
		Version:       "1.0.0",
		Type:          "inference",
		Requirements: capsule.Requirements{
			Platform: []string{"darwin-arm64"},
			VRAMMin:  "2GB",
		},
		Execution: capsule.Execution{
			Runtime:    "python-uv",
			Entrypoint: "server.py",
			Port:       8081,
		},
		Routing: &capsule.Routing{
			Weight:          "light",
			FallbackToCloud: true,
		},
	}

	err = capsuleStore.Insert(testCapsule)
	require.NoError(t, err)

	t.Run("Route decision is recorded", func(t *testing.T) {
decision, err := r.Decide(ctx, "test-tracking")
require.NoError(t, err)

// Record decision
err = capsuleStore.RecordRouteDecision("test-tracking", string(decision), "test")
require.NoError(t, err)

// Verify it was recorded
decisions, err := capsuleStore.GetRouteDecisions("test-tracking", 10)
require.NoError(t, err)
assert.NotEmpty(t, decisions)

lastDecision := decisions[0]
assert.Equal(t, string(decision), lastDecision.Decision)
assert.Equal(t, "test", lastDecision.Reason)
})

	t.Run("Multiple decisions are tracked", func(t *testing.T) {
// Make multiple decisions
for i := 0; i < 5; i++ {
			decision, err := r.Decide(ctx, "test-tracking")
			require.NoError(t, err)

			err = capsuleStore.RecordRouteDecision(
"test-tracking",
string(decision),
fmt.Sprintf("test-%d", i),
)
			require.NoError(t, err)
		}

		// Verify all were recorded
		decisions, err := capsuleStore.GetRouteDecisions("test-tracking", 10)
		require.NoError(t, err)
		assert.GreaterOrEqual(t, len(decisions), 5)
	})
}

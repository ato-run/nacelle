package router

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/capsule"
	"github.com/onescluster/coordinator/pkg/cloud"
	"github.com/onescluster/coordinator/pkg/hardware"
	"github.com/onescluster/coordinator/pkg/registry"
)

func TestEndpointPoolAddRemove(t *testing.T) {
	pool := NewEndpointPool()

	// Add endpoints
	pool.AddEndpoint(cloud.Endpoint{URL: "http://a.example.com", Healthy: true})
	pool.AddEndpoint(cloud.Endpoint{URL: "http://b.example.com", Healthy: true})

	endpoints := pool.GetAllEndpoints()
	if len(endpoints) != 2 {
		t.Errorf("expected 2 endpoints, got %d", len(endpoints))
	}

	// Add duplicate (should update, not add)
	pool.AddEndpoint(cloud.Endpoint{URL: "http://a.example.com", Healthy: false})
	endpoints = pool.GetAllEndpoints()
	if len(endpoints) != 2 {
		t.Errorf("expected still 2 endpoints after duplicate add, got %d", len(endpoints))
	}

	// Remove endpoint
	pool.RemoveEndpoint("http://a.example.com")
	endpoints = pool.GetAllEndpoints()
	if len(endpoints) != 1 {
		t.Errorf("expected 1 endpoint after remove, got %d", len(endpoints))
	}
	if endpoints[0].URL != "http://b.example.com" {
		t.Errorf("expected http://b.example.com, got %s", endpoints[0].URL)
	}
}

func TestEndpointPoolRoundRobin(t *testing.T) {
	pool := NewEndpointPool()
	pool.AddEndpoint(cloud.Endpoint{URL: "http://a.example.com", Healthy: true})
	pool.AddEndpoint(cloud.Endpoint{URL: "http://b.example.com", Healthy: true})
	pool.AddEndpoint(cloud.Endpoint{URL: "http://c.example.com", Healthy: true})

	ctx := context.Background()

	// Should cycle through endpoints
	urls := make([]string, 6)
	for i := 0; i < 6; i++ {
		ep, err := pool.SelectEndpoint(ctx, "test")
		if err != nil {
			t.Fatalf("SelectEndpoint failed: %v", err)
		}
		urls[i] = ep.URL
	}

	// Should have cycled twice through a, b, c
	expected := []string{
		"http://a.example.com",
		"http://b.example.com",
		"http://c.example.com",
		"http://a.example.com",
		"http://b.example.com",
		"http://c.example.com",
	}

	for i, url := range urls {
		if url != expected[i] {
			t.Errorf("at index %d: expected %s, got %s", i, expected[i], url)
		}
	}
}

func TestEndpointPoolSkipUnhealthy(t *testing.T) {
	pool := NewEndpointPool()
	pool.AddEndpoint(cloud.Endpoint{URL: "http://a.example.com", Healthy: false})
	pool.AddEndpoint(cloud.Endpoint{URL: "http://b.example.com", Healthy: true})
	pool.AddEndpoint(cloud.Endpoint{URL: "http://c.example.com", Healthy: false})

	ctx := context.Background()

	// Should always select the healthy one
	for i := 0; i < 5; i++ {
		ep, err := pool.SelectEndpoint(ctx, "test")
		if err != nil {
			t.Fatalf("SelectEndpoint failed: %v", err)
		}
		if ep.URL != "http://b.example.com" {
			t.Errorf("expected healthy endpoint, got %s", ep.URL)
		}
	}
}

func TestEndpointPoolHealthCheck(t *testing.T) {
	// Create test servers
	healthyServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	defer healthyServer.Close()

	unhealthyServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "unhealthy", http.StatusServiceUnavailable)
	}))
	defer unhealthyServer.Close()

	pool := NewEndpointPool()
	pool.AddEndpoint(cloud.Endpoint{URL: healthyServer.URL, Healthy: false})
	pool.AddEndpoint(cloud.Endpoint{URL: unhealthyServer.URL, Healthy: true})

	// Run health check
	ctx := context.Background()
	err := pool.HealthCheck(ctx)
	if err != nil {
		t.Fatalf("HealthCheck failed: %v", err)
	}

	// Verify status updated
	endpoints := pool.GetAllEndpoints()
	for _, ep := range endpoints {
		if ep.URL == healthyServer.URL && !ep.Healthy {
			t.Error("healthy server should be marked healthy")
		}
		if ep.URL == unhealthyServer.URL && ep.Healthy {
			t.Error("unhealthy server should be marked unhealthy")
		}
	}

	if pool.HealthyCount() != 1 {
		t.Errorf("expected 1 healthy endpoint, got %d", pool.HealthyCount())
	}
}

func TestCloudRouterExecuteCloud(t *testing.T) {
	// Create mock cloud server
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/health" {
			w.WriteHeader(http.StatusOK)
			return
		}

		resp := cloud.ChatResponse{
			ID:      "test-123",
			Object:  "chat.completion",
			Created: time.Now().Unix(),
			Model:   "test-model",
			Choices: []cloud.Choice{
				{
					Index: 0,
					Message: cloud.ChatMessage{
						Role:    "assistant",
						Content: "Cloud response",
					},
					FinishReason: "stop",
				},
			},
		}
		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}))
	defer server.Close()

	// Create mock router with proper mocks from router_test.go
	mockStore := &MockCapsuleStore{
		capsules: make(map[string]*capsule.CapsuleManifest),
	}
	mockMonitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     32 * 1024 * 1024 * 1024,
			AvailableVRAM: 24 * 1024 * 1024 * 1024,
			TotalRAM:      64 * 1024 * 1024 * 1024,
			AvailableRAM:  32 * 1024 * 1024 * 1024,
		},
	}

	baseRouter := NewRouter(mockMonitor, mockStore, DefaultConfig())
	cloudRouter := NewCloudRouter(baseRouter)

	// Add cloud endpoint
	cloudRouter.AddEndpoint(cloud.Endpoint{
		URL:     server.URL,
		Model:   "test-model",
		Healthy: true,
	})

	// Create cloud decision
	decision := &Decision{
		Route:       RouteCloud,
		CapsuleName: "test-capsule",
	}

	// Execute on cloud
	ctx := context.Background()
	resp, err := cloudRouter.ExecuteCloud(ctx, decision, cloud.ChatRequest{
		Messages: []cloud.ChatMessage{
			{Role: "user", Content: "Hello"},
		},
	})
	if err != nil {
		t.Fatalf("ExecuteCloud failed: %v", err)
	}

	if resp.Choices[0].Message.Content != "Cloud response" {
		t.Errorf("unexpected response: %s", resp.Choices[0].Message.Content)
	}
}

func TestCloudRouterExecuteCloudLocalDecision(t *testing.T) {
	baseRouter := NewRouter(nil, nil, DefaultConfig())
	cloudRouter := NewCloudRouter(baseRouter)

	decision := &Decision{
		Route:       RouteLocal,
		CapsuleName: "test-capsule",
	}

	ctx := context.Background()
	_, err := cloudRouter.ExecuteCloud(ctx, decision, cloud.ChatRequest{})
	if err == nil {
		t.Error("expected error for local decision")
	}
}

func TestCloudRouterNoEndpoints(t *testing.T) {
	baseRouter := NewRouter(nil, nil, DefaultConfig())
	cloudRouter := NewCloudRouter(baseRouter)

	decision := &Decision{
		Route:       RouteCloud,
		CapsuleName: "test-capsule",
	}

	ctx := context.Background()
	_, err := cloudRouter.ExecuteCloud(ctx, decision, cloud.ChatRequest{})
	if err == nil {
		t.Error("expected error when no endpoints available")
	}
}

func TestCloudRouterGetEndpoints(t *testing.T) {
	baseRouter := NewRouter(nil, nil, DefaultConfig())
	cloudRouter := NewCloudRouter(baseRouter)

	cloudRouter.AddEndpoint(cloud.Endpoint{URL: "http://a.example.com"})
	cloudRouter.AddEndpoint(cloud.Endpoint{URL: "http://b.example.com"})

	endpoints := cloudRouter.GetEndpoints()
	if len(endpoints) != 2 {
		t.Errorf("expected 2 endpoints, got %d", len(endpoints))
	}

	cloudRouter.RemoveEndpoint("http://a.example.com")
	endpoints = cloudRouter.GetEndpoints()
	if len(endpoints) != 1 {
		t.Errorf("expected 1 endpoint after remove, got %d", len(endpoints))
	}
}

// MockRegistryClient implements registry.Client for testing
type MockRegistryClient struct {
	capsules      []registry.CapsuleSummary
	downloadInfo  map[string]*registry.DownloadInfo
	listError     error
	downloadError error
}

func (m *MockRegistryClient) List(ctx context.Context, opts registry.ListOptions) (*registry.CapsuleListResponse, error) {
	if m.listError != nil {
		return nil, m.listError
	}
	return &registry.CapsuleListResponse{
		Capsules: m.capsules,
		Total:    len(m.capsules),
	}, nil
}

func (m *MockRegistryClient) Get(ctx context.Context, name string) (*capsule.CapsuleManifest, error) {
	return nil, nil
}

func (m *MockRegistryClient) GetVersion(ctx context.Context, name, version string) (*capsule.CapsuleManifest, error) {
	return nil, nil
}

func (m *MockRegistryClient) GetDownloadInfo(ctx context.Context, name, version, platform string) (*registry.DownloadInfo, error) {
	if m.downloadError != nil {
		return nil, m.downloadError
	}
	key := name + "@" + version
	if info, ok := m.downloadInfo[key]; ok {
		return info, nil
	}
	return nil, fmt.Errorf("not found")
}

func (m *MockRegistryClient) Download(ctx context.Context, name, version, destPath string) error {
	return nil
}

func (m *MockRegistryClient) ListVersions(ctx context.Context, name string) (*registry.VersionListResponse, error) {
	return nil, nil
}

func TestEndpointDiscoveryDiscoverOnce(t *testing.T) {
	// Create mock registry with cloud endpoints
	mockRegistry := &MockRegistryClient{
		capsules: []registry.CapsuleSummary{
			{Name: "vllm-qwen3-8b", Version: "1.0.0", Type: "inference"},
			{Name: "mlx-local", Version: "1.0.0", Type: "inference"},
			{Name: "some-tool", Version: "1.0.0", Type: "tool"}, // Should be skipped
		},
		downloadInfo: map[string]*registry.DownloadInfo{
			"vllm-qwen3-8b@1.0.0": {
				URL:           "https://registry.example.com/vllm-qwen3-8b.tar.gz",
				CloudEndpoint: "https://api.cloud.example.com/v1",
			},
			"mlx-local@1.0.0": {
				URL: "https://registry.example.com/mlx-local.tar.gz",
				// No CloudEndpoint - local only
			},
		},
	}

	pool := NewEndpointPool()
	discovery := NewEndpointDiscovery(mockRegistry, pool)

	ctx := context.Background()
	err := discovery.DiscoverOnce(ctx)
	if err != nil {
		t.Fatalf("DiscoverOnce failed: %v", err)
	}

	// Should have discovered one cloud endpoint
	endpoints := pool.GetAllEndpoints()
	if len(endpoints) != 1 {
		t.Errorf("expected 1 cloud endpoint, got %d", len(endpoints))
	}

	if endpoints[0].URL != "https://api.cloud.example.com/v1" {
		t.Errorf("unexpected endpoint URL: %s", endpoints[0].URL)
	}

	if endpoints[0].Model != "vllm-qwen3-8b" {
		t.Errorf("unexpected endpoint model: %s", endpoints[0].Model)
	}
}

func TestEndpointDiscoveryStartStop(t *testing.T) {
	mockRegistry := &MockRegistryClient{
		capsules: []registry.CapsuleSummary{},
	}

	pool := NewEndpointPool()
	discovery := NewEndpointDiscovery(mockRegistry, pool)
	discovery.SetInterval(100 * time.Millisecond)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Start discovery
	err := discovery.Start(ctx)
	if err != nil {
		t.Fatalf("Start failed: %v", err)
	}

	// Starting again should fail
	err = discovery.Start(ctx)
	if err == nil {
		t.Error("expected error when starting already running discovery")
	}

	// Stop discovery
	discovery.Stop()

	// Can start again after stop
	err = discovery.Start(ctx)
	if err != nil {
		t.Fatalf("Start after stop failed: %v", err)
	}

	discovery.Stop()
}

func TestCloudRouterWithDiscovery(t *testing.T) {
	mockRegistry := &MockRegistryClient{
		capsules: []registry.CapsuleSummary{
			{Name: "cloud-model", Version: "1.0.0", Type: "inference"},
		},
		downloadInfo: map[string]*registry.DownloadInfo{
			"cloud-model@1.0.0": {
				CloudEndpoint: "https://cloud.example.com/v1",
			},
		},
	}

	baseRouter := NewRouter(nil, nil, DefaultConfig())
	cloudRouter := NewCloudRouter(baseRouter)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Start discovery
	discovery, err := cloudRouter.StartDiscovery(ctx, mockRegistry)
	if err != nil {
		t.Fatalf("StartDiscovery failed: %v", err)
	}
	defer discovery.Stop()

	// Wait for initial discovery
	time.Sleep(50 * time.Millisecond)

	// Check endpoints were discovered
	endpoints := cloudRouter.GetEndpoints()
	if len(endpoints) != 1 {
		t.Errorf("expected 1 endpoint, got %d", len(endpoints))
	}
}

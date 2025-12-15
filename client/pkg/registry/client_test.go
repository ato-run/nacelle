package registry

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/capsule"
)

// mockCapsule returns a sample capsule manifest for testing.
func mockCapsule() capsule.CapsuleManifest {
	return capsule.CapsuleManifest{
		SchemaVersion: "1.0",
		Name:          "test-capsule",
		Version:       "1.0.0",
		Type:          capsule.TypeInference,
		Metadata: capsule.Metadata{
			DisplayName: "Test Capsule",
			Description: "A test capsule",
			Author:      "test",
			Tags:        []string{"test"},
		},
		Requirements: capsule.Requirements{
			Platform: []capsule.Platform{capsule.PlatformDarwinArm64},
		},
		Execution: capsule.Execution{
			Runtime:    capsule.RuntimePythonUv,
			Entrypoint: "server.py",
			Port:       8080,
		},
	}
}

func TestList(t *testing.T) {
	// Setup mock server
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/capsules" {
			t.Errorf("unexpected path: %s", r.URL.Path)
			http.NotFound(w, r)
			return
		}

		resp := CapsuleListResponse{
			Capsules: []CapsuleSummary{
				{
					Name:        "test-capsule",
					Version:     "1.0.0",
					Type:        "inference",
					DisplayName: "Test Capsule",
					Description: "A test capsule",
				},
			},
			Total:  1,
			Limit:  50,
			Offset: 0,
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	// Create client
	client := NewClient(WithBaseURL(server.URL))

	// Test list
	ctx := context.Background()
	result, err := client.List(ctx, ListOptions{})
	if err != nil {
		t.Fatalf("List failed: %v", err)
	}

	if len(result.Capsules) != 1 {
		t.Errorf("expected 1 capsule, got %d", len(result.Capsules))
	}
	if result.Capsules[0].Name != "test-capsule" {
		t.Errorf("expected name 'test-capsule', got '%s'", result.Capsules[0].Name)
	}
}

func TestListWithFilters(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify query params
		q := r.URL.Query()
		if q.Get("q") != "qwen" {
			t.Errorf("expected q=qwen, got %s", q.Get("q"))
		}
		if q.Get("type") != "inference" {
			t.Errorf("expected type=inference, got %s", q.Get("type"))
		}
		if q.Get("platform") != "darwin-arm64" {
			t.Errorf("expected platform=darwin-arm64, got %s", q.Get("platform"))
		}

		resp := CapsuleListResponse{Capsules: []CapsuleSummary{}}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	client := NewClient(WithBaseURL(server.URL))
	_, err := client.List(context.Background(), ListOptions{
		Query:    "qwen",
		Type:     "inference",
		Platform: "darwin-arm64",
	})
	if err != nil {
		t.Fatalf("List failed: %v", err)
	}
}

func TestGet(t *testing.T) {
	manifest := mockCapsule()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/capsules/test-capsule" {
			http.NotFound(w, r)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(manifest)
	}))
	defer server.Close()

	client := NewClient(WithBaseURL(server.URL))
	result, err := client.Get(context.Background(), "test-capsule")
	if err != nil {
		t.Fatalf("Get failed: %v", err)
	}

	if result.Name != "test-capsule" {
		t.Errorf("expected name 'test-capsule', got '%s'", result.Name)
	}
	if result.Version != "1.0.0" {
		t.Errorf("expected version '1.0.0', got '%s'", result.Version)
	}
}

func TestGetNotFound(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.NotFound(w, r)
	}))
	defer server.Close()

	client := NewClient(WithBaseURL(server.URL))
	_, err := client.Get(context.Background(), "nonexistent")
	if err == nil {
		t.Error("expected error for nonexistent capsule")
	}
}

func TestGetDownloadInfo(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/capsules/test-capsule/download" {
			http.NotFound(w, r)
			return
		}

		info := DownloadInfo{
			URL:       "https://example.com/download/test-capsule.tar.gz",
			Checksum:  "sha256:abc123",
			SizeBytes: 1024000,
			ExpiresAt: time.Now().Add(1 * time.Hour).Format(time.RFC3339),
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(info)
	}))
	defer server.Close()

	client := NewClient(WithBaseURL(server.URL))
	info, err := client.GetDownloadInfo(context.Background(), "test-capsule", "1.0.0", "darwin-arm64")
	if err != nil {
		t.Fatalf("GetDownloadInfo failed: %v", err)
	}

	if info.URL == "" {
		t.Error("expected non-empty URL")
	}
	if info.Checksum != "sha256:abc123" {
		t.Errorf("expected checksum 'sha256:abc123', got '%s'", info.Checksum)
	}
}

func TestListVersions(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/capsules/test-capsule/versions" {
			http.NotFound(w, r)
			return
		}

		resp := VersionListResponse{
			Name: "test-capsule",
			Versions: []VersionInfo{
				{Version: "1.0.0", IsLatest: true},
				{Version: "0.9.0", IsLatest: false},
			},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	client := NewClient(WithBaseURL(server.URL))
	result, err := client.ListVersions(context.Background(), "test-capsule")
	if err != nil {
		t.Fatalf("ListVersions failed: %v", err)
	}

	if len(result.Versions) != 2 {
		t.Errorf("expected 2 versions, got %d", len(result.Versions))
	}
	if !result.Versions[0].IsLatest {
		t.Error("expected first version to be latest")
	}
}

func TestCaching(t *testing.T) {
	callCount := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		callCount++
		resp := CapsuleListResponse{Capsules: []CapsuleSummary{}}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	client := NewClient(
		WithBaseURL(server.URL),
		WithCacheTTL(1*time.Minute),
	)

	ctx := context.Background()

	// First call should hit server
	_, _ = client.List(ctx, ListOptions{})
	if callCount != 1 {
		t.Errorf("expected 1 call, got %d", callCount)
	}

	// Second call should use cache
	_, _ = client.List(ctx, ListOptions{})
	if callCount != 1 {
		t.Errorf("expected still 1 call (cached), got %d", callCount)
	}

	// Different query should hit server again
	_, _ = client.List(ctx, ListOptions{Query: "different"})
	if callCount != 2 {
		t.Errorf("expected 2 calls, got %d", callCount)
	}
}

func TestClientOptions(t *testing.T) {
	client := NewClient(
		WithBaseURL("https://custom.registry.com"),
		WithTimeout(60*time.Second),
		WithCacheTTL(10*time.Minute),
	)

	if client.baseURL != "https://custom.registry.com" {
		t.Errorf("unexpected baseURL: %s", client.baseURL)
	}
	if client.httpClient.Timeout != 60*time.Second {
		t.Errorf("unexpected timeout: %v", client.httpClient.Timeout)
	}
	if client.cacheTTL != 10*time.Minute {
		t.Errorf("unexpected cacheTTL: %v", client.cacheTTL)
	}
}

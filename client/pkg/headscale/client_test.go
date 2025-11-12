package headscale

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestListNodes(t *testing.T) {
	// Create test data
	testNodes := []Node{
		{
			ID:       "1",
			Name:     "node-1",
			User:     "admin",
			Online:   true,
			LastSeen: time.Now(),
		},
		{
			ID:       "2",
			Name:     "node-2",
			User:     "admin",
			Online:   false,
			LastSeen: time.Now().Add(-1 * time.Hour),
		},
	}

	// Create mock server
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Verify request
		if r.Method != http.MethodGet {
			t.Errorf("Expected GET request, got %s", r.Method)
		}

		if r.URL.Path != "/api/v1/node" {
			t.Errorf("Expected path /api/v1/node, got %s", r.URL.Path)
		}

		// Verify authentication header
		authHeader := r.Header.Get("Authorization")
		if authHeader != "Bearer test-api-key" {
			t.Errorf("Expected Bearer token, got %s", authHeader)
		}

		// Send response
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		
		response := ListNodesResponse{Nodes: testNodes}
		if err := json.NewEncoder(w).Encode(response); err != nil {
			t.Fatalf("Failed to encode response: %v", err)
		}
	}))
	defer server.Close()

	// Create client
	client := NewClient(server.URL, "test-api-key", 5*time.Second)

	// Test ListNodes
	ctx := context.Background()
	nodes, err := client.ListNodes(ctx)

	if err != nil {
		t.Fatalf("ListNodes failed: %v", err)
	}

	if len(nodes) != len(testNodes) {
		t.Errorf("Expected %d nodes, got %d", len(testNodes), len(nodes))
	}

	for i, node := range nodes {
		if node.ID != testNodes[i].ID {
			t.Errorf("Node %d: expected ID %s, got %s", i, testNodes[i].ID, node.ID)
		}
		if node.Name != testNodes[i].Name {
			t.Errorf("Node %d: expected name %s, got %s", i, testNodes[i].Name, node.Name)
		}
		if node.Online != testNodes[i].Online {
			t.Errorf("Node %d: expected online %v, got %v", i, testNodes[i].Online, node.Online)
		}
	}
}

func TestListNodesAPIError(t *testing.T) {
	tests := []struct {
		name           string
		statusCode     int
		responseBody   string
		expectedErrMsg string
	}{
		{
			name:           "unauthorized",
			statusCode:     http.StatusUnauthorized,
			responseBody:   "Unauthorized",
			expectedErrMsg: "API returned status 401",
		},
		{
			name:           "internal server error",
			statusCode:     http.StatusInternalServerError,
			responseBody:   "Internal Server Error",
			expectedErrMsg: "API returned status 500",
		},
		{
			name:           "not found",
			statusCode:     http.StatusNotFound,
			responseBody:   "Not Found",
			expectedErrMsg: "API returned status 404",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(tt.statusCode)
				w.Write([]byte(tt.responseBody))
			}))
			defer server.Close()

			client := NewClient(server.URL, "test-api-key", 5*time.Second)
			ctx := context.Background()

			_, err := client.ListNodes(ctx)

			if err == nil {
				t.Error("Expected error but got none")
				return
			}

			if !containsString(err.Error(), tt.expectedErrMsg) {
				t.Errorf("Expected error containing '%s', got '%s'", tt.expectedErrMsg, err.Error())
			}
		})
	}
}

func TestListNodesNetworkError(t *testing.T) {
	// Create client with invalid URL
	client := NewClient("http://invalid-host-that-does-not-exist:9999", "test-api-key", 1*time.Second)
	ctx := context.Background()

	_, err := client.ListNodes(ctx)

	if err == nil {
		t.Error("Expected network error but got none")
	}
}

func TestGetNodeByName(t *testing.T) {
	testNodes := []Node{
		{ID: "1", Name: "coordinator-1", User: "admin", Online: true},
		{ID: "2", Name: "coordinator-2", User: "admin", Online: true},
		{ID: "3", Name: "coordinator-3", User: "admin", Online: false},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		
		response := ListNodesResponse{Nodes: testNodes}
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-api-key", 5*time.Second)
	ctx := context.Background()

	// Test finding existing node
	node, err := client.GetNodeByName(ctx, "coordinator-2")
	if err != nil {
		t.Fatalf("GetNodeByName failed: %v", err)
	}

	if node.ID != "2" {
		t.Errorf("Expected node ID 2, got %s", node.ID)
	}

	if node.Name != "coordinator-2" {
		t.Errorf("Expected node name coordinator-2, got %s", node.Name)
	}

	// Test finding non-existent node
	_, err = client.GetNodeByName(ctx, "non-existent-node")
	if err == nil {
		t.Error("Expected error for non-existent node, but got none")
	}

	if !containsString(err.Error(), "not found") {
		t.Errorf("Expected 'not found' error, got: %s", err.Error())
	}
}

func TestGetQuorumSize(t *testing.T) {
	tests := []struct {
		name          string
		nodeCount     int
		expectedQuorum int
	}{
		{
			name:          "3 nodes",
			nodeCount:     3,
			expectedQuorum: 3,
		},
		{
			name:          "5 nodes",
			nodeCount:     5,
			expectedQuorum: 5,
		},
		{
			name:          "1 node",
			nodeCount:     1,
			expectedQuorum: 1,
		},
		{
			name:          "0 nodes",
			nodeCount:     0,
			expectedQuorum: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Generate test nodes
			nodes := make([]Node, tt.nodeCount)
			for i := 0; i < tt.nodeCount; i++ {
				nodes[i] = Node{
					ID:     string(rune(i + 1)),
					Name:   "node-" + string(rune(i + 1)),
					Online: true,
				}
			}

			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusOK)
				
				response := ListNodesResponse{Nodes: nodes}
				json.NewEncoder(w).Encode(response)
			}))
			defer server.Close()

			client := NewClient(server.URL, "test-api-key", 5*time.Second)
			ctx := context.Background()

			quorum, err := client.GetQuorumSize(ctx)
			if err != nil {
				t.Fatalf("GetQuorumSize failed: %v", err)
			}

			if quorum != tt.expectedQuorum {
				t.Errorf("Expected quorum size %d, got %d", tt.expectedQuorum, quorum)
			}
		})
	}
}

func TestIsHealthy(t *testing.T) {
	tests := []struct {
		name        string
		statusCode  int
		expectError bool
	}{
		{
			name:        "healthy",
			statusCode:  http.StatusOK,
			expectError: false,
		},
		{
			name:        "unhealthy - 500",
			statusCode:  http.StatusInternalServerError,
			expectError: true,
		},
		{
			name:        "unhealthy - 503",
			statusCode:  http.StatusServiceUnavailable,
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if r.URL.Path != "/health" {
					t.Errorf("Expected path /health, got %s", r.URL.Path)
				}
				w.WriteHeader(tt.statusCode)
			}))
			defer server.Close()

			client := NewClient(server.URL, "test-api-key", 5*time.Second)
			ctx := context.Background()

			err := client.IsHealthy(ctx)

			if tt.expectError && err == nil {
				t.Error("Expected error but got none")
			}

			if !tt.expectError && err != nil {
				t.Errorf("Unexpected error: %v", err)
			}
		})
	}
}

func TestClientTimeout(t *testing.T) {
	// Create server that takes 2 seconds to respond
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(2 * time.Second)
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	// Create client with 500ms timeout
	client := NewClient(server.URL, "test-api-key", 500*time.Millisecond)
	ctx := context.Background()

	start := time.Now()
	_, err := client.ListNodes(ctx)
	elapsed := time.Since(start)

	if err == nil {
		t.Error("Expected timeout error but got none")
	}

	// Should timeout in around 500ms, not wait 2 seconds
	if elapsed > 1*time.Second {
		t.Errorf("Timeout took too long: %v", elapsed)
	}
}

func TestClientContextCancellation(t *testing.T) {
	// Create server that takes 2 seconds to respond
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(2 * time.Second)
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := NewClient(server.URL, "test-api-key", 5*time.Second)
	
	// Create context that gets cancelled after 100ms
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	start := time.Now()
	_, err := client.ListNodes(ctx)
	elapsed := time.Since(start)

	if err == nil {
		t.Error("Expected context cancellation error but got none")
	}

	// Should cancel in around 100ms
	if elapsed > 500*time.Millisecond {
		t.Errorf("Context cancellation took too long: %v", elapsed)
	}
}

// Helper function
func containsString(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > len(substr) && 
		(s[:len(substr)] == substr || s[len(s)-len(substr):] == substr || 
		len(s) > len(substr)+1 && s[1:len(substr)+1] == substr))
}

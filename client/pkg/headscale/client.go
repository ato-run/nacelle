package headscale

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

// Client provides access to Headscale API
type Client struct {
	baseURL    string
	apiKey     string
	httpClient *http.Client
}

// NewClient creates a new Headscale API client
func NewClient(baseURL, apiKey string, timeout time.Duration) *Client {
	return &Client{
		baseURL: baseURL,
		apiKey:  apiKey,
		httpClient: &http.Client{
			Timeout: timeout,
		},
	}
}

// Node represents a node in the Headscale mesh network
type Node struct {
	ID       string    `json:"id"`
	Name     string    `json:"name"`
	User     string    `json:"user"`
	Online   bool      `json:"online"`
	LastSeen time.Time `json:"lastSeen"`
}

// ListNodesResponse represents the response from ListNodes API
type ListNodesResponse struct {
	Nodes []Node `json:"nodes"`
}

// ListNodes retrieves all nodes registered in Headscale
// This is used to determine the quorum for master election
func (c *Client) ListNodes(ctx context.Context) ([]Node, error) {
	url := fmt.Sprintf("%s/api/v1/node", c.baseURL)

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}

	// Add authentication header
	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", c.apiKey))
	req.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("failed to execute request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("API returned status %d: %s", resp.StatusCode, string(body))
	}

	var response ListNodesResponse
	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return response.Nodes, nil
}

// GetNodeByName retrieves a specific node by its name
func (c *Client) GetNodeByName(ctx context.Context, name string) (*Node, error) {
	nodes, err := c.ListNodes(ctx)
	if err != nil {
		return nil, err
	}

	for _, node := range nodes {
		if node.Name == name {
			return &node, nil
		}
	}

	return nil, fmt.Errorf("node %s not found", name)
}

// GetQuorumSize returns the number of nodes that should be considered for quorum
// This includes all nodes registered in Headscale, regardless of their online status
func (c *Client) GetQuorumSize(ctx context.Context) (int, error) {
	nodes, err := c.ListNodes(ctx)
	if err != nil {
		return 0, fmt.Errorf("failed to get quorum size: %w", err)
	}

	return len(nodes), nil
}

// IsHealthy performs a health check against the Headscale API
func (c *Client) IsHealthy(ctx context.Context) error {
	url := fmt.Sprintf("%s/health", c.baseURL)

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return fmt.Errorf("failed to create health check request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("health check failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("health check returned status %d", resp.StatusCode)
	}

	return nil
}

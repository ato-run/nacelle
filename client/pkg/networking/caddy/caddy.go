package caddy

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

// Client for Caddy Admin API
type Client struct {
	BaseURL    string
	HTTPClient *http.Client
}

// NewClient creates a new Caddy Admin API client
func NewClient(adminURL string) *Client {
	if adminURL == "" {
		adminURL = "http://localhost:2019"
	}
	return &Client{
		BaseURL: adminURL,
		HTTPClient: &http.Client{
			Timeout: 5 * time.Second,
		},
	}
}

// EnsureBaseConfig makes sure the HTTP app and server are configured.
// This is a idempotent operation to set up the skeleton if it's missing.
func (c *Client) EnsureBaseConfig() error {
	// Check if srv0 exists
	resp, err := c.HTTPClient.Get(c.BaseURL + "/config/apps/http/servers/srv0")
	if err != nil {
		return fmt.Errorf("failed to check caddy config: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == 200 {
		return nil // Already exists
	}

	// Minimal config to allow HTTP on port 80 (or 8080 if running non-root)
	// We'll bind to :80 for now, assuming user has permissions or port forwarding.
	baseConfig := map[string]interface{}{
		"listen": []string{":80"}, // Standard HTTP port
		"routes": []interface{}{},  // Start empty
	}

	// Create via PUT
	jsonBody, _ := json.Marshal(baseConfig)
	req, err := http.NewRequest("POST", c.BaseURL+"/config/apps/http/servers/srv0", bytes.NewBuffer(jsonBody))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err = c.HTTPClient.Do(req)
	if err != nil {
		return fmt.Errorf("failed to initialize caddy base config: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("caddy init failed: %s", string(body))
	}

	return nil
}

// AddRoute adds or updates a reverse proxy route for a specific hostname.
// It assigns the route an ID of "capsule-<capsuleID>" for easy management.
func (c *Client) AddRoute(capsuleID string, host string, upstreamPort int) error {
	routeID := fmt.Sprintf("capsule-%s", capsuleID)
	
	// Use CADDY_UPSTREAM_HOST for Docker environments where Caddy needs to proxy to host
	// Default to 127.0.0.1 for native/localhost environments
	upstreamHost := os.Getenv("CADDY_UPSTREAM_HOST")
	if upstreamHost == "" {
		upstreamHost = "127.0.0.1"
	}
	
	// Caddy Route Structure
	route := map[string]interface{}{
		"@id": routeID,
		"match": []interface{}{
			map[string]interface{}{
				"host": []string{host},
			},
		},
		"handle": []interface{}{
			map[string]interface{}{
				"handler": "reverse_proxy",
				"upstreams": []interface{}{
					map[string]interface{}{
						"dial": fmt.Sprintf("%s:%d", upstreamHost, upstreamPort),
					},
				},
			},
		},
		"terminal": true, // Stop processing after this match
	}

	// Use PUT to /id/<id> (this works if usage of ID API is consistent, checking docs...)
	// Actually, PUT /id/xyz configures the object *referred to* by ID xyz.
	// If the object doesn't exist, we can't PUT to /id/xyz directly to *create* it in a list.
	// We must first POST it to the array if it's new, OR replace it if it exists.
	
	// Check existence
	exists := false
	resp, err := c.HTTPClient.Get(c.BaseURL + "/id/" + routeID)
	if err == nil && resp.StatusCode == 200 {
		exists = true
	}
	if resp != nil {
		resp.Body.Close()
	}

	jsonBody, err := json.Marshal(route)
	if err != nil {
		return err
	}

	var req *http.Request
	if exists {
		// Update existing route (PUT)
		req, err = http.NewRequest("PUT", c.BaseURL+"/id/"+routeID, bytes.NewBuffer(jsonBody))
	} else {
		// Append new route (POST to routes list)
		// Note: This relies on "servers/srv0" existing (EnsureBaseConfig)
		// We append to the routes list.
		req, err = http.NewRequest("POST", c.BaseURL+"/config/apps/http/servers/srv0/routes", bytes.NewBuffer(jsonBody))
	}

	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err = c.HTTPClient.Do(req)
	if err != nil {
		return fmt.Errorf("failed to configure route for %s: %w", capsuleID, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("caddy route config failed: %s", string(body))
	}

	return nil
}

// RemoveRoute deletes the route associated with the capsuleID.
func (c *Client) RemoveRoute(capsuleID string) error {
	routeID := fmt.Sprintf("capsule-%s", capsuleID)

	req, err := http.NewRequest("DELETE", c.BaseURL+"/id/"+routeID, nil)
	if err != nil {
		return err
	}

	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return fmt.Errorf("failed to delete route for %s: %w", capsuleID, err)
	}
	defer resp.Body.Close()

	// 404 is fine (already gone)
	if resp.StatusCode != 200 && resp.StatusCode != 404 {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("caddy route deletion failed: %s", string(body))
	}

	return nil
}

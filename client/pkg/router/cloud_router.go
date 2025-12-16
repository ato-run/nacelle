// Package router provides Local ↔ Cloud routing decisions for Capsules.
package router

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/onescluster/coordinator/pkg/cloud"
	"github.com/onescluster/coordinator/pkg/registry"
)

// EndpointSelector selects the best endpoint for a given capsule.
type EndpointSelector interface {
	// SelectEndpoint returns the best available endpoint for the capsule.
	SelectEndpoint(ctx context.Context, capsuleName string) (*cloud.Endpoint, error)

	// AddEndpoint adds an endpoint to the pool.
	AddEndpoint(endpoint cloud.Endpoint)

	// RemoveEndpoint removes an endpoint from the pool.
	RemoveEndpoint(url string)

	// HealthCheck checks all endpoints and updates their health status.
	HealthCheck(ctx context.Context) error
}

// CloudRouter extends Router with cloud execution capabilities.
type CloudRouter struct {
	*Router
	endpoints   *EndpointPool
}

// EndpointPool manages a pool of cloud endpoints.
type EndpointPool struct {
	endpoints []cloud.Endpoint
	mu        sync.RWMutex
	current   int // For round-robin
}

// NewEndpointPool creates a new endpoint pool.
func NewEndpointPool() *EndpointPool {
	return &EndpointPool{
		endpoints: make([]cloud.Endpoint, 0),
	}
}

// AddEndpoint adds an endpoint to the pool.
func (p *EndpointPool) AddEndpoint(endpoint cloud.Endpoint) {
	p.mu.Lock()
	defer p.mu.Unlock()

	// Check for duplicates
	for i, e := range p.endpoints {
		if e.URL == endpoint.URL {
			p.endpoints[i] = endpoint
			return
		}
	}
	p.endpoints = append(p.endpoints, endpoint)
}

// RemoveEndpoint removes an endpoint from the pool.
func (p *EndpointPool) RemoveEndpoint(url string) {
	p.mu.Lock()
	defer p.mu.Unlock()

	for i, e := range p.endpoints {
		if e.URL == url {
			p.endpoints = append(p.endpoints[:i], p.endpoints[i+1:]...)
			return
		}
	}
}

// SelectEndpoint returns the next healthy endpoint using round-robin.
func (p *EndpointPool) SelectEndpoint(ctx context.Context, capsuleName string) (*cloud.Endpoint, error) {
	p.mu.Lock()
	defer p.mu.Unlock()

	if len(p.endpoints) == 0 {
		return nil, fmt.Errorf("no endpoints available")
	}

	// Find next healthy endpoint (round-robin)
	for i := 0; i < len(p.endpoints); i++ {
		idx := (p.current + i) % len(p.endpoints)
		endpoint := &p.endpoints[idx]

		if endpoint.Healthy {
			p.current = (idx + 1) % len(p.endpoints)
			return endpoint, nil
		}
	}

	// No healthy endpoints, return first one anyway
	endpoint := &p.endpoints[p.current]
	p.current = (p.current + 1) % len(p.endpoints)
	return endpoint, nil
}

// GetAllEndpoints returns all endpoints.
func (p *EndpointPool) GetAllEndpoints() []cloud.Endpoint {
	p.mu.RLock()
	defer p.mu.RUnlock()

	result := make([]cloud.Endpoint, len(p.endpoints))
	copy(result, p.endpoints)
	return result
}

// HealthCheck checks all endpoints and updates their status.
func (p *EndpointPool) HealthCheck(ctx context.Context) error {
	p.mu.Lock()
	defer p.mu.Unlock()

	for i := range p.endpoints {
		endpoint := &p.endpoints[i]

		client := cloud.NewClient(*endpoint, cloud.WithTimeout(5*time.Second))

		start := time.Now()
		err := client.Health(ctx)
		latency := time.Since(start)

		endpoint.LastChecked = time.Now()
		endpoint.Latency = latency

		if err != nil {
			endpoint.Healthy = false
		} else {
			endpoint.Healthy = true
		}
	}

	return nil
}

// HealthyCount returns the number of healthy endpoints.
func (p *EndpointPool) HealthyCount() int {
	p.mu.RLock()
	defer p.mu.RUnlock()

	count := 0
	for _, e := range p.endpoints {
		if e.Healthy {
			count++
		}
	}
	return count
}

// NewCloudRouter creates a new CloudRouter.
func NewCloudRouter(router *Router) *CloudRouter {
	return &CloudRouter{
		Router:    router,
		endpoints: NewEndpointPool(),
	}
}

// AddEndpoint adds a cloud endpoint to the router.
func (r *CloudRouter) AddEndpoint(endpoint cloud.Endpoint) {
	r.endpoints.AddEndpoint(endpoint)
}

// RemoveEndpoint removes a cloud endpoint.
func (r *CloudRouter) RemoveEndpoint(url string) {
	r.endpoints.RemoveEndpoint(url)
}

// HealthCheckEndpoints checks the health of all cloud endpoints.
func (r *CloudRouter) HealthCheckEndpoints(ctx context.Context) error {
	return r.endpoints.HealthCheck(ctx)
}

// GetEndpoints returns all configured endpoints.
func (r *CloudRouter) GetEndpoints() []cloud.Endpoint {
	return r.endpoints.GetAllEndpoints()
}

// ExecuteCloud executes a request on a cloud endpoint.
func (r *CloudRouter) ExecuteCloud(ctx context.Context, decision *Decision, req cloud.ChatRequest) (*cloud.ChatResponse, error) {
	if decision.Route != RouteCloud {
		return nil, fmt.Errorf("decision is not cloud route")
	}

	// Select endpoint
	endpoint, err := r.endpoints.SelectEndpoint(ctx, decision.CapsuleName)
	if err != nil {
		return nil, fmt.Errorf("selecting endpoint: %w", err)
	}

	// Create client for this endpoint
	client := cloud.NewClient(*endpoint)

	// Execute request
	resp, err := client.CreateChatCompletion(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("cloud request failed: %w", err)
	}

	return resp, nil
}

// ExecuteCloudStream executes a streaming request on a cloud endpoint.
func (r *CloudRouter) ExecuteCloudStream(ctx context.Context, decision *Decision, req cloud.ChatRequest) (<-chan cloud.StreamEvent, error) {
	if decision.Route != RouteCloud {
		return nil, fmt.Errorf("decision is not cloud route")
	}

	// Select endpoint
	endpoint, err := r.endpoints.SelectEndpoint(ctx, decision.CapsuleName)
	if err != nil {
		return nil, fmt.Errorf("selecting endpoint: %w", err)
	}

	// Create client for this endpoint
	client := cloud.NewClient(*endpoint)

	// Execute streaming request
	events, err := client.CreateChatCompletionStream(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("cloud stream request failed: %w", err)
	}

	return events, nil
}

// RouteAndExecute combines routing decision and cloud execution.
func (r *CloudRouter) RouteAndExecute(ctx context.Context, capsuleName string, req cloud.ChatRequest) (*cloud.ChatResponse, *Decision, error) {
	// Make routing decision
	decision, err := r.DecideWithContext(ctx, capsuleName)
	if err != nil {
		return nil, nil, fmt.Errorf("routing decision failed: %w", err)
	}

	if decision.Route == RouteLocal {
		// Caller should handle local execution
		return nil, decision, nil
	}

	// Execute on cloud
	resp, err := r.ExecuteCloud(ctx, decision, req)
	if err != nil {
		return nil, decision, err
	}

	return resp, decision, nil
}

// EndpointDiscovery discovers and manages cloud endpoints from the Registry.
type EndpointDiscovery struct {
	registry registry.Client
	pool     *EndpointPool
	mu       sync.RWMutex
	stopCh   chan struct{}
	interval time.Duration
}

// NewEndpointDiscovery creates a new endpoint discovery service.
func NewEndpointDiscovery(registryClient registry.Client, pool *EndpointPool) *EndpointDiscovery {
	return &EndpointDiscovery{
		registry: registryClient,
		pool:     pool,
		interval: 30 * time.Second,
	}
}

// SetInterval sets the discovery interval.
func (d *EndpointDiscovery) SetInterval(interval time.Duration) {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.interval = interval
}

// DiscoverOnce performs a single discovery from the Registry.
func (d *EndpointDiscovery) DiscoverOnce(ctx context.Context) error {
	// List all capsules from registry
	result, err := d.registry.List(ctx, registry.ListOptions{
		Type: "inference", // Only look for inference capsules
	})
	if err != nil {
		return fmt.Errorf("failed to list capsules from registry: %w", err)
	}

	// Look for cloud endpoints
	for _, cap := range result.Capsules {
		// Get download info which may contain endpoint URL
		info, err := d.registry.GetDownloadInfo(ctx, cap.Name, cap.Version, "")
		if err != nil {
			continue
		}

		// If CloudEndpoint is set, add it
		if info.CloudEndpoint != "" {
			d.pool.AddEndpoint(cloud.Endpoint{
				URL:     info.CloudEndpoint,
				Model:   cap.Name,
				Healthy: true, // Assume healthy until checked
			})
		}
	}

	return nil
}

// Start begins periodic endpoint discovery.
func (d *EndpointDiscovery) Start(ctx context.Context) error {
	d.mu.Lock()
	if d.stopCh != nil {
		d.mu.Unlock()
		return fmt.Errorf("discovery already running")
	}
	d.stopCh = make(chan struct{})
	interval := d.interval
	d.mu.Unlock()

	// Initial discovery
	if err := d.DiscoverOnce(ctx); err != nil {
		// Log but don't fail - continue periodic discovery
		fmt.Printf("initial discovery failed: %v\n", err)
	}

	// Start periodic discovery
	go func() {
		ticker := time.NewTicker(interval)
		defer ticker.Stop()

		for {
			select {
			case <-ctx.Done():
				return
			case <-d.stopCh:
				return
			case <-ticker.C:
				if err := d.DiscoverOnce(ctx); err != nil {
					fmt.Printf("discovery failed: %v\n", err)
				}
				// Also run health checks
				if err := d.pool.HealthCheck(ctx); err != nil {
					fmt.Printf("health check failed: %v\n", err)
				}
			}
		}
	}()

	return nil
}

// Stop stops periodic endpoint discovery.
func (d *EndpointDiscovery) Stop() {
	d.mu.Lock()
	defer d.mu.Unlock()

	if d.stopCh != nil {
		close(d.stopCh)
		d.stopCh = nil
	}
}

// NewCloudRouterWithDiscovery creates a CloudRouter with endpoint discovery.
func NewCloudRouterWithDiscovery(baseRouter *Router, registryClient registry.Client) *CloudRouter {
	pool := NewEndpointPool()
	cr := &CloudRouter{
		Router:    baseRouter,
		endpoints: pool,
	}
	// Discovery will be started separately
	return cr
}

// SetDiscovery sets the endpoint discovery for the CloudRouter.
func (r *CloudRouter) SetDiscovery(registryClient registry.Client) *EndpointDiscovery {
	discovery := NewEndpointDiscovery(registryClient, r.endpoints)
	return discovery
}

// StartDiscovery starts the endpoint discovery with the given registry client.
func (r *CloudRouter) StartDiscovery(ctx context.Context, registryClient registry.Client) (*EndpointDiscovery, error) {
	discovery := NewEndpointDiscovery(registryClient, r.endpoints)
	if err := discovery.Start(ctx); err != nil {
		return nil, err
	}
	return discovery, nil
}

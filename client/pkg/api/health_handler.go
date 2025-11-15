package api

import (
	"context"
	"encoding/json"
	"net/http"
	"time"
)

// HealthChecker is an interface for checking the health of a dependency
type HealthChecker interface {
	Check(ctx context.Context) error
}

// HealthHandler handles health check requests
type HealthHandler struct {
	StartTime time.Time
	checkers  map[string]HealthChecker
}

// NewHealthHandler creates a new health handler
func NewHealthHandler() *HealthHandler {
	return &HealthHandler{
		StartTime: time.Now(),
		checkers:  make(map[string]HealthChecker),
	}
}

// AddChecker adds a health checker for a dependency
func (h *HealthHandler) AddChecker(name string, checker HealthChecker) {
	h.checkers[name] = checker
}

// HealthStatus represents the health status response
type HealthStatus struct {
	Status       string                    `json:"status"`
	Uptime       string                    `json:"uptime"`
	Timestamp    string                    `json:"timestamp"`
	Version      string                    `json:"version,omitempty"`
	Dependencies map[string]DependencyInfo `json:"dependencies,omitempty"`
}

// DependencyInfo represents the health status of a dependency
type DependencyInfo struct {
	Status  string `json:"status"`
	Message string `json:"message,omitempty"`
}

// HandleHealth handles GET /health requests
func (h *HealthHandler) HandleHealth(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	uptime := time.Since(h.StartTime)

	response := HealthStatus{
		Status:    "healthy",
		Uptime:    uptime.String(),
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Version:   "0.1.0", // TODO: Read from version file
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(response)
}

// HandleReadiness handles GET /ready requests (Kubernetes readiness probe)
func (h *HealthHandler) HandleReadiness(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Check all dependencies
	ctx, cancel := context.WithTimeout(r.Context(), 5*time.Second)
	defer cancel()

	dependencies := make(map[string]DependencyInfo)
	allHealthy := true

	for name, checker := range h.checkers {
		if err := checker.Check(ctx); err != nil {
			dependencies[name] = DependencyInfo{
				Status:  "unhealthy",
				Message: err.Error(),
			}
			allHealthy = false
		} else {
			dependencies[name] = DependencyInfo{
				Status: "healthy",
			}
		}
	}

	status := HealthStatus{
		Status:       "ready",
		Uptime:       time.Since(h.StartTime).String(),
		Timestamp:    time.Now().UTC().Format(time.RFC3339),
		Version:      "0.1.0",
		Dependencies: dependencies,
	}

	w.Header().Set("Content-Type", "application/json")
	
	if allHealthy {
		w.WriteHeader(http.StatusOK)
		status.Status = "ready"
	} else {
		w.WriteHeader(http.StatusServiceUnavailable)
		status.Status = "not ready"
	}

	json.NewEncoder(w).Encode(status)
}

// HandleLiveness handles GET /live requests (Kubernetes liveness probe)
func (h *HealthHandler) HandleLiveness(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Simple liveness check - if we can respond, we're alive
	w.WriteHeader(http.StatusOK)
	w.Write([]byte("alive"))
}

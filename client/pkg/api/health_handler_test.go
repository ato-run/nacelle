package api

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
)

// MockHealthChecker is a mock implementation of HealthChecker for testing
type MockHealthChecker struct {
	shouldFail bool
	err        error
}

func (m *MockHealthChecker) Check(ctx context.Context) error {
	if m.shouldFail {
		if m.err != nil {
			return m.err
		}
		return errors.New("mock health check failed")
	}
	return nil
}

func TestHealthHandler_HandleHealth(t *testing.T) {
	handler := NewHealthHandler()

	req := httptest.NewRequest(http.MethodGet, "/health", nil)
	rec := httptest.NewRecorder()

	handler.HandleHealth(rec, req)

	if rec.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", rec.Code)
	}

	var response HealthStatus
	if err := json.Unmarshal(rec.Body.Bytes(), &response); err != nil {
		t.Fatalf("Failed to parse response: %v", err)
	}

	if response.Status != "healthy" {
		t.Errorf("Expected status 'healthy', got '%s'", response.Status)
	}

	if response.Uptime == "" {
		t.Error("Expected non-empty uptime")
	}
}

func TestHealthHandler_HandleReadiness(t *testing.T) {
	handler := NewHealthHandler()

	req := httptest.NewRequest(http.MethodGet, "/ready", nil)
	rec := httptest.NewRecorder()

	handler.HandleReadiness(rec, req)

	if rec.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", rec.Code)
	}

	var response HealthStatus
	if err := json.Unmarshal(rec.Body.Bytes(), &response); err != nil {
		t.Fatalf("Failed to parse response: %v", err)
	}

	if response.Status != "ready" {
		t.Errorf("Expected status 'ready', got '%s'", response.Status)
	}
}

func TestHealthHandler_HandleReadiness_WithHealthyDependencies(t *testing.T) {
	handler := NewHealthHandler()
	handler.AddChecker("database", &MockHealthChecker{shouldFail: false})
	handler.AddChecker("grpc", &MockHealthChecker{shouldFail: false})

	req := httptest.NewRequest(http.MethodGet, "/ready", nil)
	rec := httptest.NewRecorder()

	handler.HandleReadiness(rec, req)

	if rec.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", rec.Code)
	}

	var response HealthStatus
	if err := json.Unmarshal(rec.Body.Bytes(), &response); err != nil {
		t.Fatalf("Failed to parse response: %v", err)
	}

	if response.Status != "ready" {
		t.Errorf("Expected status 'ready', got '%s'", response.Status)
	}

	if len(response.Dependencies) != 2 {
		t.Errorf("Expected 2 dependencies, got %d", len(response.Dependencies))
	}

	if response.Dependencies["database"].Status != "healthy" {
		t.Errorf("Expected database to be healthy, got '%s'", response.Dependencies["database"].Status)
	}
}

func TestHealthHandler_HandleReadiness_WithUnhealthyDependency(t *testing.T) {
	handler := NewHealthHandler()
	handler.AddChecker("database", &MockHealthChecker{shouldFail: false})
	handler.AddChecker("grpc", &MockHealthChecker{shouldFail: true, err: errors.New("connection refused")})

	req := httptest.NewRequest(http.MethodGet, "/ready", nil)
	rec := httptest.NewRecorder()

	handler.HandleReadiness(rec, req)

	if rec.Code != http.StatusServiceUnavailable {
		t.Errorf("Expected status 503, got %d", rec.Code)
	}

	var response HealthStatus
	if err := json.Unmarshal(rec.Body.Bytes(), &response); err != nil {
		t.Fatalf("Failed to parse response: %v", err)
	}

	if response.Status != "not ready" {
		t.Errorf("Expected status 'not ready', got '%s'", response.Status)
	}

	if response.Dependencies["grpc"].Status != "unhealthy" {
		t.Errorf("Expected grpc to be unhealthy, got '%s'", response.Dependencies["grpc"].Status)
	}

	if response.Dependencies["grpc"].Message != "connection refused" {
		t.Errorf("Expected error message 'connection refused', got '%s'", response.Dependencies["grpc"].Message)
	}
}

func TestHealthHandler_HandleLiveness(t *testing.T) {
	handler := NewHealthHandler()

	req := httptest.NewRequest(http.MethodGet, "/live", nil)
	rec := httptest.NewRecorder()

	handler.HandleLiveness(rec, req)

	if rec.Code != http.StatusOK {
		t.Errorf("Expected status 200, got %d", rec.Code)
	}

	if rec.Body.String() != "alive" {
		t.Errorf("Expected body 'alive', got '%s'", rec.Body.String())
	}
}

func TestHealthHandler_MethodNotAllowed(t *testing.T) {
	handler := NewHealthHandler()

	req := httptest.NewRequest(http.MethodPost, "/health", nil)
	rec := httptest.NewRecorder()

	handler.HandleHealth(rec, req)

	if rec.Code != http.StatusMethodNotAllowed {
		t.Errorf("Expected status 405, got %d", rec.Code)
	}
}

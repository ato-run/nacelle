// Package router provides Local ↔ Cloud routing decisions for Capsules.
// This is the TDD test file - tests written FIRST before implementation.
package router

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/capsule"
	"github.com/onescluster/coordinator/pkg/hardware"
)

// MockHardwareMonitor implements hardware.HardwareMonitor for testing
type MockHardwareMonitor struct {
	resources *hardware.SystemResources
	err       error
}

func (m *MockHardwareMonitor) GetCurrentResources() (*hardware.SystemResources, error) {
	if m.err != nil {
		return nil, m.err
	}
	return m.resources, nil
}

func (m *MockHardwareMonitor) CanRunCapsule(vramRequired int64) (*hardware.ResourceCheckResult, error) {
	if m.err != nil {
		return nil, m.err
	}
	available := m.resources.AvailableVRAM
	canRun := available >= vramRequired
	return &hardware.ResourceCheckResult{
		CanRun: canRun,
		Reason: "",
	}, nil
}

func (m *MockHardwareMonitor) Watch(ctx context.Context, interval time.Duration, callback func(*hardware.SystemResources)) {
	// Not implemented for tests
}

// MockCapsuleStore implements store.Store for testing
type MockCapsuleStore struct {
	capsules map[string]*capsule.CapsuleManifest
}

func (m *MockCapsuleStore) GetManifest(name string) (*capsule.CapsuleManifest, error) {
	manifest, ok := m.capsules[name]
	if !ok {
		return nil, fmt.Errorf("capsule not found: %s", name)

	}
	return manifest, nil
}

// =============================================================================
// Test Cases
// =============================================================================

func TestRouterDecide_LightCapsule_EnoughResources_ReturnsLocal(t *testing.T) {
	// Arrange: Light capsule, 16GB VRAM available, 6GB required
	monitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     16 * 1024 * 1024 * 1024, // 16GB
			AvailableVRAM: 14 * 1024 * 1024 * 1024, // 14GB available
			TotalRAM:      32 * 1024 * 1024 * 1024,
			AvailableRAM:  20 * 1024 * 1024 * 1024,
		},
	}

	manifest := &capsule.CapsuleManifest{
		Name: "mlx-qwen3-8b",
		Type: capsule.TypeInference,
		Requirements: capsule.Requirements{
			VRAMMin: "6GB",
		},
		Routing: capsule.Routing{
			Weight:          capsule.WeightLight,
			FallbackToCloud: true,
			CloudCapsule:    "vllm-qwen3-8b",
		},
	}

	store := &MockCapsuleStore{
		capsules: map[string]*capsule.CapsuleManifest{
			"mlx-qwen3-8b": manifest,
		},
	}

	router := NewRouter(monitor, store, DefaultConfig())

	// Act
	decision, err := router.Decide("mlx-qwen3-8b")

	// Assert
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if decision.Route != RouteLocal {
		t.Errorf("expected RouteLocal, got %v", decision.Route)
	}
	if decision.CapsuleName != "mlx-qwen3-8b" {
		t.Errorf("expected capsule name mlx-qwen3-8b, got %s", decision.CapsuleName)
	}
}

func TestRouterDecide_LightCapsule_InsufficientVRAM_FallbackToCloud(t *testing.T) {
	// Arrange: Light capsule, only 4GB VRAM available, 6GB required
	monitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     16 * 1024 * 1024 * 1024, // 16GB
			AvailableVRAM: 4 * 1024 * 1024 * 1024,  // Only 4GB available
			TotalRAM:      32 * 1024 * 1024 * 1024,
			AvailableRAM:  20 * 1024 * 1024 * 1024,
		},
	}

	manifest := &capsule.CapsuleManifest{
		Name: "mlx-qwen3-8b",
		Type: capsule.TypeInference,
		Requirements: capsule.Requirements{
			VRAMMin: "6GB",
		},
		Routing: capsule.Routing{
			Weight:          capsule.WeightLight,
			FallbackToCloud: true,
			CloudCapsule:    "vllm-qwen3-8b",
		},
	}

	store := &MockCapsuleStore{
		capsules: map[string]*capsule.CapsuleManifest{
			"mlx-qwen3-8b": manifest,
		},
	}

	router := NewRouter(monitor, store, DefaultConfig())

	// Act
	decision, err := router.Decide("mlx-qwen3-8b")

	// Assert
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if decision.Route != RouteCloud {
		t.Errorf("expected RouteCloud, got %v", decision.Route)
	}
	if decision.CapsuleName != "vllm-qwen3-8b" {
		t.Errorf("expected cloud capsule vllm-qwen3-8b, got %s", decision.CapsuleName)
	}
	if decision.Reason == "" {
		t.Error("expected reason to be set for cloud fallback")
	}
}

func TestRouterDecide_HeavyCapsule_AlwaysCloud(t *testing.T) {
	// Arrange: Heavy capsule should always route to cloud
	monitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     64 * 1024 * 1024 * 1024, // Even with 64GB
			AvailableVRAM: 60 * 1024 * 1024 * 1024,
			TotalRAM:      128 * 1024 * 1024 * 1024,
			AvailableRAM:  100 * 1024 * 1024 * 1024,
		},
	}

	manifest := &capsule.CapsuleManifest{
		Name: "vllm-llama70b",
		Type: capsule.TypeInference,
		Requirements: capsule.Requirements{
			VRAMMin: "80GB", // Requires 80GB
		},
		Routing: capsule.Routing{
			Weight:          capsule.WeightHeavy,
			FallbackToCloud: true,
			CloudCapsule:    "vllm-llama70b-cloud",
		},
	}

	store := &MockCapsuleStore{
		capsules: map[string]*capsule.CapsuleManifest{
			"vllm-llama70b": manifest,
		},
	}

	router := NewRouter(monitor, store, DefaultConfig())

	// Act
	decision, err := router.Decide("vllm-llama70b")

	// Assert
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if decision.Route != RouteCloud {
		t.Errorf("expected RouteCloud for heavy capsule, got %v", decision.Route)
	}
}

func TestRouterDecide_NoFallback_InsufficientResources_ReturnsError(t *testing.T) {
	// Arrange: Light capsule with no cloud fallback, insufficient resources
	monitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     16 * 1024 * 1024 * 1024,
			AvailableVRAM: 2 * 1024 * 1024 * 1024, // Only 2GB
			TotalRAM:      32 * 1024 * 1024 * 1024,
			AvailableRAM:  20 * 1024 * 1024 * 1024,
		},
	}

	manifest := &capsule.CapsuleManifest{
		Name: "local-only-capsule",
		Type: capsule.TypeInference,
		Requirements: capsule.Requirements{
			VRAMMin: "8GB",
		},
		Routing: capsule.Routing{
			Weight:          capsule.WeightLight,
			FallbackToCloud: false, // No fallback!
			CloudCapsule:    "",
		},
	}

	store := &MockCapsuleStore{
		capsules: map[string]*capsule.CapsuleManifest{
			"local-only-capsule": manifest,
		},
	}

	router := NewRouter(monitor, store, DefaultConfig())

	// Act
	decision, err := router.Decide("local-only-capsule")

	// Assert
	if err == nil {
		t.Fatal("expected error for insufficient resources without fallback")
	}
	if decision != nil {
		t.Errorf("expected nil decision on error, got %v", decision)
	}
}

func TestRouterDecide_CapsuleNotFound_ReturnsError(t *testing.T) {
	monitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{},
	}
	store := &MockCapsuleStore{
		capsules: map[string]*capsule.CapsuleManifest{},
	}

	router := NewRouter(monitor, store, DefaultConfig())

	// Act
	decision, err := router.Decide("non-existent-capsule")

	// Assert
	if err == nil {
		t.Fatal("expected error for non-existent capsule")
	}
	if decision != nil {
		t.Errorf("expected nil decision on error, got %v", decision)
	}
}

func TestRouterDecide_VRAMWarningThreshold_StillRuns(t *testing.T) {
	// Arrange: 82% VRAM used (above 80% warning, below 95% block)
	totalVRAM := int64(16 * 1024 * 1024 * 1024)
	monitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     totalVRAM,
			AvailableVRAM: int64(float64(totalVRAM) * 0.18), // 18% free = 82% used
			TotalRAM:      32 * 1024 * 1024 * 1024,
			AvailableRAM:  20 * 1024 * 1024 * 1024,
		},
	}

	manifest := &capsule.CapsuleManifest{
		Name: "small-capsule",
		Type: capsule.TypeInference,
		Requirements: capsule.Requirements{
			VRAMMin: "1GB", // Small requirement
		},
		Routing: capsule.Routing{
			Weight:          capsule.WeightLight,
			FallbackToCloud: true,
			CloudCapsule:    "small-capsule-cloud",
		},
	}

	store := &MockCapsuleStore{
		capsules: map[string]*capsule.CapsuleManifest{
			"small-capsule": manifest,
		},
	}

	router := NewRouter(monitor, store, DefaultConfig())

	// Act
	decision, err := router.Decide("small-capsule")

	// Assert
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if decision.Route != RouteLocal {
		t.Errorf("expected RouteLocal (with warning), got %v", decision.Route)
	}
	if !decision.Warning {
		t.Error("expected warning flag to be set")
	}
}

func TestRouterDecide_VRAMBlockThreshold_RoutesToCloud(t *testing.T) {
	// Arrange: 96% VRAM used (above 95% block threshold)
	totalVRAM := int64(16 * 1024 * 1024 * 1024)
	monitor := &MockHardwareMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     totalVRAM,
			AvailableVRAM: int64(float64(totalVRAM) * 0.04), // 4% free = 96% used
			TotalRAM:      32 * 1024 * 1024 * 1024,
			AvailableRAM:  20 * 1024 * 1024 * 1024,
		},
	}

	manifest := &capsule.CapsuleManifest{
		Name: "small-capsule",
		Type: capsule.TypeInference,
		Requirements: capsule.Requirements{
			VRAMMin: "100MB", // Tiny requirement
		},
		Routing: capsule.Routing{
			Weight:          capsule.WeightLight,
			FallbackToCloud: true,
			CloudCapsule:    "small-capsule-cloud",
		},
	}

	store := &MockCapsuleStore{
		capsules: map[string]*capsule.CapsuleManifest{
			"small-capsule": manifest,
		},
	}

	router := NewRouter(monitor, store, DefaultConfig())

	// Act
	decision, err := router.Decide("small-capsule")

	// Assert
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if decision.Route != RouteCloud {
		t.Errorf("expected RouteCloud due to block threshold, got %v", decision.Route)
	}
}

func TestParseSize_ValidInputs(t *testing.T) {
	tests := []struct {
		input    string
		expected int64
	}{
		{"1GB", 1 * 1024 * 1024 * 1024},
		{"6GB", 6 * 1024 * 1024 * 1024},
		{"512MB", 512 * 1024 * 1024},
		{"100MB", 100 * 1024 * 1024},
		{"1.5GB", int64(1.5 * 1024 * 1024 * 1024)},
		{"8gb", 8 * 1024 * 1024 * 1024},    // lowercase
		{"16 GB", 16 * 1024 * 1024 * 1024}, // with space
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			result, err := ParseSize(tt.input)
			if err != nil {
				t.Fatalf("unexpected error for %s: %v", tt.input, err)
			}
			// Allow 1% tolerance for floating point
			tolerance := int64(float64(tt.expected) * 0.01)
			if result < tt.expected-tolerance || result > tt.expected+tolerance {
				t.Errorf("ParseSize(%s) = %d, want %d", tt.input, result, tt.expected)
			}
		})
	}
}

func TestParseSize_InvalidInputs(t *testing.T) {
	tests := []string{
		"",
		"invalid",
		"GB",
		"-1GB",
	}

	for _, input := range tests {
		t.Run(input, func(t *testing.T) {
			_, err := ParseSize(input)
			if err == nil {
				t.Errorf("expected error for invalid input: %s", input)
			}
		})
	}
}

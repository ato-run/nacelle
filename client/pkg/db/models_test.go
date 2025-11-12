package db

import (
	"testing"
)

func TestNodeResourcesCalculateAvailableResources(t *testing.T) {
	resources := &NodeResources{
		NodeID:           "node-1",
		CPUTotal:         8000,  // 8 cores in millicores
		CPUAllocated:     3000,  // 3 cores allocated
		MemoryTotal:      16000000000, // 16 GB
		MemoryAllocated:  8000000000,  // 8 GB allocated
		StorageTotal:     1000000000000, // 1 TB
		StorageAllocated: 500000000000,  // 500 GB allocated
	}

	available := resources.CalculateAvailableResources()

	if available.CPUAvailable != 5000 {
		t.Errorf("Expected CPUAvailable to be 5000, got %d", available.CPUAvailable)
	}

	if available.MemoryAvailable != 8000000000 {
		t.Errorf("Expected MemoryAvailable to be 8000000000, got %d", available.MemoryAvailable)
	}

	if available.StorageAvailable != 500000000000 {
		t.Errorf("Expected StorageAvailable to be 500000000000, got %d", available.StorageAvailable)
	}
}

func TestNodeResourcesCanAllocate(t *testing.T) {
	resources := &NodeResources{
		NodeID:           "node-1",
		CPUTotal:         8000,
		CPUAllocated:     3000,
		MemoryTotal:      16000000000,
		MemoryAllocated:  8000000000,
		StorageTotal:     1000000000000,
		StorageAllocated: 500000000000,
	}

	tests := []struct {
		name     string
		request  CapsuleResources
		expected bool
	}{
		{
			name: "can allocate small request",
			request: CapsuleResources{
				CPURequest:     1000,
				MemoryRequest:  2000000000,
				StorageRequest: 100000000000,
			},
			expected: true,
		},
		{
			name: "cannot allocate request exceeding CPU",
			request: CapsuleResources{
				CPURequest:     6000,
				MemoryRequest:  2000000000,
				StorageRequest: 100000000000,
			},
			expected: false,
		},
		{
			name: "cannot allocate request exceeding memory",
			request: CapsuleResources{
				CPURequest:     1000,
				MemoryRequest:  10000000000,
				StorageRequest: 100000000000,
			},
			expected: false,
		},
		{
			name: "cannot allocate request exceeding storage",
			request: CapsuleResources{
				CPURequest:     1000,
				MemoryRequest:  2000000000,
				StorageRequest: 600000000000,
			},
			expected: false,
		},
		{
			name: "can allocate exact available resources",
			request: CapsuleResources{
				CPURequest:     5000,
				MemoryRequest:  8000000000,
				StorageRequest: 500000000000,
			},
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := resources.CanAllocate(tt.request)
			if result != tt.expected {
				t.Errorf("CanAllocate() = %v, expected %v", result, tt.expected)
			}
		})
	}
}

func TestNodeStatusValues(t *testing.T) {
	// Test that status constants have expected values
	if NodeStatusActive != "active" {
		t.Errorf("Expected NodeStatusActive to be 'active', got '%s'", NodeStatusActive)
	}

	if NodeStatusInactive != "inactive" {
		t.Errorf("Expected NodeStatusInactive to be 'inactive', got '%s'", NodeStatusInactive)
	}

	if NodeStatusFailed != "failed" {
		t.Errorf("Expected NodeStatusFailed to be 'failed', got '%s'", NodeStatusFailed)
	}
}

func TestCapsuleStatusValues(t *testing.T) {
	// Test that status constants have expected values
	if CapsuleStatusPending != "pending" {
		t.Errorf("Expected CapsuleStatusPending to be 'pending', got '%s'", CapsuleStatusPending)
	}

	if CapsuleStatusRunning != "running" {
		t.Errorf("Expected CapsuleStatusRunning to be 'running', got '%s'", CapsuleStatusRunning)
	}

	if CapsuleStatusStopped != "stopped" {
		t.Errorf("Expected CapsuleStatusStopped to be 'stopped', got '%s'", CapsuleStatusStopped)
	}

	if CapsuleStatusFailed != "failed" {
		t.Errorf("Expected CapsuleStatusFailed to be 'failed', got '%s'", CapsuleStatusFailed)
	}
}

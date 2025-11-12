package gpu

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

const (
	GB = 1024 * 1024 * 1024
)

// TestGpuScheduling_BestFitBinPacking verifies the BestFit (MostAllocated) strategy.
//
// This test simulates Week 1's MOCK_GPU_COUNT environment, but on the Coordinator side.
// We create mock Rigs with different VRAM profiles and verify that the scheduler
// selects the Rig that will have the highest utilization *after* placement.
func TestGpuScheduling_BestFitBinPacking(t *testing.T) {
	// Mock Rig cluster (similar to MOCK_GPU_COUNT simulation)
	rigs := []*RigGpuInfo{
		{
			RigID:             "rig-1-large-busy", // A: Large capacity, moderately used
			TotalVRAMBytes:    96 * GB,            // 96 GB total
			UsedVRAMBytes:     40 * GB,            // 40 GB in use
			CudaDriverVersion: "12.2",             // 56 GB free
		},
		{
			RigID:             "rig-2-medium-idle", // B: Medium capacity, lightly used
			TotalVRAMBytes:    48 * GB,             // 48 GB total
			UsedVRAMBytes:     10 * GB,             // 10 GB in use
			CudaDriverVersion: "12.2",              // 38 GB free
		},
	}

	// Workload requirement: 30 GB VRAM
	constraints := &GpuConstraints{
		VramMinGB:      30,
		CudaVersionMin: "12.0",
	}

	// Execute scheduler
	scheduler := NewScheduler()
	selectedRig, err := scheduler.FindBestRig(rigs, constraints)

	// --- Verification ---
	// Both Rigs pass the filter stage:
	// - rig-1: 56 GB free >= 30 GB required ✓
	// - rig-2: 38 GB free >= 30 GB required ✓
	//
	// Scoring stage (ScoreByVRAMBinPacking):
	// - rig-1 (A): (40 GB + 30 GB) / 96 GB = 70/96 = 72.9% -> score 72
	// - rig-2 (B): (10 GB + 30 GB) / 48 GB = 40/48 = 83.3% -> score 83
	//
	// BestFit strategy chooses rig-2 because it achieves higher utilization (better bin packing)
	require.NoError(t, err, "Scheduler should find a suitable Rig")
	require.NotNil(t, selectedRig, "Selected Rig should not be nil")
	assert.Equal(t, "rig-2-medium-idle", selectedRig.RigID,
		"BestFit should select rig-2 (83.3% utilization) over rig-1 (72.9% utilization)")
}

// TestGpuScheduling_Filtering verifies that filter functions correctly exclude unsuitable Rigs.
func TestGpuScheduling_Filtering(t *testing.T) {
	rigs := []*RigGpuInfo{
		{
			RigID:             "rig-a",
			TotalVRAMBytes:    64 * GB,
			UsedVRAMBytes:     10 * GB, // 54 GB free
			CudaDriverVersion: "12.2",
		},
		{
			RigID:             "rig-b",
			TotalVRAMBytes:    24 * GB,
			UsedVRAMBytes:     20 * GB, // 4 GB free (insufficient)
			CudaDriverVersion: "12.2",
		},
		{
			RigID:             "rig-c",
			TotalVRAMBytes:    32 * GB,
			UsedVRAMBytes:     10 * GB, // 22 GB free
			CudaDriverVersion: "11.8",  // Older CUDA version
		},
	}
	scheduler := NewScheduler()

	// Scenario 1: Filter by VRAM availability
	t.Run("FilterByVRAM", func(t *testing.T) {
		constraints := &GpuConstraints{
			VramMinGB:      40, // Requires 40 GB
			CudaVersionMin: "12.0",
		}
		selectedRig, err := scheduler.FindBestRig(rigs, constraints)
		require.NoError(t, err, "Should find at least one suitable Rig")
		assert.Equal(t, "rig-a", selectedRig.RigID,
			"Only rig-a (54 GB free) satisfies 40 GB requirement; rig-b (4 GB) and rig-c (22 GB) excluded")
	})

	// Scenario 2: Filter by CUDA version
	t.Run("FilterByCudaVersion", func(t *testing.T) {
		constraints := &GpuConstraints{
			VramMinGB:      20, // 20 GB requirement
			CudaVersionMin: "12.0",
		}
		selectedRig, err := scheduler.FindBestRig(rigs, constraints)
		require.NoError(t, err, "Should find at least one suitable Rig")
		// rig-a passes both filters (54 GB free, CUDA 12.2)
		// rig-b fails VRAM filter (4 GB free < 20 GB)
		// rig-c fails CUDA filter (11.8 < 12.0)
		assert.Equal(t, "rig-a", selectedRig.RigID,
			"Only rig-a satisfies both VRAM and CUDA constraints")
	})

	// Scenario 3: No suitable Rig available
	t.Run("NoRigAvailable", func(t *testing.T) {
		constraints := &GpuConstraints{
			VramMinGB:      100, // Impossible requirement (no Rig has 100 GB free)
			CudaVersionMin: "12.0",
		}
		_, err := scheduler.FindBestRig(rigs, constraints)
		require.Error(t, err, "Should return error when no Rig satisfies constraints")
		assert.Equal(t, ErrNoRigAvailable, err, "Should return specific error type")
	})
}

// TestGpuScheduling_MultipleCandidates verifies scoring when multiple Rigs are eligible.
func TestGpuScheduling_MultipleCandidates(t *testing.T) {
	// Three Rigs with different utilization profiles
	rigs := []*RigGpuInfo{
		{
			RigID:             "rig-low-util",    // 25% utilized -> will be 50% after placement
			TotalVRAMBytes:    64 * GB,
			UsedVRAMBytes:     16 * GB,           // 48 GB free
			CudaDriverVersion: "12.2",
		},
		{
			RigID:             "rig-medium-util", // 50% utilized -> will be 75% after placement
			TotalVRAMBytes:    64 * GB,
			UsedVRAMBytes:     32 * GB,           // 32 GB free
			CudaDriverVersion: "12.2",
		},
		{
			RigID:             "rig-high-util",   // 75% utilized -> will be 100% after placement
			TotalVRAMBytes:    64 * GB,
			UsedVRAMBytes:     48 * GB,           // 16 GB free
			CudaDriverVersion: "12.2",
		},
	}

	constraints := &GpuConstraints{
		VramMinGB:      16, // 16 GB requirement (all Rigs can satisfy)
		CudaVersionMin: "12.0",
	}

	scheduler := NewScheduler()
	selectedRig, err := scheduler.FindBestRig(rigs, constraints)

	// All three Rigs pass filters (16 GB free, CUDA 12.2)
	// Scoring:
	// - rig-low-util:    (16 + 16) / 64 = 50% -> score 50
	// - rig-medium-util: (32 + 16) / 64 = 75% -> score 75
	// - rig-high-util:   (48 + 16) / 64 = 100% -> score 100
	//
	// BestFit selects rig-high-util (highest utilization = best bin packing)
	require.NoError(t, err)
	assert.Equal(t, "rig-high-util", selectedRig.RigID,
		"BestFit should select the Rig that will have highest utilization after placement")
}

// TestGpuScheduling_CPUOnlyWorkload verifies handling of workloads that don't require GPU.
func TestGpuScheduling_CPUOnlyWorkload(t *testing.T) {
	rigs := []*RigGpuInfo{
		{
			RigID:             "rig-with-gpu",
			TotalVRAMBytes:    64 * GB,
			UsedVRAMBytes:     0,
			CudaDriverVersion: "12.2",
		},
		{
			RigID:             "rig-cpu-only",
			TotalVRAMBytes:    0,             // No GPU
			UsedVRAMBytes:     0,
			CudaDriverVersion: "",            // No CUDA
		},
	}

	// CPU-only workload (no GPU requirement)
	constraints := &GpuConstraints{
		VramMinGB:      0,  // No VRAM needed
		CudaVersionMin: "", // No CUDA needed
	}

	scheduler := NewScheduler()
	selectedRig, err := scheduler.FindBestRig(rigs, constraints)

	// When no GPU is required, FilterHasGPU should pass both Rigs
	// Scoring may favor one or the other depending on implementation
	require.NoError(t, err, "CPU-only workload should be schedulable")
	require.NotNil(t, selectedRig, "Should select a Rig for CPU-only workload")
	// Either Rig is acceptable for CPU-only workload
}

// TestGpuScheduling_ExactVRAMMatch verifies edge case where request exactly matches available VRAM.
func TestGpuScheduling_ExactVRAMMatch(t *testing.T) {
	rigs := []*RigGpuInfo{
		{
			RigID:             "rig-exact-match",
			TotalVRAMBytes:    64 * GB,
			UsedVRAMBytes:     48 * GB, // Exactly 16 GB free
			CudaDriverVersion: "12.2",
		},
	}

	constraints := &GpuConstraints{
		VramMinGB:      16, // Exactly matches available VRAM
		CudaVersionMin: "12.0",
	}

	scheduler := NewScheduler()
	selectedRig, err := scheduler.FindBestRig(rigs, constraints)

	// Should pass filter (16 GB free >= 16 GB required)
	// After placement: 100% utilization (perfect bin packing)
	require.NoError(t, err, "Exact VRAM match should be schedulable")
	assert.Equal(t, "rig-exact-match", selectedRig.RigID)
}

// TestGpuScheduling_CudaVersionComparison verifies semantic version comparison.
func TestGpuScheduling_CudaVersionComparison(t *testing.T) {
	testCases := []struct {
		name              string
		rigCuda           string
		requiredCuda      string
		shouldPass        bool
	}{
		{"ExactMatch", "12.0", "12.0", true},
		{"NewerMinor", "12.1", "12.0", true},
		{"NewerMajor", "13.0", "12.0", true},
		{"OlderMinor", "12.0", "12.1", false},
		{"OlderMajor", "11.8", "12.0", false},
		{"NoRequirement", "12.0", "", true},
		{"NoGPU", "", "12.0", false},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			rigs := []*RigGpuInfo{
				{
					RigID:             "test-rig",
					TotalVRAMBytes:    64 * GB,
					UsedVRAMBytes:     0,
					CudaDriverVersion: tc.rigCuda,
				},
			}

			constraints := &GpuConstraints{
				VramMinGB:      8, // Small requirement to focus on CUDA version
				CudaVersionMin: tc.requiredCuda,
			}

			scheduler := NewScheduler()
			_, err := scheduler.FindBestRig(rigs, constraints)

			if tc.shouldPass {
				assert.NoError(t, err, "CUDA version %s should satisfy requirement %s", tc.rigCuda, tc.requiredCuda)
			} else {
				assert.Error(t, err, "CUDA version %s should NOT satisfy requirement %s", tc.rigCuda, tc.requiredCuda)
			}
		})
	}
}

// TestScheduler_CustomFiltersAndScorers verifies extensibility of the scheduler.
func TestScheduler_CustomFiltersAndScorers(t *testing.T) {
	rigs := []*RigGpuInfo{
		{RigID: "rig-a", TotalVRAMBytes: 64 * GB, UsedVRAMBytes: 0, CudaDriverVersion: "12.2"},
		{RigID: "rig-b", TotalVRAMBytes: 32 * GB, UsedVRAMBytes: 0, CudaDriverVersion: "12.2"},
	}

	constraints := &GpuConstraints{VramMinGB: 16, CudaVersionMin: "12.0"}

	// Create scheduler with custom policy
	scheduler := NewScheduler()

	// Add custom filter: only allow Rigs with ID containing "rig-a"
	customFilter := func(rig *RigGpuInfo, _ *GpuConstraints) bool {
		return rig.RigID == "rig-a"
	}
	scheduler.AddFilter(customFilter)

	selectedRig, err := scheduler.FindBestRig(rigs, constraints)

	require.NoError(t, err)
	assert.Equal(t, "rig-a", selectedRig.RigID, "Custom filter should exclude rig-b")
}

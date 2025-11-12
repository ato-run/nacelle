package gpu

// ScoreFunc assigns a priority score (0-100) to a Rig based on the given constraints.
// Higher scores indicate higher priority for placement.
//
// Score functions are only called on Rigs that have passed all filters.
// Multiple score functions can be combined with weights to create sophisticated
// scheduling policies.
type ScoreFunc func(rig *RigGpuInfo, constraints *GpuConstraints) int64

// ----------------------------------------------------------------------------
// Scoring Implementations
// ----------------------------------------------------------------------------

// ScoreByVRAMBinPacking implements the "BestFit (MostAllocated)" strategy for VRAM allocation.
//
// Strategy Philosophy:
// This scorer favors placing workloads on Rigs that will have the *highest* VRAM
// utilization *after* placement. This bin-packing approach consolidates workloads
// onto fewer nodes, leaving other nodes more idle and available for large jobs.
//
// Algorithm:
//  1. Calculate projected VRAM utilization after placing this workload
//  2. Convert utilization ratio (0.0-1.0) to score (0-100)
//  3. Higher utilization = higher score = higher priority
//
// Example Scenario:
//   Rig A: 96 GB total, 40 GB used (56 GB free)
//   Rig B: 48 GB total, 10 GB used (38 GB free)
//   Request: 30 GB
//
//   After placement:
//   Rig A: (40 + 30) / 96 = 72.9% utilization -> score 72
//   Rig B: (10 + 30) / 48 = 83.3% utilization -> score 83
//
//   Result: Rig B is chosen (higher score) because it achieves better bin packing
//
// Benefits:
// - Consolidates workloads onto fewer nodes
// - Keeps some nodes relatively idle for large batch jobs
// - Reduces GPU fragmentation across the cluster
// - Improves overall cluster utilization efficiency
//
// References:
// [4] Kubernetes MostAllocated strategy
// [5] Bin packing algorithms for resource allocation
func ScoreByVRAMBinPacking(rig *RigGpuInfo, constraints *GpuConstraints) int64 {
	requiredVRAM := constraints.RequiredVRAMBytes()

	if rig.TotalVRAMBytes == 0 {
		return 0 // CPU-only node (should have been filtered out)
	}

	// Calculate projected VRAM usage *after* placing this workload
	newUsedVRAM := rig.UsedVRAMBytes + requiredVRAM

	// Safety check: this should not happen if filters are working correctly
	if newUsedVRAM > rig.TotalVRAMBytes {
		return 0
	}

	// Calculate utilization ratio (0.0 to 1.0)
	utilizationRatio := float64(newUsedVRAM) / float64(rig.TotalVRAMBytes)

	// Convert to score (0 to 100)
	// Higher utilization = higher score = higher priority for placement
	score := int64(utilizationRatio * 100)

	return score
}

// ScoreByCudaVersion can be used to prefer Rigs with newer CUDA versions.
// This is optional and typically used with a lower weight than bin packing.
//
// Returns a score based on how much the host CUDA version exceeds the requirement.
// This can be useful for:
// - Preferring newer hardware for better performance
// - Distributing workloads evenly across CUDA versions
//
// Note: This is an example scorer and may not be needed for the initial implementation.
func ScoreByCudaVersion(rig *RigGpuInfo, constraints *GpuConstraints) int64 {
	if constraints.CudaVersionMin == "" {
		return 50 // Neutral score when no CUDA requirement specified
	}

	reqVer, err := constraints.CudaVersion()
	if err != nil || reqVer == nil {
		return 50
	}

	hostVer, err := rig.CudaVersion()
	if err != nil || hostVer == nil {
		return 0
	}

	// Calculate version difference
	// For simplicity, compare major.minor versions
	// e.g., CUDA 12.2 on host, 12.0 required -> difference of 0.2 -> score boost
	versionDiff := hostVer.Major() - reqVer.Major()
	if versionDiff > 0 {
		return 100 // Significantly newer major version
	}

	minorDiff := hostVer.Minor() - reqVer.Minor()
	if minorDiff > 0 {
		// Slightly newer minor version
		// Cap at 100, give 10 points per minor version difference
		score := 50 + (minorDiff * 10)
		if score > 100 {
			score = 100
		}
		return int64(score)
	}

	return 50 // Exact match or very close
}

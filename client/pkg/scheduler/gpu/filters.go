package gpu

import (
	"log"

	"github.com/Masterminds/semver/v3"
)

// FilterFunc is a predicate that determines whether a Rig satisfies the given constraints.
// Returns true if the Rig passes the filter (is eligible), false otherwise.
type FilterFunc func(rig *RigGpuInfo, constraints *GpuConstraints) bool

// ----------------------------------------------------------------------------
// Filter Implementations
// ----------------------------------------------------------------------------

// FilterByVRAM checks if the Rig has sufficient *available* VRAM to satisfy the request.
//
// This filter implements the first stage of GPU-aware scheduling:
// - Exclude Rigs that don't have enough free VRAM for this workload
// - Consider both total VRAM and currently used VRAM (from running capsules)
//
// Example:
//   - Rig: 64 GB total, 40 GB used -> 24 GB available
//   - Request: 30 GB
//   - Result: false (insufficient available VRAM)
func FilterByVRAM(rig *RigGpuInfo, constraints *GpuConstraints) bool {
	requiredVRAM := constraints.RequiredVRAMBytes()
	if requiredVRAM == 0 {
		return true // No VRAM requirement - pass this filter
	}

	availableVRAM := rig.AvailableVRAMBytes()
	return availableVRAM >= requiredVRAM
}

// FilterByCudaVersion checks if the Rig's CUDA driver version meets the minimum requirement.
//
// This filter ensures CUDA compatibility:
// - Uses semantic versioning for accurate comparison (e.g., "12.1.0" >= "12.0.0")
// - CUDA driver backward compatibility: newer drivers support older CUDA versions
// - Returns false if the Rig doesn't support CUDA at all (empty version string)
//
// Example:
//   - Rig: CUDA 12.2
//   - Request: CUDA 12.0 minimum
//   - Result: true (12.2 >= 12.0)
func FilterByCudaVersion(rig *RigGpuInfo, constraints *GpuConstraints) bool {
	if constraints.CudaVersionMin == "" {
		return true // No CUDA requirement - pass this filter
	}

	if rig.CudaDriverVersion == "" {
		return false // Rig doesn't support CUDA (CPU-only node)
	}

	reqVer, err := semver.NewVersion(constraints.CudaVersionMin)
	if err != nil {
		log.Printf("WARN: Invalid CUDA version in constraints: %s (error: %v)", constraints.CudaVersionMin, err)
		return false // Invalid requirement can't be satisfied
	}

	hostVer, err := semver.NewVersion(rig.CudaDriverVersion)
	if err != nil {
		log.Printf("WARN: Invalid CUDA version on Rig %s: %s (error: %v)", rig.RigID, rig.CudaDriverVersion, err)
		return false // Invalid host version can't satisfy requirements
	}

	// Check if host version >= required version
	// semver v3 uses LessThan, so we check if host is NOT less than required
	return !hostVer.LessThan(reqVer)
}

// FilterHasGPU checks if the Rig has any GPU resources at all.
// This is a basic sanity check to exclude CPU-only nodes when GPU is required.
func FilterHasGPU(rig *RigGpuInfo, constraints *GpuConstraints) bool {
	if !constraints.RequiresGPU() {
		return true // Workload doesn't need GPU - any node is OK
	}
	return rig.TotalVRAMBytes > 0
}

// Package gpu implements GPU-aware scheduling with Filter-Score pipeline.
//
// This scheduler follows Kubernetes scheduling philosophy with lightweight
// filter and score functions to intelligently place GPU workloads across
// the cluster using BestFit (MostAllocated) bin packing strategy.
package gpu

import "github.com/Masterminds/semver/v3"

const (
	// Gigabyte represents one gigabyte in bytes (1 GB = 1024^3 bytes)
	Gigabyte = 1024 * 1024 * 1024
)

// RigGpuInfo represents GPU resource information reported by an Agent and stored in the database.
// This corresponds to the Rust RigHardwareReport from the Agent.
type RigGpuInfo struct {
	RigID             string // Unique identifier for the Rig (Agent node)
	TotalVRAMBytes    uint64    // Total VRAM capacity across all GPUs on this Rig
	UsedVRAMBytes     uint64    // Currently reserved VRAM on this Rig (sum of all running capsules)
	CudaDriverVersion string    // CUDA driver version (e.g., "12.2")
	Gpus              []GpuInfo // List of individual GPUs
	IsRemote          bool      // True if the node is connected via Tailnet (Cloud Bursting)
}

// GpuInfo represents a single GPU device
type GpuInfo struct {
	UUID             string
	DeviceName       string
	TotalVRAMBytes   uint64
	AvailableVRAMBytes uint64 // Calculated by scheduler
}

// AvailableVRAMBytes returns the currently available VRAM in bytes.
func (r *RigGpuInfo) AvailableVRAMBytes() uint64 {
	if r.UsedVRAMBytes > r.TotalVRAMBytes {
		return 0 // Safety: should not happen in normal operation
	}
	return r.TotalVRAMBytes - r.UsedVRAMBytes
}

// AvailableVRAMGB returns the currently available VRAM in gigabytes.
func (r *RigGpuInfo) AvailableVRAMGB() float64 {
	return float64(r.AvailableVRAMBytes()) / (1024 * 1024 * 1024)
}

// TotalVRAMGB returns the total VRAM capacity in gigabytes.
func (r *RigGpuInfo) TotalVRAMGB() float64 {
	return float64(r.TotalVRAMBytes) / (1024 * 1024 * 1024)
}

// CudaVersion returns the parsed semantic version of the CUDA driver, or nil if parsing fails.
func (r *RigGpuInfo) CudaVersion() (*semver.Version, error) {
	if r.CudaDriverVersion == "" {
		return nil, nil
	}
	return semver.NewVersion(r.CudaDriverVersion)
}

// GpuConstraints represents GPU resource requirements parsed from adep.json scheduling.constraints.
//
// Example adep.json:
//
//	{
//	  "scheduling": {
//	    "constraints": {
//	      "gpu": {
//	        "vram_min_gb": 16,
//	        "cuda_version_min": "12.1"
//	      }
//	    }
//	  }
//	}
type GpuConstraints struct {
	VramMinGB       uint64 // Minimum required VRAM in GB (0 = no GPU required)
	CudaVersionMin  string // Minimum required CUDA version (empty = no requirement)
	AllowCloudBurst bool   // If true, allows scheduling on remote nodes
}

// RequiresGPU returns true if this workload requires GPU resources.
func (c *GpuConstraints) RequiresGPU() bool {
	return c.VramMinGB > 0 || c.CudaVersionMin != ""
}

// RequiredVRAMBytes returns the required VRAM in bytes.
func (c *GpuConstraints) RequiredVRAMBytes() uint64 {
	return c.VramMinGB * 1024 * 1024 * 1024
}

// CudaVersion returns the parsed semantic version of the minimum CUDA requirement, or nil if empty.
func (c *GpuConstraints) CudaVersion() (*semver.Version, error) {
	if c.CudaVersionMin == "" {
		return nil, nil
	}
	return semver.NewVersion(c.CudaVersionMin)
}

// scoredRig is an internal structure holding a Rig and its computed score.
// Used during the scoring phase to rank Rigs by priority.
type scoredRig struct {
	Rig   *RigGpuInfo
	Score int64 // Higher score = higher priority
}

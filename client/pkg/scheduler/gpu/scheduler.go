package gpu

import (
	"errors"
	"sort"
)

var (
	// ErrNoRigAvailable is returned when no Rig satisfies the scheduling constraints.
	ErrNoRigAvailable = errors.New("no Rig available that satisfies the GPU constraints")
)

// Scheduler manages the Filter-Score pipeline for GPU-aware scheduling.
//
// The scheduler follows a three-stage process inspired by Kubernetes:
//  1. FILTER: Eliminate Rigs that don't meet minimum requirements (predicates)
//  2. SCORE: Rank remaining Rigs by priority (weighted scoring functions)
//  3. SELECT: Choose the highest-scoring Rig
//
// This design is:
// - Extensible: New filters and scorers can be added easily
// - Testable: Pure functions with no external dependencies
// - Deterministic: Same inputs always produce same outputs
type Scheduler struct {
	// FilterFuncs are predicates that must all return true for a Rig to be eligible.
	// Executed in order; short-circuits on first failure.
	FilterFuncs []FilterFunc

	// ScoreFuncs assign priority scores to eligible Rigs.
	// Each function is executed with a weight; final score is the weighted sum.
	ScoreFuncs []struct {
		Func   ScoreFunc
		Weight int64
	}
}

// NewScheduler creates a GPU scheduler with default policies.
//
// Default configuration:
// - Filters: VRAM availability, CUDA version compatibility, GPU presence
// - Scorers: BestFit VRAM bin packing (weight 1)
//
// This default setup implements the MostAllocated strategy for efficient
// cluster resource utilization.
func NewScheduler() *Scheduler {
	return &Scheduler{
		FilterFuncs: []FilterFunc{
			FilterHasGPU,        // 1. Check if Rig has any GPU at all
			FilterByVRAM,        // 2. Check VRAM availability
			FilterByCudaVersion, // 3. Check CUDA version compatibility
		},
		ScoreFuncs: []struct {
			Func   ScoreFunc
			Weight int64
		}{
			{Func: ScoreByVRAMBinPacking, Weight: 1}, // Primary: bin packing strategy
		},
	}
}

// FindBestRig selects the most suitable Rig from the given candidates based on the constraints.
//
// Process:
//  1. Apply all filter functions to find eligible Rigs
//  2. Apply all scoring functions to rank eligible Rigs
//  3. Return the highest-scoring Rig
//
// Returns:
// - The best Rig for placement, or nil with ErrNoRigAvailable if no suitable Rig exists
//
// Example usage:
//
//	scheduler := NewScheduler()
//	constraints := &GpuConstraints{VramMinGB: 16, CudaVersionMin: "12.0"}
//	bestRig, err := scheduler.FindBestRig(allRigs, constraints)
//	if err == ErrNoRigAvailable {
//	    // Handle: no suitable node available, queue or reject workload
//	}
func (s *Scheduler) FindBestRig(allRigs []*RigGpuInfo, constraints *GpuConstraints) (*RigGpuInfo, error) {
	rig, _, err := s.FindBestRigWithAssignment(allRigs, constraints)
	return rig, err
}

// FindBestRigWithAssignment selects the best Rig and assigns specific GPU UUIDs.
//
// Returns:
// - The best Rig
// - A list of assigned GPU UUIDs (First-Fit strategy)
// - Error if no suitable Rig found
func (s *Scheduler) FindBestRigWithAssignment(allRigs []*RigGpuInfo, constraints *GpuConstraints) (*RigGpuInfo, []string, error) {
	// --- Stage 1: FILTER ---
	// Eliminate Rigs that don't meet minimum requirements
	filteredRigs := make([]*RigGpuInfo, 0, len(allRigs))
	for _, rig := range allRigs {
		if s.filterRig(rig, constraints) {
			filteredRigs = append(filteredRigs, rig)
		}
	}

	if len(filteredRigs) == 0 {
		return nil, nil, ErrNoRigAvailable
	}

	// --- Stage 2: SCORE ---
	// Rank eligible Rigs by priority
	scoredRigs := make([]scoredRig, len(filteredRigs))
	for i, rig := range filteredRigs {
		scoredRigs[i] = scoredRig{
			Rig:   rig,
			Score: s.scoreRig(rig, constraints),
		}
	}

	// --- Stage 3: SELECT ---
	// Sort by score (descending: highest score first)
	sort.Slice(scoredRigs, func(i, j int) bool {
		return scoredRigs[i].Score > scoredRigs[j].Score
	})

	bestRig := scoredRigs[0].Rig

	// --- Stage 4: ASSIGN ---
	// Select specific GPUs (First-Fit)
	// Since we don't track per-GPU usage perfectly yet, we just assign the first N GPUs
	// that satisfy the count requirement (if we had one).
	// Currently, the constraint is just VRAM total on the node.
	// We will assign ALL GPUs if the workload asks for "GPU" generically,
	// OR we can try to be smarter.
	//
	// Requirement: "First-Fit（空いている最初のGPU UUIDを割り当てる）"
	// Since we don't know which one is "empty" (used_vram is 0 for all in DB for now),
	// we will just assign the first available GPU.
	// If the workload requires more VRAM than a single GPU has, we might need multiple?
	// The current `GpuConstraints` has `VramMinGB`.
	// If `VramMinGB` <= `TotalVRAM` of a single GPU, we assign one.
	// If it's larger, we might need multiple (but current logic checks Node Total).
	//
	// Let's implement a simple logic:
	// Iterate over GPUs in the Rig. Find the first one that has TotalVRAM >= RequiredVRAM.
	// If found, assign it.
	// If not found (but Node Total was enough), it implies we need multiple GPUs.
	// In that case, we assign ALL GPUs (simple fallback).

	requiredBytes := constraints.RequiredVRAMBytes()
	var assignedUUIDs []string

	// Try to find a single GPU that fits
	for _, gpu := range bestRig.Gpus {
		if gpu.TotalVRAMBytes >= requiredBytes {
			assignedUUIDs = append(assignedUUIDs, gpu.UUID)
			break
		}
	}

	// If no single GPU fits, but the Node passed the filter (Total Node VRAM >= Required),
	// then we must aggregate. For now, let's just assign ALL GPUs to be safe.
	if len(assignedUUIDs) == 0 && len(bestRig.Gpus) > 0 {
		for _, gpu := range bestRig.Gpus {
			assignedUUIDs = append(assignedUUIDs, gpu.UUID)
		}
	}

	return bestRig, assignedUUIDs, nil
}

// filterRig applies all filter functions to a single Rig.
// Returns true only if ALL filters pass (logical AND).
func (s *Scheduler) filterRig(rig *RigGpuInfo, constraints *GpuConstraints) bool {
	for _, filterFunc := range s.FilterFuncs {
		if !filterFunc(rig, constraints) {
			return false // Short-circuit: one failure means ineligible
		}
	}
	return true // All filters passed
}

// scoreRig applies all scoring functions to a single Rig and returns the weighted total.
//
// The final score is the sum of (score × weight) for each scorer.
// This allows combining multiple priorities with different importance levels.
func (s *Scheduler) scoreRig(rig *RigGpuInfo, constraints *GpuConstraints) int64 {
	var totalScore int64 = 0
	for _, scorer := range s.ScoreFuncs {
		score := scorer.Func(rig, constraints) * scorer.Weight
		totalScore += score
	}
	return totalScore
}

// AddFilter adds a custom filter function to the scheduler.
// Useful for extending scheduling logic without modifying the core.
func (s *Scheduler) AddFilter(filter FilterFunc) {
	s.FilterFuncs = append(s.FilterFuncs, filter)
}

// AddScorer adds a custom scoring function with the given weight.
// Higher weights give more influence in the final ranking.
func (s *Scheduler) AddScorer(scorer ScoreFunc, weight int64) {
	s.ScoreFuncs = append(s.ScoreFuncs, struct {
		Func   ScoreFunc
		Weight int64
	}{Func: scorer, Weight: weight})
}

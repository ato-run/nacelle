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
	// --- Stage 1: FILTER ---
	// Eliminate Rigs that don't meet minimum requirements
	filteredRigs := make([]*RigGpuInfo, 0, len(allRigs))
	for _, rig := range allRigs {
		if s.filterRig(rig, constraints) {
			filteredRigs = append(filteredRigs, rig)
		}
	}

	if len(filteredRigs) == 0 {
		return nil, ErrNoRigAvailable
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

	// Return the highest-scoring Rig
	return scoredRigs[0].Rig, nil
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

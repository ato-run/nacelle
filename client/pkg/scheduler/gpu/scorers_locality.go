package gpu

// ScoreByLocality prefers local nodes over remote nodes.
//
// Strategy:
// - Local nodes get a high score (100)
// - Remote nodes get a low score (0)
//
// This ensures that we only burst to the cloud if local resources are exhausted
// (or if other scorers like VRAM bin packing heavily favor the remote node,
// but typically locality is a strong preference).
func ScoreByLocality(rig *RigGpuInfo, constraints *GpuConstraints) int64 {
	if !rig.IsRemote {
		return 100
	}
	return 0
}

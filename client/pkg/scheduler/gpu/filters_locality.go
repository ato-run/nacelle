package gpu

// FilterByLocality checks if the Rig satisfies the locality constraints.
//
// If AllowCloudBurst is false, it rejects remote nodes.
// If AllowCloudBurst is true, it allows both local and remote nodes.
func FilterByLocality(rig *RigGpuInfo, constraints *GpuConstraints) bool {
	if !constraints.AllowCloudBurst && rig.IsRemote {
		return false
	}
	return true
}

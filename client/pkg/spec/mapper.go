package spec

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"
	pb "github.com/onescluster/coordinator/pkg/proto"
	"github.com/onescluster/coordinator/pkg/router"
)

const defaultStorageMountBase = "/var/lib/gumball/volumes"

func storageMountBase() string {
	if v := strings.TrimSpace(os.Getenv("GUMBALL_STORAGE_BASE")); v != "" {
		return v
	}
	return defaultStorageMountBase
}

// ParseRunPlanFile parses a manifest file (either capsule_v1 style or [package]/[run] style)
// and returns a proto RunPlan plus a suggested capsule ID.
// ParseRunPlanFile parses a manifest file (either capsule_v1 style or [package]/[run] style)
// and returns a proto RunPlan plus a suggested capsule ID.
func ParseRunPlanFile(path string) (string, *pb.RunPlan, error) {
	contentBytes, err := os.ReadFile(path)
	if err != nil {
		return "", nil, fmt.Errorf("read manifest: %w", err)
	}
	return ParseRunPlanContent(string(contentBytes))
}

// ParseRunPlanContent parses a manifest string (either capsule_v1 style or [package]/[run] style)
// and returns a proto RunPlan plus a suggested capsule ID.
func ParseRunPlanContent(content string) (string, *pb.RunPlan, error) {
	// Try capsule_v1 style first.
	{
		var spec CapsuleSpec
		md, err := toml.Decode(content, &spec)
		if err == nil && (md.IsDefined("schema_version") || md.IsDefined("execution", "runtime") || md.IsDefined("execution", "entrypoint")) {
			plan, err := ToRunPlan(&spec)
			if err != nil {
				return "", nil, err
			}
			return plan.CapsuleId, plan, nil
		}
	}

	// Fallback: [package]/[run] style.
	{
		var manifest CapsuleManifestV1
		md, err := toml.Decode(content, &manifest)
		if err == nil && (md.IsDefined("package", "name") || md.IsDefined("run", "image")) {
			plan, err := ToRunPlanFromPackageRun(&manifest)
			if err != nil {
				return "", nil, err
			}
			capsuleID := plan.CapsuleId
			if capsuleID == "" {
				capsuleID = manifest.Package.Name
			}
			return capsuleID, plan, nil
		}
	}

	// Legacy: [capsule] / [runtime] style.
	{
		var legacy LegacyCapsuleManifest
		md, err := toml.Decode(content, &legacy)
		if err == nil && (md.IsDefined("capsule", "name") || md.IsDefined("runtime", "type")) {
			plan, err := ToRunPlanFromLegacy(&legacy)
			if err != nil {
				return "", nil, err
			}
			capsuleID := plan.CapsuleId
			if capsuleID == "" {
				capsuleID = legacy.Capsule.Name
			}
			return capsuleID, plan, nil
		}
	}

	return "", nil, fmt.Errorf("unrecognized manifest schema")
}

// ParseFile parses a capsule.toml v1.0 file.
func ParseFile(path string) (*CapsuleSpec, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read capsule spec: %w", err)
	}
	return ParseString(string(data))
}

// ParseString parses capsule.toml content (schema_version=1.0).
func ParseString(content string) (*CapsuleSpec, error) {
	var spec CapsuleSpec
	if _, err := toml.Decode(content, &spec); err != nil {
		return nil, fmt.Errorf("parse capsule spec TOML: %w", err)
	}
	return &spec, nil
}

// ToRunPlan converts a capsule.toml v1.0 spec into a proto RunPlan.
// This mirrors libadep-core's CapsuleManifestV1::to_run_plan.
func ToRunPlan(spec *CapsuleSpec) (*pb.RunPlan, error) {
	if spec == nil {
		return nil, fmt.Errorf("spec is nil")
	}
	if err := validate(spec); err != nil {
		return nil, err
	}

	ports := make([]*pb.Port, 0, 1)
	if spec.Execution.Port != nil && *spec.Execution.Port != 0 {
		ports = append(ports, &pb.Port{
			ContainerPort: uint32(*spec.Execution.Port),
			Protocol:      "tcp",
		})
	}

	env := spec.Execution.Env
	if env == nil {
		env = map[string]string{}
	}

	plan := &pb.RunPlan{
		CapsuleId: spec.Name,
		Name:      spec.Name,
		Version:   spec.Version,
	}

	// v1 uses vram_min as a memory hint (mirrors Rust).
	if spec.Requirements.VRAMMin != nil {
		bytes, err := router.ParseSize(*spec.Requirements.VRAMMin)
		if err != nil {
			return nil, fmt.Errorf("invalid requirements.vram_min: %w", err)
		}
		if bytes > 0 {
			plan.MemoryBytes = uint64(bytes)
		}
	}

	switch spec.Execution.Runtime {
	case RuntimeTypeDocker:
		docker := &pb.DockerRuntime{
			Image: spec.Execution.Entrypoint,
			Env:   env,
			Ports: ports,
		}
		// v1 storage volumes are mapped to Docker bind mounts (HostPath -> ContainerPath).
		if spec.Storage != nil && len(spec.Storage.Volumes) > 0 {
			base := storageMountBase()
			for _, vol := range spec.Storage.Volumes {
				name := strings.TrimSpace(vol.Name)
				mountPath := strings.TrimSpace(vol.MountPath)
				if name == "" {
					return nil, fmt.Errorf("storage.volumes[].name is required")
				}
				if mountPath == "" {
					return nil, fmt.Errorf("storage.volumes[%s].mount_path is required", name)
				}
				cleanTarget := filepath.Clean(mountPath)
				if !strings.HasPrefix(cleanTarget, "/") || strings.Contains(cleanTarget, "..") {
					return nil, fmt.Errorf("invalid storage.volumes[%s].mount_path: %q", name, mountPath)
				}
				hostPath := filepath.Join(base, spec.Name, name)
				docker.Mounts = append(docker.Mounts, &pb.Mount{
					Source:   hostPath,
					Target:   cleanTarget,
					Readonly: vol.ReadOnly,
				})
			}
		}
		plan.Runtime = &pb.RunPlan_Docker{Docker: docker}
	case RuntimeTypeNative:
		plan.Runtime = &pb.RunPlan_Native{Native: &pb.NativeRuntime{
			BinaryPath: spec.Execution.Entrypoint,
			Env:        env,
		}}
	case RuntimeTypePythonUV:
		plan.Runtime = &pb.RunPlan_PythonUv{PythonUv: &pb.PythonUvRuntime{
			Entrypoint: spec.Execution.Entrypoint,
			Env:        env,
			Ports:      ports,
		}}
	default:
		return nil, fmt.Errorf("unsupported execution.runtime: %q", spec.Execution.Runtime)
	}

	// Reject storage config for non-docker runtimes in this minimal slice.
	if spec.Execution.Runtime != RuntimeTypeDocker && spec.Storage != nil && len(spec.Storage.Volumes) > 0 {
		return nil, fmt.Errorf("storage volumes are only supported for execution.runtime=docker")
	}

	return plan, nil
}

// ToRunPlanFromPackageRun converts a [package]/[run] style manifest into a proto RunPlan.
func ToRunPlanFromPackageRun(manifest *CapsuleManifestV1) (*pb.RunPlan, error) {
	if manifest == nil {
		return nil, fmt.Errorf("manifest is nil")
	}
	if strings.TrimSpace(manifest.Package.Name) == "" {
		return nil, fmt.Errorf("package.name is required")
	}
	if strings.TrimSpace(manifest.Package.Version) == "" {
		return nil, fmt.Errorf("package.version is required")
	}
	if strings.TrimSpace(manifest.Run.Image) == "" {
		return nil, fmt.Errorf("run.image is required")
	}

	env := manifest.Run.Env
	if env == nil {
		env = map[string]string{}
	}

	plan := &pb.RunPlan{
		CapsuleId: manifest.Package.Name,
		Name:      manifest.Package.Name,
		Version:   manifest.Package.Version,
		Runtime: &pb.RunPlan_Docker{Docker: &pb.DockerRuntime{
			Image:   manifest.Run.Image,
			Command: manifest.Run.Cmd,
			Env:     env,
		}},
	}

	// Map Network config
	if manifest.Network != nil {
		docker := plan.GetDocker()
		if docker != nil {
			if manifest.Network.HttpPort != 0 {
				docker.Ports = append(docker.Ports, &pb.Port{
					ContainerPort: uint32(manifest.Network.HttpPort),
					Protocol:      "tcp",
				})
			}
			if manifest.Network.Public {
				if docker.Env == nil {
					docker.Env = make(map[string]string)
				}
				docker.Env["GUMBALL_PUBLIC"] = "true"
			}
		}
	}

	// MVP-ish resource mapping.
	if manifest.Resources != nil {
		gpu := strings.TrimSpace(manifest.Resources.GPU)
		if gpu != "" && gpu != "none" {
			plan.GpuProfile = gpu
		}
		if mem := strings.TrimSpace(manifest.Resources.Memory); mem != "" {
			bytes, err := parseByteString(mem)
			if err != nil {
				return nil, fmt.Errorf("invalid resources.memory: %w", err)
			}
			if bytes > 0 {
				plan.MemoryBytes = bytes
			}
		}
		if cpu := strings.TrimSpace(manifest.Resources.CPU); cpu != "" {
			cores, err := parseCPUCores(cpu)
			if err != nil {
				return nil, fmt.Errorf("invalid resources.cpu: %w", err)
			}
			plan.CpuCores = cores
		}
	}

	return plan, nil
}

// ToRunPlanFromLegacy converts a legacy [capsule]/[runtime] style manifest into a proto RunPlan.
func ToRunPlanFromLegacy(manifest *LegacyCapsuleManifest) (*pb.RunPlan, error) {
	if manifest == nil {
		return nil, fmt.Errorf("manifest is nil")
	}
	if strings.TrimSpace(manifest.Capsule.Name) == "" {
		return nil, fmt.Errorf("capsule.name is required")
	}
	if strings.TrimSpace(manifest.Capsule.Version) == "" {
		return nil, fmt.Errorf("capsule.version is required")
	}
	if manifest.Runtime == nil {
		return nil, fmt.Errorf("runtime is required")
	}
	rt := strings.ToLower(strings.TrimSpace(manifest.Runtime.Type))
	if rt == "" {
		return nil, fmt.Errorf("runtime.type is required")
	}

	env := manifest.Runtime.Env
	if env == nil {
		env = map[string]string{}
	}

	plan := &pb.RunPlan{
		CapsuleId: manifest.Capsule.Name,
		Name:      manifest.Capsule.Name,
		Version:   manifest.Capsule.Version,
	}

	if manifest.Resources != nil {
		if manifest.Resources.CPUCores != nil {
			plan.CpuCores = *manifest.Resources.CPUCores
		}
		if manifest.Resources.Memory != nil {
			bytes, err := parseByteString(*manifest.Resources.Memory)
			if err != nil {
				return nil, fmt.Errorf("invalid resources.memory: %w", err)
			}
			if bytes > 0 {
				plan.MemoryBytes = bytes
			}
		}
	}

	switch rt {
	case "docker":
		if manifest.Runtime.Executable == nil || strings.TrimSpace(*manifest.Runtime.Executable) == "" {
			return nil, fmt.Errorf("runtime.executable is required for docker")
		}
		plan.Runtime = &pb.RunPlan_Docker{Docker: &pb.DockerRuntime{
			Image:   strings.TrimSpace(*manifest.Runtime.Executable),
			Command: manifest.Runtime.Args,
			Env:     env,
		}}
	case "native":
		if manifest.Runtime.Executable == nil || strings.TrimSpace(*manifest.Runtime.Executable) == "" {
			return nil, fmt.Errorf("runtime.executable is required for native")
		}
		plan.Runtime = &pb.RunPlan_Native{Native: &pb.NativeRuntime{
			BinaryPath: strings.TrimSpace(*manifest.Runtime.Executable),
			Args:       manifest.Runtime.Args,
			Env:        env,
		}}
	default:
		return nil, fmt.Errorf("unsupported legacy runtime.type: %q", manifest.Runtime.Type)
	}

	return plan, nil
}

func parseCPUCores(s string) (uint32, error) {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0, nil
	}
	if strings.HasSuffix(s, "m") {
		milli, err := strconv.ParseFloat(strings.TrimSuffix(s, "m"), 64)
		if err != nil {
			return 0, err
		}
		if milli <= 0 {
			return 0, nil
		}
		cores := milli / 1000.0
		if cores < 1.0 {
			return 0, nil
		}
		return uint32(cores + 0.5), nil
	}
	cores, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return 0, err
	}
	if cores <= 0 {
		return 0, nil
	}
	return uint32(cores + 0.5), nil
}

// parseByteString parses common Kubernetes-style memory strings like "64Mi" / "1Gi".
func parseByteString(s string) (uint64, error) {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0, fmt.Errorf("empty")
	}

	// Support binary suffixes Ki/Mi/Gi/Ti.
	re := regexp.MustCompile(`(?i)^(\d+(?:\.\d+)?)\s*(Ki|Mi|Gi|Ti)$`)
	if m := re.FindStringSubmatch(s); m != nil {
		v, err := strconv.ParseFloat(m[1], 64)
		if err != nil {
			return 0, err
		}
		if v <= 0 {
			return 0, nil
		}
		unit := strings.ToLower(m[2])
		mult := float64(1)
		switch unit {
		case "ki":
			mult = 1024
		case "mi":
			mult = 1024 * 1024
		case "gi":
			mult = 1024 * 1024 * 1024
		case "ti":
			mult = 1024 * 1024 * 1024 * 1024
		}
		return uint64(v * mult), nil
	}

	// Fallback: existing size parser for GB/MB/etc.
	bytes, err := router.ParseSize(s)
	if err != nil {
		return 0, err
	}
	if bytes <= 0 {
		return 0, nil
	}
	return uint64(bytes), nil
}

func validate(spec *CapsuleSpec) error {
	if strings.TrimSpace(spec.SchemaVersion) != "1.0" {
		return fmt.Errorf("invalid schema_version %q (expected %q)", spec.SchemaVersion, "1.0")
	}
	if strings.TrimSpace(spec.Name) == "" {
		return fmt.Errorf("name is required")
	}
	if strings.TrimSpace(spec.Version) == "" {
		return fmt.Errorf("version is required")
	}
	if strings.TrimSpace(spec.Execution.Entrypoint) == "" {
		return fmt.Errorf("execution.entrypoint is required")
	}
	if strings.TrimSpace(string(spec.Execution.Runtime)) == "" {
		return fmt.Errorf("execution.runtime is required")
	}
	if spec.Storage != nil {
		seen := map[string]struct{}{}
		for _, vol := range spec.Storage.Volumes {
			name := strings.TrimSpace(vol.Name)
			if name == "" {
				return fmt.Errorf("storage.volumes[].name is required")
			}
			if _, ok := seen[name]; ok {
				return fmt.Errorf("duplicate storage volume name: %q", name)
			}
			seen[name] = struct{}{}
			mountPath := strings.TrimSpace(vol.MountPath)
			if mountPath == "" {
				return fmt.Errorf("storage.volumes[%s].mount_path is required", name)
			}
			cleanTarget := filepath.Clean(mountPath)
			if !strings.HasPrefix(cleanTarget, "/") || strings.Contains(cleanTarget, "..") {
				return fmt.Errorf("invalid storage.volumes[%s].mount_path: %q", name, mountPath)
			}
		}
	}
	return nil
}

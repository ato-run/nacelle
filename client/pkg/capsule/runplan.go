package capsule

import (
	pb "github.com/onescluster/coordinator/pkg/proto"
)

// RunPlan is the normalized execution plan shared between runtimes.
// Mirrors libadep-core's runplan.rs shape.
type RunPlan struct {
	CapsuleID       string           `json:"capsule_id"`
	Name            string           `json:"name"`
	Version         string           `json:"version"`
	Docker          *DockerRuntime   `json:"docker,omitempty"`
	Native          *NativeRuntime   `json:"native,omitempty"`
	PythonUv        *PythonUvRuntime `json:"python_uv,omitempty"`
	CPUCores        *uint32          `json:"cpu_cores,omitempty"`
	MemoryBytes     *uint64          `json:"memory_bytes,omitempty"`
	GPUProfile      string           `json:"gpu_profile,omitempty"`
	EgressAllowlist []string         `json:"egress_allowlist,omitempty"`
}

type DockerRuntime struct {
	Image      string            `json:"image"`
	Digest     string            `json:"digest,omitempty"`
	Command    []string          `json:"command,omitempty"`
	Env        map[string]string `json:"env,omitempty"`
	WorkingDir string            `json:"working_dir,omitempty"`
	User       string            `json:"user,omitempty"`
	Ports      []Port            `json:"ports,omitempty"`
	Mounts     []Mount           `json:"mounts,omitempty"`
}

type NativeRuntime struct {
	BinaryPath string            `json:"binary_path"`
	Args       []string          `json:"args,omitempty"`
	Env        map[string]string `json:"env,omitempty"`
	WorkingDir string            `json:"working_dir,omitempty"`
}

type PythonUvRuntime struct {
	Entrypoint string            `json:"entrypoint"`
	Args       []string          `json:"args,omitempty"`
	Env        map[string]string `json:"env,omitempty"`
	WorkingDir string            `json:"working_dir,omitempty"`
	Ports      []Port            `json:"ports,omitempty"`
}

type Port struct {
	ContainerPort uint32  `json:"container_port"`
	HostPort      *uint32 `json:"host_port,omitempty"`
	Protocol      string  `json:"protocol,omitempty"`
}

type Mount struct {
	Source   string `json:"source"`
	Target   string `json:"target"`
	Readonly bool   `json:"readonly"`
}

// ToRunPlan converts a CapsuleManifest into a RunPlan (v0).
func (m *CapsuleManifest) ToRunPlan() (*RunPlan, error) {
	// Validate basic fields are present
	if m.Name == "" {
		return nil, ErrInvalidName
	}
	if m.Version == "" {
		return nil, ErrInvalidVersion
	}

	rp := &RunPlan{
		CapsuleID: m.Name,
		Name:      m.Name,
		Version:   m.Version,
	}

	// Memory hint from vram_min if present
	if m.Requirements.VRAMMin != "" {
		if bytes, err := parseMemoryString(m.Requirements.VRAMMin); err == nil {
			b := uint64(bytes)
			rp.MemoryBytes = &b
		}
	}

	// Ports helper
	var ports []Port
	if m.Execution.Port > 0 {
		ports = append(ports, Port{ContainerPort: uint32(m.Execution.Port), Protocol: "tcp"})
	}

	switch m.Execution.Runtime {
	case RuntimeDocker:
		rp.Docker = &DockerRuntime{
			Image:   m.Execution.Entrypoint,
			Command: []string{},
			Env:     cloneEnv(m.Execution.Env),
			Ports:   ports,
			Mounts:  []Mount{},
		}
	case RuntimeNative:
		rp.Native = &NativeRuntime{
			BinaryPath: m.Execution.Entrypoint,
			Args:       []string{},
			Env:        cloneEnv(m.Execution.Env),
		}
	case RuntimePythonUv:
		rp.PythonUv = &PythonUvRuntime{
			Entrypoint: m.Execution.Entrypoint,
			Args:       []string{},
			Env:        cloneEnv(m.Execution.Env),
			Ports:      ports,
		}
	default:
		return nil, ErrInvalidRuntime
	}

	return rp, nil
}

func cloneEnv(env map[string]string) map[string]string {
	if env == nil {
		return nil
	}
	out := make(map[string]string, len(env))
	for k, v := range env {
		out[k] = v
	}
	return out
}

// ToProto converts internal RunPlan to proto RunPlan (coordinator.proto)
func (rp *RunPlan) ToProto() *pb.RunPlan {
	if rp == nil {
		return nil
	}

	proto := &pb.RunPlan{
		CapsuleId:       rp.CapsuleID,
		Name:            rp.Name,
		Version:         rp.Version,
		GpuProfile:      rp.GPUProfile,
		EgressAllowlist: rp.EgressAllowlist,
	}

	if rp.CPUCores != nil {
		proto.CpuCores = *rp.CPUCores
	}
	if rp.MemoryBytes != nil {
		proto.MemoryBytes = *rp.MemoryBytes
	}

	if rp.Docker != nil {
		ports := make([]*pb.Port, len(rp.Docker.Ports))
		for i, p := range rp.Docker.Ports {
			ports[i] = &pb.Port{
				ContainerPort: p.ContainerPort,
				Protocol:      p.Protocol,
			}
			if p.HostPort != nil {
				ports[i].HostPort = *p.HostPort
			}
		}

		mounts := make([]*pb.Mount, len(rp.Docker.Mounts))
		for i, m := range rp.Docker.Mounts {
			mounts[i] = &pb.Mount{
				Source:   m.Source,
				Target:   m.Target,
				Readonly: m.Readonly,
			}
		}

		proto.Runtime = &pb.RunPlan_Docker{
			Docker: &pb.DockerRuntime{
				Image:      rp.Docker.Image,
				Digest:     rp.Docker.Digest,
				Command:    rp.Docker.Command,
				Env:        rp.Docker.Env,
				WorkingDir: rp.Docker.WorkingDir,
				User:       rp.Docker.User,
				Ports:      ports,
				Mounts:     mounts,
			},
		}
	} else if rp.Native != nil {
		proto.Runtime = &pb.RunPlan_Native{
			Native: &pb.NativeRuntime{
				BinaryPath: rp.Native.BinaryPath,
				Args:       rp.Native.Args,
				Env:        rp.Native.Env,
				WorkingDir: rp.Native.WorkingDir,
			},
		}
	} else if rp.PythonUv != nil {
		ports := make([]*pb.Port, len(rp.PythonUv.Ports))
		for i, p := range rp.PythonUv.Ports {
			ports[i] = &pb.Port{
				ContainerPort: p.ContainerPort,
				Protocol:      p.Protocol,
			}
			if p.HostPort != nil {
				ports[i].HostPort = *p.HostPort
			}
		}

		proto.Runtime = &pb.RunPlan_PythonUv{
			PythonUv: &pb.PythonUvRuntime{
				Entrypoint: rp.PythonUv.Entrypoint,
				Args:       rp.PythonUv.Args,
				Env:        rp.PythonUv.Env,
				WorkingDir: rp.PythonUv.WorkingDir,
				Ports:      ports,
			},
		}
	}

	return proto
}

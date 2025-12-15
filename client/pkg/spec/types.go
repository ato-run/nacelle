package spec

// --- Alternate (package/run) manifest shape ---
// Some tooling uses a simpler TOML layout:
//
//   [package]
//   name = "..."
//   version = "..."
//
//   [run]
//   image = "..."
//   cmd = ["..."]
//   [run.env]
//   KEY = "VALUE"
//
// This is supported by capsule-cli for Phase 2 final verification.

// CapsuleManifestV1 maps to the simple capsule.toml layout using [package]/[run] tables.
type CapsuleManifestV1 struct {
	Package   PackageMetadata `toml:"package"`
	Run       RunConfig       `toml:"run"`
	Resources *ResourceConfig `toml:"resources"`
	Network   *NetworkConfig  `toml:"network"`
}

type PackageMetadata struct {
	Name    string `toml:"name"`
	Version string `toml:"version"`
}

type RunConfig struct {
	Image string            `toml:"image"`
	Cmd   []string          `toml:"cmd"`
	Env   map[string]string `toml:"env"`
}

type ResourceConfig struct {
	CPU    string `toml:"cpu"`
	Memory string `toml:"memory"`
	GPU    string `toml:"gpu"`
}

type NetworkConfig struct {
	Public   bool   `toml:"public"`
	HttpPort uint16 `toml:"http_port"`
}

// --- Legacy manifest shape ---
// Mirrors libadep-core's capsule_manifest::CapsuleManifest (TOML mode), which uses
// top-level [capsule] and [runtime] tables.
//
// This is used as a compatibility layer for older Engine deployments that do not
// yet accept DeployRequest.run_plan.

type LegacyCapsuleManifest struct {
	Capsule   LegacyCapsuleMetadata `toml:"capsule"`
	Runtime   *LegacyRuntimeConfig  `toml:"runtime"`
	Resources *LegacyResources      `toml:"resources"`
}

type LegacyCapsuleMetadata struct {
	Name        string  `toml:"name"`
	Version     string  `toml:"version"`
	Description *string `toml:"description"`
}

type LegacyRuntimeConfig struct {
	Type       string            `toml:"type"`
	Executable *string           `toml:"executable"`
	Args       []string          `toml:"args"`
	Env        map[string]string `toml:"env"`
}

type LegacyResources struct {
	CPUCores *uint32 `toml:"cpu_cores"`
	Memory   *string `toml:"memory"`
}

// CapsuleType defines the fundamental nature of the Capsule.
// Mirrors libadep-core's capsule_v1::CapsuleType (serde: lowercase).
type CapsuleType string

const (
	CapsuleTypeInference CapsuleType = "inference"
	CapsuleTypeTool      CapsuleType = "tool"
	CapsuleTypeApp       CapsuleType = "app"
)

// RuntimeType defines how the Capsule is executed.
// Mirrors libadep-core's capsule_v1::RuntimeType (serde: kebab-case).
type RuntimeType string

const (
	RuntimeTypePythonUV RuntimeType = "python-uv"
	RuntimeTypeDocker   RuntimeType = "docker"
	RuntimeTypeNative   RuntimeType = "native"
)

// RouteWeight determines local vs cloud routing.
// Mirrors libadep-core's capsule_v1::RouteWeight (serde: lowercase).
type RouteWeight string

const (
	RouteWeightLight RouteWeight = "light"
	RouteWeightHeavy RouteWeight = "heavy"
)

// Quantization is a model quantization format.
// Mirrors libadep-core's capsule_v1::Quantization (serde: lowercase, with 8bit/4bit).
type Quantization string

const (
	QuantizationFP16 Quantization = "fp16"
	QuantizationBF16 Quantization = "bf16"
	Quantization8Bit Quantization = "8bit"
	Quantization4Bit Quantization = "4bit"
)

// Platform is a platform target.
// Mirrors libadep-core's capsule_v1::Platform (serde: kebab-case).
type Platform string

const (
	PlatformDarwinArm64  Platform = "darwin-arm64"
	PlatformDarwinX86_64 Platform = "darwin-x86-64"
	PlatformLinuxAmd64   Platform = "linux-amd64"
	PlatformLinuxArm64   Platform = "linux-arm64"
)

// CapsuleSpec represents capsule.toml schema_version=1.0.
// Mirrors libadep-core's capsule_v1::CapsuleManifestV1.
type CapsuleSpec struct {
	SchemaVersion string      `toml:"schema_version"`
	Name          string      `toml:"name"`
	Version       string      `toml:"version"`
	CapsuleType   CapsuleType `toml:"type"`

	Metadata     CapsuleMetadata      `toml:"metadata"`
	Capabilities *CapsuleCapabilities `toml:"capabilities"`
	Requirements CapsuleRequirements  `toml:"requirements"`
	Execution    CapsuleExecution     `toml:"execution"`
	Storage      *CapsuleStorage      `toml:"storage"`
	Routing      CapsuleRouting       `toml:"routing"`
	Model        *ModelConfig         `toml:"model"`
}

// CapsuleStorage declares per-capsule persistent volumes.
// These volumes are resolved to host bind mounts by the deploy coordinator.
type CapsuleStorage struct {
	Volumes []StorageVolume `toml:"volumes"`
}

type StorageVolume struct {
	Name      string `toml:"name"`
	MountPath string `toml:"mount_path"`
	ReadOnly  bool   `toml:"read_only"`
}

// CapsuleMetadata is human-readable metadata.
type CapsuleMetadata struct {
	DisplayName *string  `toml:"display_name"`
	Description *string  `toml:"description"`
	Author      *string  `toml:"author"`
	Icon        *string  `toml:"icon"`
	Tags        []string `toml:"tags"`
}

// CapsuleCapabilities describes inference capabilities.
type CapsuleCapabilities struct {
	Chat            bool    `toml:"chat"`
	FunctionCalling bool    `toml:"function_calling"`
	Vision          bool    `toml:"vision"`
	ContextLength   *uint32 `toml:"context_length"`
}

// CapsuleRequirements describes system requirements.
type CapsuleRequirements struct {
	Platform        []Platform `toml:"platform"`
	VRAMMin         *string    `toml:"vram_min"`
	VRAMRecommended *string    `toml:"vram_recommended"`
	Disk            *string    `toml:"disk"`
	Dependencies    []string   `toml:"dependencies"`
}

// SignalConfig configures graceful shutdown signals.
type SignalConfig struct {
	Stop string `toml:"stop"`
	Kill string `toml:"kill"`
}

// CapsuleExecution describes execution configuration.
type CapsuleExecution struct {
	Runtime        RuntimeType       `toml:"runtime"`
	Entrypoint     string            `toml:"entrypoint"`
	Port           *uint16           `toml:"port"`
	HealthCheck    *string           `toml:"health_check"`
	StartupTimeout uint32            `toml:"startup_timeout"`
	Env            map[string]string `toml:"env"`
	Signals        SignalConfig      `toml:"signals"`
}

// CapsuleRouting describes routing configuration.
type CapsuleRouting struct {
	Weight          RouteWeight `toml:"weight"`
	FallbackToCloud *bool       `toml:"fallback_to_cloud"`
	CloudCapsule    *string     `toml:"cloud_capsule"`
}

// ModelConfig describes inference model configuration.
type ModelConfig struct {
	Source       *string       `toml:"source"`
	Quantization *Quantization `toml:"quantization"`
}

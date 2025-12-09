// Package capsule provides types and utilities for Capsule manifest parsing.
// Implements the "Everything is a Capsule" paradigm for Gumball v0.3.0.
package capsule

import "time"

// CapsuleType represents the fundamental nature of a Capsule
type CapsuleType string

const (
TypeInference CapsuleType = "inference"
TypeTool      CapsuleType = "tool"
TypeApp       CapsuleType = "app"
)

// RuntimeType represents how the Capsule is executed
type RuntimeType string

const (
RuntimePythonUv RuntimeType = "python-uv"
RuntimeDocker   RuntimeType = "docker"
RuntimeNative   RuntimeType = "native"
)

// RouteWeight determines local vs cloud routing preference
type RouteWeight string

const (
WeightLight RouteWeight = "light"
WeightHeavy RouteWeight = "heavy"
)

// Quantization represents model quantization format
type Quantization string

const (
QuantFP16 Quantization = "fp16"
QuantBF16 Quantization = "bf16"
Quant8Bit Quantization = "8bit"
Quant4Bit Quantization = "4bit"
)

// Platform represents target platform
type Platform string

const (
PlatformDarwinArm64  Platform = "darwin-arm64"
PlatformDarwinX86_64 Platform = "darwin-x86_64"
PlatformLinuxAmd64   Platform = "linux-amd64"
PlatformLinuxArm64   Platform = "linux-arm64"
)

// CapsuleStatus represents the current state of a Capsule
type CapsuleStatus string

const (
StatusStopped  CapsuleStatus = "stopped"
StatusStarting CapsuleStatus = "starting"
StatusRunning  CapsuleStatus = "running"
StatusError    CapsuleStatus = "error"
)

// CapsuleManifest represents the v1.0 Capsule manifest schema
type CapsuleManifest struct {
	SchemaVersion string         `toml:"schema_version" json:"schema_version"`
	Name          string         `toml:"name" json:"name"`
	Version       string         `toml:"version" json:"version"`
	Type          CapsuleType    `toml:"type" json:"type"`
	Metadata      Metadata       `toml:"metadata" json:"metadata"`
	Capabilities  *Capabilities  `toml:"capabilities,omitempty" json:"capabilities,omitempty"`
	Requirements  Requirements   `toml:"requirements" json:"requirements"`
	Execution     Execution      `toml:"execution" json:"execution"`
	Routing       Routing        `toml:"routing" json:"routing"`
	Model         *ModelConfig   `toml:"model,omitempty" json:"model,omitempty"`
}

// Metadata contains human-readable information about the Capsule
type Metadata struct {
	DisplayName string   `toml:"display_name,omitempty" json:"display_name,omitempty"`
	Description string   `toml:"description,omitempty" json:"description,omitempty"`
	Author      string   `toml:"author,omitempty" json:"author,omitempty"`
	Icon        string   `toml:"icon,omitempty" json:"icon,omitempty"`
	Tags        []string `toml:"tags,omitempty" json:"tags,omitempty"`
}

// Capabilities defines what the Capsule can do (for inference type)
type Capabilities struct {
	Chat           bool   `toml:"chat" json:"chat"`
	FunctionCalling bool  `toml:"function_calling" json:"function_calling"`
	Vision         bool   `toml:"vision" json:"vision"`
	ContextLength  uint32 `toml:"context_length,omitempty" json:"context_length,omitempty"`
}

// Requirements defines system requirements for the Capsule
type Requirements struct {
	Platform        []Platform `toml:"platform,omitempty" json:"platform,omitempty"`
	VRAMMin         string     `toml:"vram_min,omitempty" json:"vram_min,omitempty"`
	VRAMRecommended string     `toml:"vram_recommended,omitempty" json:"vram_recommended,omitempty"`
	Disk            string     `toml:"disk,omitempty" json:"disk,omitempty"`
	Dependencies    []string   `toml:"dependencies,omitempty" json:"dependencies,omitempty"`
}

// SignalConfig defines shutdown signal configuration
type SignalConfig struct {
	Stop string `toml:"stop,omitempty" json:"stop,omitempty"`
	Kill string `toml:"kill,omitempty" json:"kill,omitempty"`
}

// Execution defines how the Capsule is executed
type Execution struct {
	Runtime        RuntimeType       `toml:"runtime" json:"runtime"`
	Entrypoint     string            `toml:"entrypoint" json:"entrypoint"`
	Port           uint16            `toml:"port,omitempty" json:"port,omitempty"`
	HealthCheck    string            `toml:"health_check,omitempty" json:"health_check,omitempty"`
	StartupTimeout uint32            `toml:"startup_timeout,omitempty" json:"startup_timeout,omitempty"`
	Env            map[string]string `toml:"env,omitempty" json:"env,omitempty"`
	Signals        SignalConfig      `toml:"signals,omitempty" json:"signals,omitempty"`
}

// Routing defines local/cloud routing behavior
type Routing struct {
	Weight          RouteWeight `toml:"weight,omitempty" json:"weight,omitempty"`
	FallbackToCloud bool        `toml:"fallback_to_cloud,omitempty" json:"fallback_to_cloud,omitempty"`
	CloudCapsule    string      `toml:"cloud_capsule,omitempty" json:"cloud_capsule,omitempty"`
}

// ModelConfig defines model-specific configuration (for inference type)
type ModelConfig struct {
	Source       string       `toml:"source,omitempty" json:"source,omitempty"`
	Quantization Quantization `toml:"quantization,omitempty" json:"quantization,omitempty"`
}

// InstalledCapsule represents a Capsule installed on the local system
type InstalledCapsule struct {
	ID          string          `json:"id"`
	Version     string          `json:"version"`
	Manifest    *CapsuleManifest `json:"manifest"`
	InstallPath string          `json:"install_path"`
	Status      CapsuleStatus   `json:"status"`
	InstalledAt time.Time       `json:"installed_at"`
	LastUsedAt  *time.Time      `json:"last_used_at,omitempty"`
}

// ProcessInfo contains runtime information about a running Capsule
type ProcessInfo struct {
	CapsuleID    string    `json:"capsule_id"`
	PID          int       `json:"pid"`
	Port         int       `json:"port"`
	StartedAt    time.Time `json:"started_at"`
	HealthStatus string    `json:"health_status"`
}

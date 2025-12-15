package capsule

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"

	"github.com/BurntSushi/toml"
)

// Parsing errors
var (
	ErrInvalidSchemaVersion = errors.New("invalid schema_version, expected '1.0'")
	ErrInvalidName          = errors.New("invalid name, must be kebab-case")
	ErrInvalidVersion       = errors.New("invalid version, must be semver")
	ErrMissingCapabilities  = errors.New("inference Capsule must have capabilities defined")
	ErrMissingModelConfig   = errors.New("inference Capsule must have model config defined")
	ErrInvalidPort          = errors.New("invalid port number")
	ErrInvalidRuntime       = errors.New("invalid runtime")
)

// kebabCaseRegex matches valid kebab-case identifiers
var kebabCaseRegex = regexp.MustCompile(`^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$`)

// semverRegex matches semver versions
var semverRegex = regexp.MustCompile(`^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$`)

// ParseTOML parses a TOML string into a CapsuleManifest
func ParseTOML(content string) (*CapsuleManifest, error) {
	var manifest CapsuleManifest
	if _, err := toml.Decode(content, &manifest); err != nil {
		return nil, fmt.Errorf("TOML parse error: %w", err)
	}
	return &manifest, nil
}

// ParseJSON parses a JSON string into a CapsuleManifest
func ParseJSON(content string) (*CapsuleManifest, error) {
	var manifest CapsuleManifest
	if err := json.Unmarshal([]byte(content), &manifest); err != nil {
		return nil, fmt.Errorf("JSON parse error: %w", err)
	}
	return &manifest, nil
}

// LoadFromFile loads a CapsuleManifest from a file, auto-detecting format
func LoadFromFile(path string) (*CapsuleManifest, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("failed to read file: %w", err)
	}

	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".toml":
		return ParseTOML(string(content))
	case ".json":
		return ParseJSON(string(content))
	default:
		// Try TOML first, then JSON
		if m, err := ParseTOML(string(content)); err == nil {
			return m, nil
		}
		return ParseJSON(string(content))
	}
}

// ToJSON serializes the manifest to JSON
func (m *CapsuleManifest) ToJSON() (string, error) {
	data, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return "", fmt.Errorf("JSON serialize error: %w", err)
	}
	return string(data), nil
}

// Validate checks if the manifest is valid
func (m *CapsuleManifest) Validate() error {
	var errs []error

	// Schema version must be "1.0"
	if m.SchemaVersion != "1.0" {
		errs = append(errs, ErrInvalidSchemaVersion)
	}

	// Name must be kebab-case
	if !kebabCaseRegex.MatchString(m.Name) {
		errs = append(errs, ErrInvalidName)
	}

	// Version must be semver
	if !semverRegex.MatchString(m.Version) {
		errs = append(errs, ErrInvalidVersion)
	}

	// Inference type checks
	if m.Type == TypeInference {
		if m.Capabilities == nil {
			errs = append(errs, ErrMissingCapabilities)
		}
		if m.Model == nil {
			errs = append(errs, ErrMissingModelConfig)
		}
	}

	// Port validation
	// Note: port is uint16 in the schema, so it cannot exceed 65535.
	// We treat 0 as "unset" (allowed) for backward compatibility.

	// Storage validation (minimal): docker-only, absolute mount paths, unique volume names.
	if m.Storage != nil && len(m.Storage.Volumes) > 0 {
		if m.Execution.Runtime != RuntimeDocker {
			errs = append(errs, errors.New("storage volumes are only supported for runtime=docker"))
		} else {
			seen := map[string]struct{}{}
			for _, vol := range m.Storage.Volumes {
				name := strings.TrimSpace(vol.Name)
				if name == "" {
					errs = append(errs, errors.New("storage.volumes[].name is required"))
					continue
				}
				if _, ok := seen[name]; ok {
					errs = append(errs, fmt.Errorf("duplicate storage volume name: %q", name))
					continue
				}
				seen[name] = struct{}{}
				mountPath := strings.TrimSpace(vol.MountPath)
				if mountPath == "" {
					errs = append(errs, fmt.Errorf("storage.volumes[%s].mount_path is required", name))
					continue
				}
				clean := filepath.Clean(mountPath)
				if !strings.HasPrefix(clean, "/") || strings.Contains(clean, "..") {
					errs = append(errs, fmt.Errorf("invalid storage.volumes[%s].mount_path: %q", name, mountPath))
					continue
				}
			}
		}
	}

	if len(errs) > 0 {
		return fmt.Errorf("validation errors: %v", errs)
	}
	return nil
}

// SupportsCurrentPlatform checks if this Capsule can run on the current platform
func (m *CapsuleManifest) SupportsCurrentPlatform() bool {
	if len(m.Requirements.Platform) == 0 {
		return true // No platform restrictions
	}

	var currentPlatform Platform
	switch runtime.GOOS {
	case "darwin":
		if runtime.GOARCH == "arm64" {
			currentPlatform = PlatformDarwinArm64
		} else {
			currentPlatform = PlatformDarwinX86_64
		}
	case "linux":
		if runtime.GOARCH == "arm64" {
			currentPlatform = PlatformLinuxArm64
		} else {
			currentPlatform = PlatformLinuxAmd64
		}
	default:
		return false
	}

	for _, p := range m.Requirements.Platform {
		if p == currentPlatform {
			return true
		}
	}
	return false
}

// DisplayName returns the effective display name
func (m *CapsuleManifest) DisplayName() string {
	if m.Metadata.DisplayName != "" {
		return m.Metadata.DisplayName
	}
	return m.Name
}

// IsInference returns true if this is an inference Capsule
func (m *CapsuleManifest) IsInference() bool {
	return m.Type == TypeInference
}

// CanFallbackToCloud returns true if cloud fallback is configured
func (m *CapsuleManifest) CanFallbackToCloud() bool {
	return m.Routing.FallbackToCloud && m.Routing.CloudCapsule != ""
}

// VRAMMinBytes parses the vram_min requirement into bytes
func (m *CapsuleManifest) VRAMMinBytes() (int64, error) {
	if m.Requirements.VRAMMin == "" {
		return 0, nil
	}
	return parseMemoryString(m.Requirements.VRAMMin)
}

// VRAMRecommendedBytes parses the vram_recommended requirement into bytes
func (m *CapsuleManifest) VRAMRecommendedBytes() (int64, error) {
	if m.Requirements.VRAMRecommended == "" {
		return 0, nil
	}
	return parseMemoryString(m.Requirements.VRAMRecommended)
}

// DiskBytes parses the disk requirement into bytes
func (m *CapsuleManifest) DiskBytes() (int64, error) {
	if m.Requirements.Disk == "" {
		return 0, nil
	}
	return parseMemoryString(m.Requirements.Disk)
}

// parseMemoryString parses a memory string like "6GB" or "512MB" into bytes
func parseMemoryString(s string) (int64, error) {
	s = strings.TrimSpace(strings.ToUpper(s))

	var multiplier int64 = 1
	var numStr string

	switch {
	case strings.HasSuffix(s, "TB"):
		multiplier = 1024 * 1024 * 1024 * 1024
		numStr = strings.TrimSuffix(s, "TB")
	case strings.HasSuffix(s, "GB"):
		multiplier = 1024 * 1024 * 1024
		numStr = strings.TrimSuffix(s, "GB")
	case strings.HasSuffix(s, "MB"):
		multiplier = 1024 * 1024
		numStr = strings.TrimSuffix(s, "MB")
	case strings.HasSuffix(s, "KB"):
		multiplier = 1024
		numStr = strings.TrimSuffix(s, "KB")
	case strings.HasSuffix(s, "B"):
		numStr = strings.TrimSuffix(s, "B")
	default:
		return 0, fmt.Errorf("invalid memory string format: %s", s)
	}

	numStr = strings.TrimSpace(numStr)

	// Try parsing as float first (for values like "6.5GB")
	if f, err := strconv.ParseFloat(numStr, 64); err == nil {
		return int64(f * float64(multiplier)), nil
	}

	// Fallback to int
	n, err := strconv.ParseInt(numStr, 10, 64)
	if err != nil {
		return 0, fmt.Errorf("invalid number in memory string: %s", s)
	}

	return n * multiplier, nil
}

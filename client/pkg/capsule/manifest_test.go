package capsule

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

const validTOML = `
schema_version = "1.0"
name = "mlx-qwen3-8b"
version = "1.0.0"
type = "inference"

[metadata]
display_name = "Qwen3 8B (MLX)"
description = "Local inference on Apple Silicon"
author = "gumball-official"
tags = ["llm", "mlx"]

[capabilities]
chat = true
function_calling = true
vision = false
context_length = 128000

[requirements]
platform = ["darwin-arm64"]
vram_min = "6GB"
vram_recommended = "8GB"
disk = "5GB"

[execution]
runtime = "python-uv"
entrypoint = "server.py"
port = 8081
health_check = "/health"
startup_timeout = 120

[execution.env]
GUMBALL_MODEL = "qwen3-8b"

[routing]
weight = "light"
fallback_to_cloud = true
cloud_capsule = "vllm-qwen3-8b"

[model]
source = "hf:org/model"
quantization = "4bit"
`

func TestParseTOML(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	assert.Equal(t, "mlx-qwen3-8b", manifest.Name)
	assert.Equal(t, "1.0.0", manifest.Version)
	assert.Equal(t, TypeInference, manifest.Type)
	assert.Equal(t, uint16(8081), manifest.Execution.Port)
	assert.Equal(t, RuntimePythonUv, manifest.Execution.Runtime)
	assert.True(t, manifest.Capabilities.Chat)
	assert.Equal(t, WeightLight, manifest.Routing.Weight)
}

func TestValidate_Valid(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	err = manifest.Validate()
	assert.NoError(t, err)
}

func TestValidate_InvalidSchemaVersion(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	manifest.SchemaVersion = "2.0"
	err = manifest.Validate()
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "schema_version")
}

func TestValidate_InvalidName(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	manifest.Name = "Invalid Name!"
	err = manifest.Validate()
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "kebab-case")
}

func TestToJSON_Roundtrip(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	jsonStr, err := manifest.ToJSON()
	require.NoError(t, err)

	manifest2, err := ParseJSON(jsonStr)
	require.NoError(t, err)

	assert.Equal(t, manifest.Name, manifest2.Name)
	assert.Equal(t, manifest.Version, manifest2.Version)
}

func TestDisplayName(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	assert.Equal(t, "Qwen3 8B (MLX)", manifest.DisplayName())
}

func TestCanFallbackToCloud(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	assert.True(t, manifest.CanFallbackToCloud())
}

func TestVRAMMinBytes(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	bytes, err := manifest.VRAMMinBytes()
	require.NoError(t, err)
	assert.Equal(t, int64(6*1024*1024*1024), bytes) // 6GB
}

func TestParseMemoryString(t *testing.T) {
	tests := []struct {
		input    string
		expected int64
	}{
		{"1GB", 1 * 1024 * 1024 * 1024},
		{"512MB", 512 * 1024 * 1024},
		{"6GB", 6 * 1024 * 1024 * 1024},
		{"1TB", 1 * 1024 * 1024 * 1024 * 1024},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			result, err := parseMemoryString(tt.input)
			require.NoError(t, err)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestIsInference(t *testing.T) {
	manifest, err := ParseTOML(validTOML)
	require.NoError(t, err)

	assert.True(t, manifest.IsInference())
}

func TestKebabCaseValidation(t *testing.T) {
	tests := []struct {
		name  string
		valid bool
	}{
		{"valid-name", true},
		{"name123", true},
		{"a", true},
		{"a1", true},
		{"Invalid", false},
		{"-invalid", false},
		{"invalid-", false},
		{"", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.name == "" {
				assert.False(t, kebabCaseRegex.MatchString(tt.name))
			} else {
				assert.Equal(t, tt.valid, kebabCaseRegex.MatchString(tt.name))
			}
		})
	}
}

func TestSemverValidation(t *testing.T) {
	tests := []struct {
		version string
		valid   bool
	}{
		{"1.0.0", true},
		{"0.1.0", true},
		{"1.0.0-alpha", true},
		{"1.0", false},
		{"v1.0.0", false},
	}

	for _, tt := range tests {
		t.Run(tt.version, func(t *testing.T) {
			assert.Equal(t, tt.valid, semverRegex.MatchString(tt.version))
		})
	}
}

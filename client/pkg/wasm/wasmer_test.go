//go:build cgo
// +build cgo

package wasm

import (
	"testing"
)

func TestWasmerHost_ValidateManifest_Valid(t *testing.T) {
	host, err := NewWasmerHost("adep_logic.wasm")
	if err != nil {
		t.Fatalf("Failed to create WasmerHost: %v", err)
	}
	defer host.Close()

	validJSON := []byte(`{
		"name": "test-workload",
		"compute": {
			"image": "nginx:latest"
		}
	}`)

	isValid, err := host.ValidateManifest(validJSON)
	if err != nil {
		t.Fatalf("ValidateManifest failed: %v", err)
	}

	if !isValid {
		t.Error("Expected valid manifest, got invalid")
	}
}

func TestWasmerHost_ValidateManifest_MissingName(t *testing.T) {
	host, err := NewWasmerHost("adep_logic.wasm")
	if err != nil {
		t.Fatalf("Failed to create WasmerHost: %v", err)
	}
	defer host.Close()

	invalidJSON := []byte(`{
		"name": "",
		"compute": {
			"image": "nginx:latest"
		}
	}`)

	isValid, err := host.ValidateManifest(invalidJSON)
	if err != nil {
		t.Fatalf("ValidateManifest failed: %v", err)
	}

	if isValid {
		t.Error("Expected invalid manifest (empty name), got valid")
	}
}

func TestWasmerHost_ValidateManifest_MissingImage(t *testing.T) {
	host, err := NewWasmerHost("adep_logic.wasm")
	if err != nil {
		t.Fatalf("Failed to create WasmerHost: %v", err)
	}
	defer host.Close()

	invalidJSON := []byte(`{
		"name": "test-workload",
		"compute": {
			"image": ""
		}
	}`)

	isValid, err := host.ValidateManifest(invalidJSON)
	if err != nil {
		t.Fatalf("ValidateManifest failed: %v", err)
	}

	if isValid {
		t.Error("Expected invalid manifest (empty image), got valid")
	}
}

func TestWasmerHost_ValidateManifest_InvalidJSON(t *testing.T) {
	host, err := NewWasmerHost("adep_logic.wasm")
	if err != nil {
		t.Fatalf("Failed to create WasmerHost: %v", err)
	}
	defer host.Close()

	invalidJSON := []byte(`{not valid json}`)

	isValid, err := host.ValidateManifest(invalidJSON)
	if err != nil {
		t.Fatalf("ValidateManifest failed: %v", err)
	}

	if isValid {
		t.Error("Expected invalid manifest (malformed JSON), got valid")
	}
}

func TestWasmerHost_MultipleValidations(t *testing.T) {
	host, err := NewWasmerHost("adep_logic.wasm")
	if err != nil {
		t.Fatalf("Failed to create WasmerHost: %v", err)
	}
	defer host.Close()

	// First validation (valid)
	validJSON := []byte(`{"name": "test1", "compute": {"image": "nginx"}}`)
	isValid, err := host.ValidateManifest(validJSON)
	if err != nil {
		t.Fatalf("First validation failed: %v", err)
	}
	if !isValid {
		t.Error("First validation: expected valid, got invalid")
	}

	// Second validation (invalid)
	invalidJSON := []byte(`{"name": "", "compute": {"image": "nginx"}}`)
	isValid, err = host.ValidateManifest(invalidJSON)
	if err != nil {
		t.Fatalf("Second validation failed: %v", err)
	}
	if isValid {
		t.Error("Second validation: expected invalid, got valid")
	}

	// Third validation (valid again)
	validJSON2 := []byte(`{"name": "test2", "compute": {"image": "nginx"}}`)
	isValid, err = host.ValidateManifest(validJSON2)
	if err != nil {
		t.Fatalf("Third validation failed: %v", err)
	}
	if !isValid {
		t.Error("Third validation: expected valid, got invalid")
	}
}

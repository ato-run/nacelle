//go:build !cgo
// +build !cgo

package wasm

import (
	"fmt"
)

// WasmerHost provides Wasm-based adep.json validation using Wasmer runtime
// This is a stub implementation for builds without CGO support
type WasmerHost struct{}

// NewWasmerHost creates a new Wasmer host (stub for non-CGO builds)
func NewWasmerHost() (*WasmerHost, error) {
	return nil, fmt.Errorf("wasmer support requires CGO (build with CGO_ENABLED=1)")
}

// ValidateManifest validates an adep.json manifest (stub for non-CGO builds)
func (h *WasmerHost) ValidateManifest(manifestJSON []byte) (bool, error) {
	return false, fmt.Errorf("wasmer support requires CGO (build with CGO_ENABLED=1)")
}

// Close releases resources (stub for non-CGO builds)
func (h *WasmerHost) Close() error {
	return nil
}

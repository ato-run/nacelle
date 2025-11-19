//go:build cgo
// +build cgo

package wasm

import (
	_ "embed"
	"fmt"
	"sync"

	"github.com/wasmerio/wasmer-go/wasmer"
)

//go:embed adep_logic.wasm
var adepLogicWasm []byte

// WasmerHost provides Wasm-based adep.json validation using Wasmer runtime
type WasmerHost struct {
	instance *wasmer.Instance
	mu       sync.Mutex
}

// NewWasmerHost creates a new Wasmer host with the embedded adep_logic.wasm module
func NewWasmerHost() (*WasmerHost, error) {
	// Create a new Wasmer engine
	engine := wasmer.NewEngine()

	// Create a new store
	store := wasmer.NewStore(engine)

	// Compile the Wasm module
	module, err := wasmer.NewModule(store, adepLogicWasm)
	if err != nil {
		return nil, fmt.Errorf("failed to compile wasm module: %w", err)
	}

	// Create an empty import object (adep_logic.wasm has no imports)
	importObject := wasmer.NewImportObject()

	// Instantiate the module
	instance, err := wasmer.NewInstance(module, importObject)
	if err != nil {
		return nil, fmt.Errorf("failed to instantiate wasm module: %w", err)
	}

	return &WasmerHost{
		instance: instance,
	}, nil
}

// ValidateManifest validates an adep.json manifest using the Wasm module
// Returns true if the manifest is valid, false otherwise
func (h *WasmerHost) ValidateManifest(manifestJSON []byte) (bool, error) {
	h.mu.Lock()
	defer h.mu.Unlock()

	// Get the validate_manifest function
	validateFunc, err := h.instance.Exports.GetFunction("validate_manifest")
	if err != nil {
		return false, fmt.Errorf("failed to get validate_manifest function: %w", err)
	}

	// Get the alloc function
	allocFunc, err := h.instance.Exports.GetFunction("alloc")
	if err != nil {
		return false, fmt.Errorf("failed to get alloc function: %w", err)
	}

	// Allocate memory in Wasm for the JSON string
	// Get the memory instance
	memory, err := h.instance.Exports.GetMemory("memory")
	if err != nil {
		return false, fmt.Errorf("failed to get wasm memory: %w", err)
	}

	jsonLen := len(manifestJSON)

	// Call alloc(len) to get a pointer
	ptrVal, err := allocFunc(jsonLen)
	if err != nil {
		return false, fmt.Errorf("failed to allocate memory in wasm: %w", err)
	}
	ptr := ptrVal.(int32)

	// Get the memory data
	memoryData := memory.Data()

	// Check bounds (though alloc should have ensured space, we check against the view)
	if int(ptr)+jsonLen > len(memoryData) {
		return false, fmt.Errorf("allocated memory out of bounds")
	}

	// Copy JSON to Wasm memory at the allocated pointer
	copy(memoryData[ptr:], manifestJSON)

	// Call validate_manifest(json_ptr, json_len)
	result, err := validateFunc(ptr, jsonLen)
	if err != nil {
		return false, fmt.Errorf("failed to call validate_manifest: %w", err)
	}

	// Result is 1 for valid, 0 for invalid
	isValid := result.(int32) == 1
	return isValid, nil
}

// Close releases resources held by the Wasmer instance
func (h *WasmerHost) Close() error {
	h.mu.Lock()
	defer h.mu.Unlock()

	if h.instance != nil {
		h.instance.Close()
		h.instance = nil
	}
	return nil
}

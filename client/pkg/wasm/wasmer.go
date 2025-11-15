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
	
	// Allocate memory in Wasm for the JSON string
	// Get the memory instance
	memory, err := h.instance.Exports.GetMemory("memory")
	if err != nil {
		return false, fmt.Errorf("failed to get wasm memory: %w", err)
	}
	
	// Get the memory data
	memoryData := memory.Data()
	
	// Find a place to write the JSON (we'll use offset 0 for simplicity)
	// In production, you might want a proper allocator
	jsonLen := len(manifestJSON)
	if jsonLen > len(memoryData) {
		return false, fmt.Errorf("manifest JSON too large: %d bytes (max: %d)", jsonLen, len(memoryData))
	}
	
	// Copy JSON to Wasm memory
	copy(memoryData, manifestJSON)
	
	// Call validate_manifest(json_ptr, json_len)
	// json_ptr = 0 (we wrote at offset 0)
	// json_len = length of JSON
	result, err := validateFunc(0, jsonLen)
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

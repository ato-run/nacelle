// Package capsule provides Capsule lifecycle management.
package capsule

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"

	"github.com/onescluster/coordinator/pkg/hardware"
	"github.com/onescluster/coordinator/pkg/store"
)

// Manager handles Capsule lifecycle operations
type Manager struct {
	store           store.Store
	monitor         hardware.HardwareMonitor
	capsules        map[string]*RunningCapsule
	mu              sync.RWMutex
	dataDir         string
	activeInference string // Currently active inference capsule name
}

// RunningCapsule represents a running capsule process
type RunningCapsule struct {
	Manifest  *CapsuleManifest
	Process   *os.Process
	Cmd       *exec.Cmd
	StartedAt time.Time
	Port      int
}

// NewManager creates a new Capsule Manager
func NewManager(st store.Store, mon hardware.HardwareMonitor, dataDir string) *Manager {
	return &Manager{
		store:    st,
		monitor:  mon,
		capsules: make(map[string]*RunningCapsule),
		dataDir:  dataDir,
	}
}

// Install registers a new capsule from a directory containing capsule.toml
func (m *Manager) Install(ctx context.Context, capsulePath string) (*CapsuleManifest, error) {
	manifestPath := filepath.Join(capsulePath, "capsule.toml")
	manifest, err := LoadFromFile(manifestPath)
	if err != nil {
		return nil, fmt.Errorf("failed to load capsule manifest: %w", err)
	}

	if err := manifest.Validate(); err != nil {
		return nil, fmt.Errorf("invalid capsule manifest: %w", err)
	}

	// Store capsule in database
	capsule := &store.Capsule{
		Name:         manifest.Name,
		Version:      manifest.Version,
		Type:         string(manifest.Type),
		ManifestPath: manifestPath,
		Status:       store.StatusStopped,
		InstalledAt:  time.Now(),
	}

	if err := m.store.Install(ctx, capsule); err != nil {
		return nil, fmt.Errorf("failed to store capsule: %w", err)
	}

	return manifest, nil
}

// Start starts a capsule by name
func (m *Manager) Start(ctx context.Context, name string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	// Check if already running
	if _, running := m.capsules[name]; running {
		return fmt.Errorf("capsule %s is already running", name)
	}

	// Get capsule from store
	capsule, err := m.store.Get(ctx, name)
	if err != nil {
		return fmt.Errorf("capsule not found: %w", err)
	}

	// Load manifest
	manifest, err := LoadFromFile(capsule.ManifestPath)
	if err != nil {
		return fmt.Errorf("failed to load manifest: %w", err)
	}

	// Check VRAM requirements
	if manifest.Requirements.VRAMMin != "" {
		requiredBytes, err := manifest.VRAMMinBytes()
		if err != nil {
			return fmt.Errorf("failed to parse VRAM requirement: %w", err)
		}
		result, err := m.monitor.CanRunCapsule(requiredBytes)
		if err != nil {
			return fmt.Errorf("failed to check system resources: %w", err)
		}
		if !result.CanRun {
			return fmt.Errorf("insufficient resources: %s", result.Reason)
		}
	}

	// Start the capsule based on runtime type
	runningCapsule, err := m.startRuntime(ctx, manifest, filepath.Dir(capsule.ManifestPath))
	if err != nil {
		return fmt.Errorf("failed to start capsule: %w", err)
	}

	// Record start in store
	if err := m.store.RecordStart(ctx, name, runningCapsule.Process.Pid); err != nil {
		// Kill the process if we can't record it
runningCapsule.Process.Kill()
return fmt.Errorf("failed to record start: %w", err)
}

// Update store status
if err := m.store.UpdateStatus(ctx, name, store.StatusRunning); err != nil {
runningCapsule.Process.Kill()
return fmt.Errorf("failed to update status: %w", err)
}

m.capsules[name] = runningCapsule
return nil
}

// Stop stops a running capsule
func (m *Manager) Stop(ctx context.Context, name string) error {
m.mu.Lock()
defer m.mu.Unlock()

running, exists := m.capsules[name]
if !exists {
return fmt.Errorf("capsule %s is not running", name)
}

// Send interrupt signal
if err := running.Cmd.Process.Signal(os.Interrupt); err != nil {
// If interrupt fails, try kill
running.Cmd.Process.Kill()
}

// Wait for process to exit (with timeout)
done := make(chan error, 1)
go func() {
done <- running.Cmd.Wait()
}()

select {
case <-done:
// Process exited
case <-time.After(10 * time.Second):
// Force kill
running.Cmd.Process.Kill()
}

// Record stop
if err := m.store.RecordStop(ctx, name, running.Process.Pid); err != nil {
return fmt.Errorf("failed to record stop: %w", err)
}

// Update status
if err := m.store.UpdateStatus(ctx, name, store.StatusStopped); err != nil {
return fmt.Errorf("failed to update status: %w", err)
}

delete(m.capsules, name)
return nil
}

// List returns all installed capsules
func (m *Manager) List(ctx context.Context) ([]*store.Capsule, error) {
return m.store.List(ctx)
}

// Status returns the status of a capsule
func (m *Manager) Status(ctx context.Context, name string) (*store.Capsule, bool, error) {
capsule, err := m.store.Get(ctx, name)
if err != nil {
return nil, false, err
}

m.mu.RLock()
_, running := m.capsules[name]
m.mu.RUnlock()

return capsule, running, nil
}

// Uninstall removes a capsule
func (m *Manager) Uninstall(ctx context.Context, name string) error {
	// Stop if running
	m.mu.RLock()
	_, running := m.capsules[name]
	m.mu.RUnlock()

	if running {
		if err := m.Stop(ctx, name); err != nil {
			return fmt.Errorf("failed to stop capsule before uninstall: %w", err)
		}
	}

	// Clear active inference if this was it
	m.mu.Lock()
	if m.activeInference == name {
		m.activeInference = ""
	}
	m.mu.Unlock()

	return m.store.Delete(ctx, name)
}

// =============================================================================
// Multiple Inference Capsule Management (v0.3.0)
// =============================================================================

// SetActiveInference sets the active inference capsule
// Only capsules with type "inference" can be set as active
func (m *Manager) SetActiveInference(ctx context.Context, name string) error {
	// Get capsule from store
	capsule, err := m.store.Get(ctx, name)
	if err != nil {
		return fmt.Errorf("capsule not found: %w", err)
	}

	// Load manifest to check type
	manifest, err := LoadFromFile(capsule.ManifestPath)
	if err != nil {
		return fmt.Errorf("failed to load manifest: %w", err)
	}

	// Verify it's an inference capsule
	if manifest.Type != TypeInference {
		return fmt.Errorf("capsule %s is not an inference capsule (type: %s)", name, manifest.Type)
	}

	m.mu.Lock()
	m.activeInference = name
	m.mu.Unlock()

	return nil
}

// GetActiveInference returns the name of the currently active inference capsule
// Returns empty string if no inference capsule is active
func (m *Manager) GetActiveInference(ctx context.Context) (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.activeInference, nil
}

// SwitchInference switches to a different inference capsule
// This will set the new capsule as active (does not start/stop automatically)
func (m *Manager) SwitchInference(ctx context.Context, name string) error {
	// Validate the new capsule exists and is an inference type
	if err := m.SetActiveInference(ctx, name); err != nil {
		return fmt.Errorf("failed to switch inference: %w", err)
	}

	return nil
}

// ListInferenceCapsules returns all installed inference-type capsules
func (m *Manager) ListInferenceCapsules(ctx context.Context) ([]*store.Capsule, error) {
	all, err := m.store.List(ctx)
	if err != nil {
		return nil, err
	}

	var inference []*store.Capsule
	for _, c := range all {
		if c.Type == string(TypeInference) {
			inference = append(inference, c)
		}
	}

	return inference, nil
}

// GetActiveInferenceManifest returns the manifest of the active inference capsule
// Returns nil if no inference capsule is active
func (m *Manager) GetActiveInferenceManifest(ctx context.Context) (*CapsuleManifest, error) {
	m.mu.RLock()
	activeName := m.activeInference
	m.mu.RUnlock()

	if activeName == "" {
		return nil, nil
	}

	capsule, err := m.store.Get(ctx, activeName)
	if err != nil {
		return nil, fmt.Errorf("active inference capsule not found: %w", err)
	}

	return LoadFromFile(capsule.ManifestPath)
}

// IsInferenceActive checks if a specific inference capsule is the active one
func (m *Manager) IsInferenceActive(name string) bool {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.activeInference == name
}

// startRuntime starts the appropriate runtime for the capsule
func (m *Manager) startRuntime(ctx context.Context, manifest *CapsuleManifest, workDir string) (*RunningCapsule, error) {
switch manifest.Execution.Runtime {
case RuntimePythonUv:
return m.startPythonUv(ctx, manifest, workDir)
case RuntimeDocker:
return nil, fmt.Errorf("docker runtime not yet implemented")
case RuntimeNative:
return nil, fmt.Errorf("native runtime not yet implemented")
default:
return nil, fmt.Errorf("unknown runtime: %s", manifest.Execution.Runtime)
}
}

// startPythonUv starts a python-uv based capsule
func (m *Manager) startPythonUv(ctx context.Context, manifest *CapsuleManifest, workDir string) (*RunningCapsule, error) {
// Find port - start from 8080, increment if in use
port := 8080
// TODO: Check if port is available

entrypoint := manifest.Execution.Entrypoint
if entrypoint == "" {
entrypoint = "main:app"
}

// Split entrypoint into module:app
// e.g., "server:app" -> "server", "app"
var module, app string
if n, _ := fmt.Sscanf(entrypoint, "%s:%s", &module, &app); n == 2 {
// Use uvicorn
} else {
module = entrypoint
app = "app"
}

// Use uv run to execute uvicorn
cmd := exec.CommandContext(ctx, "uv", "run", "uvicorn",
fmt.Sprintf("%s:%s", module, app),
"--host", "127.0.0.1",
"--port", fmt.Sprintf("%d", port),
)
cmd.Dir = workDir
cmd.Env = append(os.Environ(), "PYTHONUNBUFFERED=1")

// Set environment from manifest
for k, v := range manifest.Execution.Env {
cmd.Env = append(cmd.Env, fmt.Sprintf("%s=%s", k, v))
}

// Start the process
if err := cmd.Start(); err != nil {
return nil, fmt.Errorf("failed to start process: %w", err)
}

return &RunningCapsule{
Manifest:  manifest,
Process:   cmd.Process,
Cmd:       cmd,
StartedAt: time.Now(),
Port:      port,
}, nil
}

// Close stops all running capsules
func (m *Manager) Close() error {
ctx := context.Background()
m.mu.Lock()
names := make([]string, 0, len(m.capsules))
for name := range m.capsules {
names = append(names, name)
}
m.mu.Unlock()

for _, name := range names {
m.Stop(ctx, name)
}
return nil
}

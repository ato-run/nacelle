package capsule

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/hardware"
	"github.com/onescluster/coordinator/pkg/store"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockMonitor implements hardware.HardwareMonitor for testing
type mockMonitor struct {
	resources *hardware.SystemResources
	canRun    bool
	reason    string
}

func (m *mockMonitor) GetCurrentResources() (*hardware.SystemResources, error) {
	return m.resources, nil
}

func (m *mockMonitor) GetSystemSummary() (string, error) {
	return "mock summary", nil
}

func (m *mockMonitor) CanRunCapsule(requiredVRAM int64) (*hardware.ResourceCheckResult, error) {
	return &hardware.ResourceCheckResult{
		CanRun:      m.canRun,
		Reason:      m.reason,
		VRAMWarning: false,
		RAMWarning:  false,
	}, nil
}

func (m *mockMonitor) Watch(ctx context.Context, interval time.Duration, callback func(*hardware.SystemResources)) {
	// no-op for tests
}

func TestManager_Install(t *testing.T) {
	tmpDir := t.TempDir()
	
	// Create SQLite store
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()
	
	// Create mock monitor
	mon := &mockMonitor{
		resources: &hardware.SystemResources{
			TotalVRAM:     16 * 1024 * 1024 * 1024,
			AvailableVRAM: 10 * 1024 * 1024 * 1024,
		},
		canRun: true,
	}
	
	// Create manager
	mgr := NewManager(st, mon, tmpDir)
	
	// Create a test capsule directory
	capsuleDir := filepath.Join(tmpDir, "test-capsule")
	require.NoError(t, os.MkdirAll(capsuleDir, 0755))
	
	manifestContent := `
schema_version = "1.0"
name = "test-capsule"
version = "1.0.0"
type = "inference"

[capabilities]
chat = true

[requirements]
platform = ["darwin-arm64"]
vram_min = "4GB"

[execution]
runtime = "python-uv"
entrypoint = "main:app"

[routing]
weight = "light"

[model]
source = "test-model"
`
	require.NoError(t, os.WriteFile(
filepath.Join(capsuleDir, "capsule.toml"),
[]byte(manifestContent),
0644,
))
	
	// Test install
	ctx := context.Background()
	manifest, err := mgr.Install(ctx, capsuleDir)
	require.NoError(t, err)
	
	assert.Equal(t, "test-capsule", manifest.Name)
	assert.Equal(t, "1.0.0", manifest.Version)
	assert.Equal(t, CapsuleType("inference"), manifest.Type)
	
	// Verify in store
	capsule, err := st.Get(ctx, "test-capsule")
	require.NoError(t, err)
	assert.Equal(t, "test-capsule", capsule.Name)
	assert.Equal(t, store.StatusStopped, capsule.Status)
}

func TestManager_InstallInvalidManifest(t *testing.T) {
	tmpDir := t.TempDir()
	
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()
	
	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)
	
	// Create capsule with invalid manifest (missing required fields)
	capsuleDir := filepath.Join(tmpDir, "invalid-capsule")
	require.NoError(t, os.MkdirAll(capsuleDir, 0755))
	
	invalidManifest := `
schema_version = "1.0"
# Missing name, version, type
`
	require.NoError(t, os.WriteFile(
filepath.Join(capsuleDir, "capsule.toml"),
[]byte(invalidManifest),
0644,
))
	
	ctx := context.Background()
	_, err = mgr.Install(ctx, capsuleDir)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "invalid capsule manifest")
}

func TestManager_List(t *testing.T) {
	tmpDir := t.TempDir()
	
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()
	
	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)
	
	// Install multiple capsules
	for i, name := range []string{"capsule-a", "capsule-b"} {
		capsuleDir := filepath.Join(tmpDir, name)
		require.NoError(t, os.MkdirAll(capsuleDir, 0755))
		
		manifest := `
schema_version = "1.0"
name = "` + name + `"
version = "1.0.` + string(rune('0'+i)) + `"
type = "tool"

[execution]
runtime = "python-uv"
entrypoint = "main:app"

[routing]
weight = "light"
`
		require.NoError(t, os.WriteFile(
filepath.Join(capsuleDir, "capsule.toml"),
[]byte(manifest),
0644,
))
		
		_, err := mgr.Install(context.Background(), capsuleDir)
		require.NoError(t, err)
	}
	
	// List
	capsules, err := mgr.List(context.Background())
	require.NoError(t, err)
	assert.Len(t, capsules, 2)
}

func TestManager_Status(t *testing.T) {
	tmpDir := t.TempDir()
	
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()
	
	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)
	
	// Create and install a capsule
	capsuleDir := filepath.Join(tmpDir, "status-test")
	require.NoError(t, os.MkdirAll(capsuleDir, 0755))
	
	manifest := `
schema_version = "1.0"
name = "status-test"
version = "1.0.0"
type = "tool"

[execution]
runtime = "python-uv"
entrypoint = "main:app"

[routing]
weight = "light"
`
	require.NoError(t, os.WriteFile(
filepath.Join(capsuleDir, "capsule.toml"),
[]byte(manifest),
0644,
))
	
	_, err = mgr.Install(context.Background(), capsuleDir)
	require.NoError(t, err)
	
	// Check status
	capsule, running, err := mgr.Status(context.Background(), "status-test")
	require.NoError(t, err)
	assert.Equal(t, "status-test", capsule.Name)
	assert.False(t, running, "Should not be running initially")
}

func TestManager_Uninstall(t *testing.T) {
	tmpDir := t.TempDir()
	
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()
	
	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)
	
	// Create and install a capsule
	capsuleDir := filepath.Join(tmpDir, "uninstall-test")
	require.NoError(t, os.MkdirAll(capsuleDir, 0755))
	
	manifest := `
schema_version = "1.0"
name = "uninstall-test"
version = "1.0.0"
type = "tool"

[execution]
runtime = "python-uv"
entrypoint = "main:app"

[routing]
weight = "light"
`
	require.NoError(t, os.WriteFile(
filepath.Join(capsuleDir, "capsule.toml"),
[]byte(manifest),
0644,
))
	
	_, err = mgr.Install(context.Background(), capsuleDir)
	require.NoError(t, err)
	
	// Uninstall
	err = mgr.Uninstall(context.Background(), "uninstall-test")
	require.NoError(t, err)
	
	// Verify removed
	_, err = st.Get(context.Background(), "uninstall-test")
	assert.Error(t, err)
}

// =============================================================================
// Multiple Inference Capsule Switching Tests (v0.3.0)
// =============================================================================

func TestManager_SetActiveInference(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()

	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)

	// Create and install inference capsule
	capsuleDir := filepath.Join(tmpDir, "mlx-qwen3")
	require.NoError(t, os.MkdirAll(capsuleDir, 0755))
	require.NoError(t, os.WriteFile(
		filepath.Join(capsuleDir, "capsule.toml"),
		[]byte(`
schema_version = "1.0"
name = "mlx-qwen3"
version = "1.0.0"
type = "inference"

[capabilities]
chat = true

[execution]
runtime = "python-uv"
entrypoint = "server:app"

[routing]
weight = "light"

[model]
source = "hf:test/model"
`),
		0644,
	))

	ctx := context.Background()
	_, err = mgr.Install(ctx, capsuleDir)
	require.NoError(t, err)

	// Set as active inference
	err = mgr.SetActiveInference(ctx, "mlx-qwen3")
	require.NoError(t, err)

	// Get active inference
	active, err := mgr.GetActiveInference(ctx)
	require.NoError(t, err)
	assert.Equal(t, "mlx-qwen3", active)
}

func TestManager_SetActiveInference_NonInferenceType(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()

	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)

	// Create and install a TOOL capsule (not inference)
	capsuleDir := filepath.Join(tmpDir, "my-tool")
	require.NoError(t, os.MkdirAll(capsuleDir, 0755))
	require.NoError(t, os.WriteFile(
		filepath.Join(capsuleDir, "capsule.toml"),
		[]byte(`
schema_version = "1.0"
name = "my-tool"
version = "1.0.0"
type = "tool"

[execution]
runtime = "python-uv"
entrypoint = "main:app"

[routing]
weight = "light"
`),
		0644,
	))

	ctx := context.Background()
	_, err = mgr.Install(ctx, capsuleDir)
	require.NoError(t, err)

	// Attempt to set tool as active inference - should fail
	err = mgr.SetActiveInference(ctx, "my-tool")
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "not an inference capsule")
}

func TestManager_SwitchInference(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()

	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)
	ctx := context.Background()

	// Install two inference capsules
	for _, name := range []string{"capsule-a", "capsule-b"} {
		capsuleDir := filepath.Join(tmpDir, name)
		require.NoError(t, os.MkdirAll(capsuleDir, 0755))
		require.NoError(t, os.WriteFile(
			filepath.Join(capsuleDir, "capsule.toml"),
			[]byte(`
schema_version = "1.0"
name = "`+name+`"
version = "1.0.0"
type = "inference"

[capabilities]
chat = true

[execution]
runtime = "python-uv"
entrypoint = "server:app"

[routing]
weight = "light"

[model]
source = "hf:test/model"
`),
			0644,
		))
		_, err = mgr.Install(ctx, capsuleDir)
		require.NoError(t, err)
	}

	// Set A as active
	err = mgr.SetActiveInference(ctx, "capsule-a")
	require.NoError(t, err)

	// Switch to B
	err = mgr.SwitchInference(ctx, "capsule-b")
	require.NoError(t, err)

	// Verify B is now active
	active, err := mgr.GetActiveInference(ctx)
	require.NoError(t, err)
	assert.Equal(t, "capsule-b", active)
}

func TestManager_ListInferenceCapsules(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()

	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)
	ctx := context.Background()

	// Install mixed capsules
	capsules := []struct {
		name     string
		capsType string
	}{
		{"inference-1", "inference"},
		{"inference-2", "inference"},
		{"tool-1", "tool"},
		{"app-1", "app"},
	}

	for _, c := range capsules {
		capsuleDir := filepath.Join(tmpDir, c.name)
		require.NoError(t, os.MkdirAll(capsuleDir, 0755))
		
		manifest := `
schema_version = "1.0"
name = "` + c.name + `"
version = "1.0.0"
type = "` + c.capsType + `"

[execution]
runtime = "python-uv"
entrypoint = "main:app"

[routing]
weight = "light"
`
		// Add required sections for inference capsules
		if c.capsType == "inference" {
			manifest += `
[capabilities]
chat = true

[model]
source = "hf:test/model"
`
		}

		require.NoError(t, os.WriteFile(
			filepath.Join(capsuleDir, "capsule.toml"),
			[]byte(manifest),
			0644,
		))
		_, err = mgr.Install(ctx, capsuleDir)
		require.NoError(t, err)
	}

	// List only inference capsules
	inferenceCapsules, err := mgr.ListInferenceCapsules(ctx)
	require.NoError(t, err)
	assert.Len(t, inferenceCapsules, 2)

	names := make([]string, len(inferenceCapsules))
	for i, c := range inferenceCapsules {
		names[i] = c.Name
	}
	assert.Contains(t, names, "inference-1")
	assert.Contains(t, names, "inference-2")
}

func TestManager_GetActiveInference_NoActive(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")
	st, err := store.NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer st.Close()

	mon := &mockMonitor{canRun: true}
	mgr := NewManager(st, mon, tmpDir)

	// No active inference set
	active, err := mgr.GetActiveInference(context.Background())
	require.NoError(t, err)
	assert.Empty(t, active)
}


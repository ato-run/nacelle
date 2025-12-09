package store

import (
	"context"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewSQLiteStore(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	assert.NotNil(t, store)
}

func TestSQLiteStore_Install(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()
	capsule := &Capsule{
		Name:         "test-capsule",
		Version:      "1.0.0",
		Type:         "inference",
		ManifestPath: "/path/to/capsule.toml",
		Status:       StatusStopped,
		InstalledAt:  time.Now(),
	}

	err = store.Install(ctx, capsule)
	require.NoError(t, err)

	// Verify
	retrieved, err := store.Get(ctx, "test-capsule")
	require.NoError(t, err)
	assert.Equal(t, "test-capsule", retrieved.Name)
	assert.Equal(t, "1.0.0", retrieved.Version)
	assert.Equal(t, StatusStopped, retrieved.Status)
}

func TestSQLiteStore_List(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()

	// Install multiple capsules
	for _, name := range []string{"capsule-a", "capsule-b", "capsule-c"} {
		err := store.Install(ctx, &Capsule{
			Name:         name,
			Version:      "1.0.0",
			Type:         "tool",
			ManifestPath: "/path/to/" + name + "/capsule.toml",
			Status:       StatusStopped,
			InstalledAt:  time.Now(),
		})
		require.NoError(t, err)
	}

	capsules, err := store.List(ctx)
	require.NoError(t, err)
	assert.Len(t, capsules, 3)
}

func TestSQLiteStore_UpdateStatus(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()
	err = store.Install(ctx, &Capsule{
		Name:         "status-test",
		Version:      "1.0.0",
		Type:         "inference",
		ManifestPath: "/path/to/capsule.toml",
		Status:       StatusStopped,
		InstalledAt:  time.Now(),
	})
	require.NoError(t, err)

	// Update status
	err = store.UpdateStatus(ctx, "status-test", StatusRunning)
	require.NoError(t, err)

	// Verify
	capsule, err := store.Get(ctx, "status-test")
	require.NoError(t, err)
	assert.Equal(t, StatusRunning, capsule.Status)
}

func TestSQLiteStore_Delete(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()
	err = store.Install(ctx, &Capsule{
		Name:         "delete-test",
		Version:      "1.0.0",
		Type:         "tool",
		ManifestPath: "/path/to/capsule.toml",
		Status:       StatusStopped,
		InstalledAt:  time.Now(),
	})
	require.NoError(t, err)

	// Delete
	err = store.Delete(ctx, "delete-test")
	require.NoError(t, err)

	// Verify deleted
	_, err = store.Get(ctx, "delete-test")
	assert.Error(t, err)
}

func TestSQLiteStore_RecordProcess(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()
	err = store.Install(ctx, &Capsule{
		Name:         "process-test",
		Version:      "1.0.0",
		Type:         "inference",
		ManifestPath: "/path/to/capsule.toml",
		Status:       StatusStopped,
		InstalledAt:  time.Now(),
	})
	require.NoError(t, err)

	// Record start
	err = store.RecordStart(ctx, "process-test", 12345)
	require.NoError(t, err)

	// Get process
	proc, err := store.GetProcess(ctx, "process-test")
	require.NoError(t, err)
	assert.NotNil(t, proc)
	assert.Equal(t, 12345, proc.PID)

	// Status should be starting
	capsule, _ := store.Get(ctx, "process-test")
	assert.Equal(t, StatusStarting, capsule.Status)

	// Record stop
	err = store.RecordStop(ctx, "process-test", 12345)
	require.NoError(t, err)

	// Process should be gone
	proc, err = store.GetProcess(ctx, "process-test")
	require.NoError(t, err)
	assert.Nil(t, proc)
}

func TestSQLiteStore_HardwareSnapshots(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()

	// Record snapshot
	snapshot := &HardwareSnapshot{
		Timestamp:       time.Now(),
		TotalVRAMGB:     16.0,
		AvailableVRAMGB: 10.0,
		TotalRAMGB:      32.0,
		AvailableRAMGB:  20.0,
		CPUUsagePercent: 25.0,
	}
	err = store.RecordHardwareSnapshot(ctx, snapshot)
	require.NoError(t, err)

	// Get latest
	latest, err := store.GetLatestHardware(ctx)
	require.NoError(t, err)
	assert.NotNil(t, latest)
	assert.Equal(t, 16.0, latest.TotalVRAMGB)
	assert.Equal(t, 10.0, latest.AvailableVRAMGB)
}

func TestSQLiteStore_GetNonExistent(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()
	_, err = store.Get(ctx, "nonexistent")
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "capsule not found")
}

func TestSQLiteStore_RouteDecisions(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()

	// Record route decisions
	err = store.RecordRouteDecision(ctx, "mlx-qwen3-8b", "local", "", 45.0)
	require.NoError(t, err)

	err = store.RecordRouteDecision(ctx, "vllm-llama70b", "cloud", "heavy capsule", 80.0)
	require.NoError(t, err)

	// Get recent decisions
	decisions, err := store.GetRecentRouteDecisions(ctx, 10)
	require.NoError(t, err)
	assert.Len(t, decisions, 2)

	// Most recent first
	assert.Equal(t, "vllm-llama70b", decisions[0].CapsuleName)
	assert.Equal(t, "cloud", decisions[0].Decision)
	assert.Equal(t, "heavy capsule", decisions[0].Reason)
	assert.Equal(t, 80.0, decisions[0].VRAMUsagePercent)

	assert.Equal(t, "mlx-qwen3-8b", decisions[1].CapsuleName)
	assert.Equal(t, "local", decisions[1].Decision)
}

func TestSQLiteStore_LocalNode(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.db")

	store, err := NewSQLiteStore(dbPath)
	require.NoError(t, err)
	defer store.Close()

	ctx := context.Background()

	// Initially no local node
	cfg, err := store.GetLocalNode(ctx)
	require.NoError(t, err)
	assert.Nil(t, cfg)

	// Set local node
	nodeCfg := &LocalNodeConfig{
		NodeID:    "node-123",
		Hostname:  "my-macbook",
		TailnetIP: "100.64.0.1",
		IsOnline:  true,
	}
	err = store.SetLocalNode(ctx, nodeCfg)
	require.NoError(t, err)

	// Get local node
	retrieved, err := store.GetLocalNode(ctx)
	require.NoError(t, err)
	assert.NotNil(t, retrieved)
	assert.Equal(t, "node-123", retrieved.NodeID)
	assert.Equal(t, "my-macbook", retrieved.Hostname)
	assert.Equal(t, "100.64.0.1", retrieved.TailnetIP)
	assert.True(t, retrieved.IsOnline)

	// Update local node
	nodeCfg.IsOnline = false
	err = store.SetLocalNode(ctx, nodeCfg)
	require.NoError(t, err)

	retrieved, err = store.GetLocalNode(ctx)
	require.NoError(t, err)
	assert.False(t, retrieved.IsOnline)
}


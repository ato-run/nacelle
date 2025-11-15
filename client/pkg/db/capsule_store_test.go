package db

import (
	"context"
	"database/sql"
	"testing"
	"time"

	_ "modernc.org/sqlite" // Import sqlite driver
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func setupTestCapsuleStore(t *testing.T) (*CapsuleStore, func()) {
	db := setupTestDB(t)
	store := NewCapsuleStore(db)
	
	cleanup := func() {
		db.Close()
	}
	
	return store, cleanup
}

func TestCapsuleStore_Create(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	capsule := &Capsule{
		Name:          "test-capsule",
		NodeID:        "node-123",
		Manifest:      `{"name":"test","version":"1.0.0"}`,
		Status:        CapsuleStatusPending,
		StoragePath:   "/var/lib/capsules/test",
		BundlePath:    "/var/lib/bundles/test",
		NetworkConfig: `{"port":8080}`,
	}
	
	err := store.Create(ctx, capsule)
	require.NoError(t, err)
	assert.NotEmpty(t, capsule.ID)
	assert.False(t, capsule.CreatedAt.IsZero())
	assert.False(t, capsule.UpdatedAt.IsZero())
}

func TestCapsuleStore_Get(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Create a capsule first
	capsule := &Capsule{
		Name:     "test-capsule",
		NodeID:   "node-123",
		Manifest: `{"name":"test"}`,
		Status:   CapsuleStatusRunning,
	}
	err := store.Create(ctx, capsule)
	require.NoError(t, err)
	
	// Get the capsule
	retrieved, err := store.Get(ctx, capsule.ID)
	require.NoError(t, err)
	assert.Equal(t, capsule.ID, retrieved.ID)
	assert.Equal(t, capsule.Name, retrieved.Name)
	assert.Equal(t, capsule.NodeID, retrieved.NodeID)
	assert.Equal(t, capsule.Status, retrieved.Status)
}

func TestCapsuleStore_Get_NotFound(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	_, err := store.Get(ctx, "nonexistent-id")
	require.Error(t, err)
	assert.Contains(t, err.Error(), "capsule not found")
}

func TestCapsuleStore_List(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Create multiple capsules
	capsules := []*Capsule{
		{Name: "capsule-1", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusRunning},
		{Name: "capsule-2", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusRunning},
		{Name: "capsule-3", NodeID: "node-2", Manifest: `{}`, Status: CapsuleStatusPending},
	}
	
	for _, c := range capsules {
		err := store.Create(ctx, c)
		require.NoError(t, err)
	}
	
	// List all capsules
	all, err := store.List(ctx, "", "")
	require.NoError(t, err)
	assert.Len(t, all, 3)
	
	// List capsules by node
	node1Capsules, err := store.List(ctx, "node-1", "")
	require.NoError(t, err)
	assert.Len(t, node1Capsules, 2)
	
	// List capsules by status
	runningCapsules, err := store.List(ctx, "", CapsuleStatusRunning)
	require.NoError(t, err)
	assert.Len(t, runningCapsules, 2)
	
	// List capsules by node and status
	node1Running, err := store.List(ctx, "node-1", CapsuleStatusRunning)
	require.NoError(t, err)
	assert.Len(t, node1Running, 2)
}

func TestCapsuleStore_Update(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Create a capsule
	capsule := &Capsule{
		Name:     "test-capsule",
		NodeID:   "node-123",
		Manifest: `{"version":"1.0.0"}`,
		Status:   CapsuleStatusPending,
	}
	err := store.Create(ctx, capsule)
	require.NoError(t, err)
	
	originalUpdatedAt := capsule.UpdatedAt
	time.Sleep(10 * time.Millisecond) // Ensure time difference
	
	// Update the capsule
	capsule.Name = "updated-capsule"
	capsule.Status = CapsuleStatusRunning
	capsule.Manifest = `{"version":"2.0.0"}`
	
	err = store.Update(ctx, capsule)
	require.NoError(t, err)
	assert.True(t, capsule.UpdatedAt.After(originalUpdatedAt))
	
	// Verify the update
	retrieved, err := store.Get(ctx, capsule.ID)
	require.NoError(t, err)
	assert.Equal(t, "updated-capsule", retrieved.Name)
	assert.Equal(t, CapsuleStatusRunning, retrieved.Status)
	assert.Equal(t, `{"version":"2.0.0"}`, retrieved.Manifest)
}

func TestCapsuleStore_Update_NotFound(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	capsule := &Capsule{
		ID:       "nonexistent-id",
		Name:     "test",
		NodeID:   "node-1",
		Manifest: `{}`,
		Status:   CapsuleStatusRunning,
	}
	
	err := store.Update(ctx, capsule)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "capsule not found")
}

func TestCapsuleStore_UpdateStatus(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Create a capsule
	capsule := &Capsule{
		Name:     "test-capsule",
		NodeID:   "node-123",
		Manifest: `{}`,
		Status:   CapsuleStatusPending,
	}
	err := store.Create(ctx, capsule)
	require.NoError(t, err)
	
	// Update status
	err = store.UpdateStatus(ctx, capsule.ID, CapsuleStatusRunning)
	require.NoError(t, err)
	
	// Verify the update
	retrieved, err := store.Get(ctx, capsule.ID)
	require.NoError(t, err)
	assert.Equal(t, CapsuleStatusRunning, retrieved.Status)
}

func TestCapsuleStore_Delete(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Create a capsule
	capsule := &Capsule{
		Name:     "test-capsule",
		NodeID:   "node-123",
		Manifest: `{}`,
		Status:   CapsuleStatusRunning,
	}
	err := store.Create(ctx, capsule)
	require.NoError(t, err)
	
	// Delete the capsule
	err = store.Delete(ctx, capsule.ID)
	require.NoError(t, err)
	
	// Verify deletion
	_, err = store.Get(ctx, capsule.ID)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "capsule not found")
}

func TestCapsuleStore_Delete_NotFound(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	err := store.Delete(ctx, "nonexistent-id")
	require.Error(t, err)
	assert.Contains(t, err.Error(), "capsule not found")
}

func TestCapsuleStore_GetByNodeID(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Create capsules on different nodes
	capsules := []*Capsule{
		{Name: "capsule-1", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusRunning},
		{Name: "capsule-2", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusPending},
		{Name: "capsule-3", NodeID: "node-2", Manifest: `{}`, Status: CapsuleStatusRunning},
	}
	
	for _, c := range capsules {
		err := store.Create(ctx, c)
		require.NoError(t, err)
	}
	
	// Get capsules for node-1
	node1Capsules, err := store.GetByNodeID(ctx, "node-1")
	require.NoError(t, err)
	assert.Len(t, node1Capsules, 2)
	
	for _, c := range node1Capsules {
		assert.Equal(t, "node-1", c.NodeID)
	}
}

func TestCapsuleStore_GetByStatus(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Create capsules with different statuses
	capsules := []*Capsule{
		{Name: "capsule-1", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusRunning},
		{Name: "capsule-2", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusRunning},
		{Name: "capsule-3", NodeID: "node-2", Manifest: `{}`, Status: CapsuleStatusPending},
	}
	
	for _, c := range capsules {
		err := store.Create(ctx, c)
		require.NoError(t, err)
	}
	
	// Get running capsules
	runningCapsules, err := store.GetByStatus(ctx, CapsuleStatusRunning)
	require.NoError(t, err)
	assert.Len(t, runningCapsules, 2)
	
	for _, c := range runningCapsules {
		assert.Equal(t, CapsuleStatusRunning, c.Status)
	}
}

func TestCapsuleStore_Count(t *testing.T) {
	store, cleanup := setupTestCapsuleStore(t)
	defer cleanup()
	
	ctx := context.Background()
	
	// Initially no capsules
	count, err := store.Count(ctx, "")
	require.NoError(t, err)
	assert.Equal(t, int64(0), count)
	
	// Create capsules
	capsules := []*Capsule{
		{Name: "capsule-1", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusRunning},
		{Name: "capsule-2", NodeID: "node-1", Manifest: `{}`, Status: CapsuleStatusRunning},
		{Name: "capsule-3", NodeID: "node-2", Manifest: `{}`, Status: CapsuleStatusPending},
	}
	
	for _, c := range capsules {
		err := store.Create(ctx, c)
		require.NoError(t, err)
	}
	
	// Count all capsules
	count, err = store.Count(ctx, "")
	require.NoError(t, err)
	assert.Equal(t, int64(3), count)
	
	// Count running capsules
	runningCount, err := store.Count(ctx, CapsuleStatusRunning)
	require.NoError(t, err)
	assert.Equal(t, int64(2), runningCount)
	
	// Count pending capsules
	pendingCount, err := store.Count(ctx, CapsuleStatusPending)
	require.NoError(t, err)
	assert.Equal(t, int64(1), pendingCount)
}

// Helper function from models_test.go
func setupTestDB(t *testing.T) *sql.DB {
	db, err := sql.Open("sqlite", ":memory:")
	require.NoError(t, err)
	
	// Read and execute schema
	schema := `
	CREATE TABLE IF NOT EXISTS capsules (
		id TEXT PRIMARY KEY,
		name TEXT NOT NULL,
		node_id TEXT NOT NULL,
		manifest TEXT NOT NULL,
		status TEXT NOT NULL,
		storage_path TEXT,
		bundle_path TEXT,
		network_config TEXT,
		created_at INTEGER NOT NULL,
		updated_at INTEGER NOT NULL
	);
	
	CREATE INDEX IF NOT EXISTS idx_capsules_node_id ON capsules(node_id);
	CREATE INDEX IF NOT EXISTS idx_capsules_status ON capsules(status);
	CREATE INDEX IF NOT EXISTS idx_capsules_name ON capsules(name);
	`
	
	_, err = db.Exec(schema)
	require.NoError(t, err)
	
	return db
}

package api

import (
	"context"
	"database/sql"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	_ "modernc.org/sqlite"

	"github.com/onescluster/coordinator/pkg/db"
)

func setupTestHandler(t *testing.T) (*CapsuleHandler, *sql.DB, func()) {
	database, err := sql.Open("sqlite", ":memory:")
	require.NoError(t, err)

	// Create schema
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
	`
	_, err = database.Exec(schema)
	require.NoError(t, err)

	nodeStore := db.NewNodeStore(database)
	capsuleStore := db.NewCapsuleStore(database)
	handler := NewCapsuleHandler(nodeStore, capsuleStore)

	cleanup := func() {
		database.Close()
	}

	return handler, database, cleanup
}

func TestCapsuleHandler_HandleGetCapsule(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	ctx := context.Background()

	// Create a test capsule
	capsule := &db.Capsule{
		Name:     "test-capsule",
		NodeID:   "node-123",
		Manifest: `{"name":"test"}`,
		Status:   db.CapsuleStatusRunning,
	}
	err := handler.CapsuleStore.Create(ctx, capsule)
	require.NoError(t, err)

	// Test successful retrieval
	req := httptest.NewRequest(http.MethodGet, "/api/v1/capsules/"+capsule.ID, nil)
	rec := httptest.NewRecorder()

	handler.HandleGetCapsule(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)
	assert.Contains(t, rec.Header().Get("Content-Type"), "application/json")

	var response db.Capsule
	err = json.NewDecoder(rec.Body).Decode(&response)
	require.NoError(t, err)
	assert.Equal(t, capsule.ID, response.ID)
	assert.Equal(t, capsule.Name, response.Name)
}

func TestCapsuleHandler_HandleGetCapsule_NotFound(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	// Test non-existent capsule
	req := httptest.NewRequest(http.MethodGet, "/api/v1/capsules/nonexistent-id", nil)
	rec := httptest.NewRecorder()

	handler.HandleGetCapsule(rec, req)

	assert.Equal(t, http.StatusNotFound, rec.Code)
	assert.Contains(t, rec.Body.String(), "Capsule not found")
}

func TestCapsuleHandler_HandleGetCapsule_InvalidMethod(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	req := httptest.NewRequest(http.MethodPost, "/api/v1/capsules/some-id", nil)
	rec := httptest.NewRecorder()

	handler.HandleGetCapsule(rec, req)

	assert.Equal(t, http.StatusMethodNotAllowed, rec.Code)
}

func TestCapsuleHandler_HandleListCapsules(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	ctx := context.Background()

	// Create test capsules
	capsules := []*db.Capsule{
		{Name: "capsule-1", NodeID: "node-1", Manifest: `{}`, Status: db.CapsuleStatusRunning},
		{Name: "capsule-2", NodeID: "node-1", Manifest: `{}`, Status: db.CapsuleStatusPending},
		{Name: "capsule-3", NodeID: "node-2", Manifest: `{}`, Status: db.CapsuleStatusRunning},
	}

	for _, c := range capsules {
		err := handler.CapsuleStore.Create(ctx, c)
		require.NoError(t, err)
	}

	// Test listing all capsules
	req := httptest.NewRequest(http.MethodGet, "/api/v1/capsules", nil)
	rec := httptest.NewRecorder()

	handler.HandleListCapsules(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)
	assert.Contains(t, rec.Header().Get("Content-Type"), "application/json")

	var response map[string]interface{}
	err := json.NewDecoder(rec.Body).Decode(&response)
	require.NoError(t, err)

	assert.Contains(t, response, "capsules")
	assert.Contains(t, response, "count")

	count := int(response["count"].(float64))
	assert.Equal(t, 3, count)
}

func TestCapsuleHandler_HandleListCapsules_Empty(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	req := httptest.NewRequest(http.MethodGet, "/api/v1/capsules", nil)
	rec := httptest.NewRecorder()

	handler.HandleListCapsules(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)

	var response map[string]interface{}
	err := json.NewDecoder(rec.Body).Decode(&response)
	require.NoError(t, err)

	count := int(response["count"].(float64))
	assert.Equal(t, 0, count)
}

func TestCapsuleHandler_HandleListCapsules_InvalidMethod(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	req := httptest.NewRequest(http.MethodPost, "/api/v1/capsules", nil)
	rec := httptest.NewRecorder()

	handler.HandleListCapsules(rec, req)

	assert.Equal(t, http.StatusMethodNotAllowed, rec.Code)
}

func TestCapsuleHandler_HandleDeleteCapsule(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	ctx := context.Background()

	// Create a test capsule
	capsule := &db.Capsule{
		Name:     "test-capsule",
		NodeID:   "node-123",
		Manifest: `{"name":"test"}`,
		Status:   db.CapsuleStatusRunning,
	}
	err := handler.CapsuleStore.Create(ctx, capsule)
	require.NoError(t, err)

	// Test successful deletion
	req := httptest.NewRequest(http.MethodDelete, "/api/v1/capsules/"+capsule.ID, nil)
	rec := httptest.NewRecorder()

	handler.HandleDeleteCapsule(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)
	assert.Contains(t, rec.Header().Get("Content-Type"), "application/json")

	var response map[string]interface{}
	err = json.NewDecoder(rec.Body).Decode(&response)
	require.NoError(t, err)
	assert.Equal(t, true, response["success"])
	assert.Contains(t, response["message"], "deleted successfully")

	// Verify capsule is deleted
	_, err = handler.CapsuleStore.Get(ctx, capsule.ID)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "capsule not found")
}

func TestCapsuleHandler_HandleDeleteCapsule_NotFound(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	req := httptest.NewRequest(http.MethodDelete, "/api/v1/capsules/nonexistent-id", nil)
	rec := httptest.NewRecorder()

	handler.HandleDeleteCapsule(rec, req)

	assert.Equal(t, http.StatusNotFound, rec.Code)
	assert.Contains(t, rec.Body.String(), "Capsule not found")
}

func TestCapsuleHandler_HandleDeleteCapsule_InvalidMethod(t *testing.T) {
	handler, _, cleanup := setupTestHandler(t)
	defer cleanup()

	req := httptest.NewRequest(http.MethodGet, "/api/v1/capsules/some-id", nil)
	rec := httptest.NewRecorder()

	handler.HandleDeleteCapsule(rec, req)

	assert.Equal(t, http.StatusMethodNotAllowed, rec.Code)
}

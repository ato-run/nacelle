package api

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/onescluster/coordinator/pkg/db"
)

func setupTestHandler(t *testing.T) (*CapsuleHandler, func()) {
	sm := db.NewStateManager(nil)
	h := NewCapsuleHandler(sm, nil)

	cleanup := func() {}
	return h, cleanup
}

func TestCapsuleHandler_HandleGetCapsule(t *testing.T) {
	handler, cleanup := setupTestHandler(t)
	defer cleanup()

	// Seed a test capsule in the in-memory state
	capsule := &db.Capsule{ID: "cap-1", Name: "test-capsule", NodeID: "node-123", Manifest: `{"name":"test"}`, Status: db.CapsuleStatusRunning}
	handler.StateManager.SetCapsuleInCache(capsule)

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
	handler, cleanup := setupTestHandler(t)
	defer cleanup()

	// Test non-existent capsule
	req := httptest.NewRequest(http.MethodGet, "/api/v1/capsules/nonexistent-id", nil)
	rec := httptest.NewRecorder()

	handler.HandleGetCapsule(rec, req)

	assert.Equal(t, http.StatusNotFound, rec.Code)
	assert.Contains(t, rec.Body.String(), "Capsule not found")
}

func TestCapsuleHandler_HandleGetCapsule_InvalidMethod(t *testing.T) {
	handler, cleanup := setupTestHandler(t)
	defer cleanup()

	req := httptest.NewRequest(http.MethodPost, "/api/v1/capsules/some-id", nil)
	rec := httptest.NewRecorder()

	handler.HandleGetCapsule(rec, req)

	assert.Equal(t, http.StatusMethodNotAllowed, rec.Code)
}

func TestCapsuleHandler_HandleListCapsules(t *testing.T) {
	handler, cleanup := setupTestHandler(t)
	defer cleanup()

	// Seed capsules in memory
	handler.StateManager.SetCapsuleInCache(&db.Capsule{ID: "cap-1", Name: "capsule-1", NodeID: "node-1", Manifest: `{}`, Status: db.CapsuleStatusRunning})
	handler.StateManager.SetCapsuleInCache(&db.Capsule{ID: "cap-2", Name: "capsule-2", NodeID: "node-1", Manifest: `{}`, Status: db.CapsuleStatusPending})
	handler.StateManager.SetCapsuleInCache(&db.Capsule{ID: "cap-3", Name: "capsule-3", NodeID: "node-2", Manifest: `{}`, Status: db.CapsuleStatusRunning})

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
	handler, cleanup := setupTestHandler(t)
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
	handler, cleanup := setupTestHandler(t)
	defer cleanup()

	req := httptest.NewRequest(http.MethodPost, "/api/v1/capsules", nil)
	rec := httptest.NewRecorder()

	handler.HandleListCapsules(rec, req)

	assert.Equal(t, http.StatusMethodNotAllowed, rec.Code)
}

package api

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	"github.com/onescluster/coordinator/pkg/db"
)

// CapsuleHandler handles Capsule CRUD operations
type CapsuleHandler struct {
	StateManager *db.StateManager
}

// NewCapsuleHandler creates a new capsule handler
func NewCapsuleHandler(stateManager *db.StateManager) *CapsuleHandler {
	return &CapsuleHandler{
		StateManager: stateManager,
	}
}

// HandleGetCapsule retrieves a specific capsule by ID
// GET /api/v1/capsules/:id
func (h *CapsuleHandler) HandleGetCapsule(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Extract capsule ID from URL path
	// Expected format: /api/v1/capsules/{id}
	path := r.URL.Path
	parts := strings.Split(strings.Trim(path, "/"), "/")
	if len(parts) < 4 {
		http.Error(w, "Invalid URL path", http.StatusBadRequest)
		return
	}
	capsuleID := parts[3]

	if capsuleID == "" {
		http.Error(w, "Capsule ID required", http.StatusBadRequest)
		return
	}

	// Query capsule from StateManager
	capsule, exists := h.StateManager.GetCapsule(capsuleID)
	if !exists {
		http.Error(w, fmt.Sprintf("Capsule not found: %s", capsuleID), http.StatusNotFound)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(capsule)
}

// HandleListCapsules lists all capsules
// GET /api/v1/capsules
func (h *CapsuleHandler) HandleListCapsules(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Query all capsules from StateManager
	capsules := h.StateManager.GetAllCapsules()

	response := map[string]interface{}{
		"capsules": capsules,
		"count":    len(capsules),
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(response)
}

// HandleDeleteCapsule deletes a specific capsule
// DELETE /api/v1/capsules/:id
func (h *CapsuleHandler) HandleDeleteCapsule(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodDelete {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Extract capsule ID from URL path
	path := r.URL.Path
	parts := strings.Split(strings.Trim(path, "/"), "/")
	if len(parts) < 4 {
		http.Error(w, "Invalid URL path", http.StatusBadRequest)
		return
	}
	capsuleID := parts[3]

	if capsuleID == "" {
		http.Error(w, "Capsule ID required", http.StatusBadRequest)
		return
	}

	// For now, just delete from StateManager
	err := h.StateManager.DeleteCapsule(capsuleID)
	if err != nil {
		http.Error(w, fmt.Sprintf("Failed to delete capsule: %v", err), http.StatusInternalServerError)
		return
	}

	response := map[string]interface{}{
		"success": true,
		"message": fmt.Sprintf("Capsule %s deleted successfully", capsuleID),
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(response)
}

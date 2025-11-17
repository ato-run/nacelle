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
	NodeStore    *db.NodeStore
	CapsuleStore *db.CapsuleStore
}

// NewCapsuleHandler creates a new capsule handler
func NewCapsuleHandler(nodeStore *db.NodeStore, capsuleStore *db.CapsuleStore) *CapsuleHandler {
	return &CapsuleHandler{
		NodeStore:    nodeStore,
		CapsuleStore: capsuleStore,
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

	ctx := r.Context()

	// Query capsule from CapsuleStore
	capsule, err := h.CapsuleStore.Get(ctx, capsuleID)
	if err != nil {
		if strings.Contains(err.Error(), "capsule not found") {
			http.Error(w, fmt.Sprintf("Capsule not found: %s", capsuleID), http.StatusNotFound)
		} else {
			http.Error(w, fmt.Sprintf("Failed to retrieve capsule: %v", err), http.StatusInternalServerError)
		}
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

	ctx := r.Context()

	// Query all capsules from CapsuleStore
	capsules, err := h.CapsuleStore.List(ctx, "", "")
	if err != nil {
		http.Error(w, fmt.Sprintf("Failed to list capsules: %v", err), http.StatusInternalServerError)
		return
	}

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

	ctx := r.Context()

	// TODO: Delete capsule via gRPC call to Engine
	// 1. Query capsule to find which Rig it's on
	// 2. Call Engine's StopWorkload RPC
	// 3. Release VRAM reservation
	// This will be implemented in Phase 1 Week 2

	// For now, just delete from CapsuleStore
	err := h.CapsuleStore.Delete(ctx, capsuleID)
	if err != nil {
		if strings.Contains(err.Error(), "capsule not found") {
			http.Error(w, fmt.Sprintf("Capsule not found: %s", capsuleID), http.StatusNotFound)
		} else {
			http.Error(w, fmt.Sprintf("Failed to delete capsule: %v", err), http.StatusInternalServerError)
		}
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

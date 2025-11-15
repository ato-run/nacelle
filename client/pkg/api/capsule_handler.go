package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	"github.com/onescluster/coordinator/pkg/db"
)

// CapsuleHandler handles Capsule CRUD operations
type CapsuleHandler struct {
	NodeStore *db.NodeStore
	// TODO: Add CapsuleStore when state management is implemented
}

// NewCapsuleHandler creates a new capsule handler
func NewCapsuleHandler(nodeStore *db.NodeStore) *CapsuleHandler {
	return &CapsuleHandler{
		NodeStore: nodeStore,
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

	// TODO: Query capsule from CapsuleStore
	// For now, return a placeholder response
	response := map[string]interface{}{
		"id":     capsuleID,
		"status": "running",
		"message": "Capsule retrieval not fully implemented yet",
		"note": "This is a placeholder. Implement CapsuleStore in Phase 1 Week 1.",
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(response)
}

// HandleListCapsules lists all capsules
// GET /api/v1/capsules
func (h *CapsuleHandler) HandleListCapsules(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	ctx := r.Context()
	
	// TODO: Query all capsules from CapsuleStore
	// For now, return Rigs info as a workaround to show something
	rigs, err := h.NodeStore.GetAllGpuRigs(ctx)
	if err != nil {
		http.Error(w, fmt.Sprintf("Failed to query rigs: %v", err), http.StatusInternalServerError)
		return
	}

	response := map[string]interface{}{
		"capsules": []interface{}{},
		"rigs_count": len(rigs),
		"message": "Capsule listing not fully implemented yet",
		"note": "This is a placeholder. Implement CapsuleStore in Phase 1 Week 1.",
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

	ctx := context.Background()
	_ = ctx

	// TODO: Delete capsule via gRPC call to Engine
	// 1. Query capsule to find which Rig it's on
	// 2. Call Engine's StopWorkload RPC
	// 3. Release VRAM reservation
	// 4. Delete from CapsuleStore

	response := map[string]interface{}{
		"success": true,
		"message": fmt.Sprintf("Capsule %s deletion requested", capsuleID),
		"note": "This is a placeholder. Implement deletion in Phase 1 Week 1.",
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(response)
}

package api

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
	"github.com/onescluster/coordinator/pkg/supabase"
)

// CapsuleHandler handles Capsule CRUD operations
type CapsuleHandler struct {
	StateManager *db.StateManager
	Supabase     *supabase.Client
}

// NewCapsuleHandler creates a new capsule handler
func NewCapsuleHandler(stateManager *db.StateManager, supabase *supabase.Client) *CapsuleHandler {
	return &CapsuleHandler{
		StateManager: stateManager,
		Supabase:     supabase,
	}
}

// StreamLogs streams capsule logs over Server-Sent Events (SSE).
// GET /api/v1/capsules/:id/logs
func (h *CapsuleHandler) StreamLogs(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	capsuleID, err := extractCapsuleIDFromLogsPath(r.URL.Path)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "Streaming unsupported", http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("Access-Control-Allow-Origin", "*")

	ctx := r.Context()
	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case t := <-ticker.C:
			fmt.Fprintf(w, "data: Log entry at %s for capsule %s\n\n", t.Format(time.RFC3339), capsuleID)
			flusher.Flush()
		}
	}
}

// extractCapsuleIDFromLogsPath parses /api/v1/capsules/{id}/logs and returns {id}.
func extractCapsuleIDFromLogsPath(path string) (string, error) {
	const prefix = "/api/v1/capsules/"
	const suffix = "/logs"

	if !strings.HasPrefix(path, prefix) || !strings.HasSuffix(path, suffix) {
		return "", fmt.Errorf("invalid logs path")
	}

	trimmed := strings.TrimPrefix(path, prefix)
	trimmed = strings.TrimSuffix(trimmed, suffix)
	trimmed = strings.Trim(trimmed, "/")

	if trimmed == "" {
		return "", fmt.Errorf("capsule id required")
	}

	return trimmed, nil
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

	// Get capsule before deleting to log usage
	capsule, exists := h.StateManager.GetCapsule(capsuleID)
	if exists && capsule.UserID != "" {
		duration := time.Since(capsule.CreatedAt).Hours()
		// Log usage
		go func() {
			err := h.Supabase.LogUsage(supabase.UsageLog{
				UserID:    capsule.UserID,
				CapsuleID: capsule.ID,
				Resource:  "compute_hours",
				Amount:    duration,
				StartTime: capsule.CreatedAt,
				EndTime:   time.Now(),
			})
			if err != nil {
				fmt.Printf("Failed to log usage: %v\n", err)
			}
		}()
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

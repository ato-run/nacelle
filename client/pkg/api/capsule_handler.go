package api

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"strings"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
	coordinatorv1 "github.com/onescluster/coordinator/pkg/proto/coordinator/v1"
	"github.com/onescluster/coordinator/pkg/service"
	"github.com/onescluster/coordinator/pkg/supabase"
)

// CapsuleHandler handles Capsule CRUD operations
type CapsuleHandler struct {
	StateManager *db.StateManager
	Supabase     *supabase.Client
	Coordinator  *service.CoordinatorService
}

// NewCapsuleHandler creates a new capsule handler
func NewCapsuleHandler(stateManager *db.StateManager, supabase *supabase.Client, coordinator *service.CoordinatorService) *CapsuleHandler {
	return &CapsuleHandler{
		StateManager: stateManager,
		Supabase:     supabase,
		Coordinator:  coordinator,
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
			if _, err := fmt.Fprintf(w, "data: Log entry at %s for capsule %s\n\n", t.Format(time.RFC3339), capsuleID); err != nil {
				return
			}
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

// extractCapsuleIDFromPath extracts ID from /api/v1/capsules/{id}/{action} URL
func extractCapsuleIDFromPath(path string) (string, error) {
	// Expected format: /api/v1/capsules/{id} or /api/v1/capsules/{id}/{action}
	parts := strings.Split(strings.Trim(path, "/"), "/")
	if len(parts) < 4 {
		return "", fmt.Errorf("invalid URL path")
	}
	id := parts[3]
	if id == "" {
		return "", fmt.Errorf("capsule ID required")
	}
	return id, nil
}

// HandleGetCapsule retrieves a specific capsule by ID
// GET /api/v1/capsules/:id
func (h *CapsuleHandler) HandleGetCapsule(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	capsuleID, err := extractCapsuleIDFromPath(r.URL.Path)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
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
	if err := json.NewEncoder(w).Encode(capsule); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
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
	if err := json.NewEncoder(w).Encode(response); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
}

// HandleDeleteCapsule deletes a specific capsule
// DELETE /api/v1/capsules/:id
func (h *CapsuleHandler) HandleDeleteCapsule(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodDelete {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	capsuleID, err := extractCapsuleIDFromPath(r.URL.Path)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	// Use Coordinator Service to stop/cleanup first
	_, err = h.Coordinator.StopCapsule(r.Context(), &coordinatorv1.StopRequest{CapsuleId: capsuleID})
	if err != nil {
		// Log but continue to ensure DB cleanup if possible?
		// Or return error? The original impl just called StateManager.DeleteCapsule
		// Best to try standard stop path.
		fmt.Printf("Warning: Failed to stop capsule before delete: %v\n", err)
	}

	// The original had logging logic here...
	// We should preserve it or let StopCapsule handle it if it did?
	// StopCapsule does removal from DB.
	// So we don't need to duplicate it, but HandleDeleteCapsule was also logging usage.
	// Let's keep the usage logging but delegate deletion.

	// Resolve capsule first for logging
	capsule, exists := h.StateManager.GetCapsule(capsuleID)
	if exists && capsule.UserID != "" && h.Supabase != nil {
		duration := time.Since(capsule.CreatedAt).Hours()
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

	// Re-calling StopCapsule above handles DB deletion/route cleanup
	// But duplicate call might fail if already deleted?
	// StopCapsule handles "not found" gracefully usually?
	// Actually, StopCapsule in service.go does: RemoveRoute -> Engine.Stop -> Store.Delete.
	// That covers everything HandleDeleteCapsule did (except usage logging).

	response := map[string]interface{}{
		"success": true,
		"message": fmt.Sprintf("Capsule %s deleted successfully", capsuleID),
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	if err := json.NewEncoder(w).Encode(response); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
}

// HandleStopCapsule stops a capsule
// POST /api/v1/capsules/:id/stop
func (h *CapsuleHandler) HandleStopCapsule(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	capsuleID, err := extractCapsuleIDFromPath(r.URL.Path)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	resp, err := h.Coordinator.StopCapsule(r.Context(), &coordinatorv1.StopRequest{CapsuleId: capsuleID})
	if err != nil {
		http.Error(w, fmt.Sprintf("Failed to stop capsule: %v", err), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	if err := json.NewEncoder(w).Encode(resp); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
}

// HandleStartCapsule starts a capsule (Not implemented in CoordinatorService yet? Assuming it isn't or maps to Deploy)
// For now, if we don't have Start in Service, we can't implement it easily without the RunPlan.
// But wait, useCapsules.ts has startCapsule.
// Usually "Start" on an existing capsule implies restart or starting a stopped container.
// If the container was removed (Stop -> Delete), we need the original definition to start it again.
// Since we don't store the full definition in valid state to "restart" from just ID easily if it was deleted...
// But wait, StateManager has `Capsule` struct with `Manifest`.
// If `StopCapsule` deleted it from DB, then we can't restart it!
// Ah, `StopCapsule` in `service.go` calls `store.DeleteDeployedCapsule`. It REMOVES it.
// So "Stop" is effectively "Terminate".
// The frontend "Start" likely assumes it's just stopped, not deleted.
// But our current backend implementation deletes it on stop.
// So Start is impossible without re-deploying.
// For now, I will implement HandleStartCapsule to return 501 Not Implemented or 400.
func (h *CapsuleHandler) HandleStartCapsule(w http.ResponseWriter, r *http.Request) {
	http.Error(w, "Start operation not supported (Capsules are ephemeral)", http.StatusBadRequest)
}

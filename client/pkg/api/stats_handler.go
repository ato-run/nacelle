package api

import (
	"encoding/json"
	"log"
	"net/http"
	"runtime"

	"github.com/onescluster/coordinator/pkg/db"
)

// StatsHandler handles system stats for Admin Dashboard
type StatsHandler struct {
	StateManager *db.StateManager
}

// NewStatsHandler creates a new stats handler
func NewStatsHandler(stateManager *db.StateManager) *StatsHandler {
	return &StatsHandler{
		StateManager: stateManager,
	}
}

// StatsResponse represents system statistics for the dashboard
type StatsResponse struct {
	ActiveCapsules    int     `json:"activeCapsules"`
	TotalCapsules     int     `json:"totalCapsules"`
	CPUUsage          float64 `json:"cpuUsage"`
	MemoryUsage       string  `json:"memoryUsage"`
	MemoryUsageBytes  uint64  `json:"memoryUsageBytes"`
	RequestsPerMinute int     `json:"requestsPerMinute"`
	ADEPVersion       string  `json:"adepVersion"`
	EngineStatus      string  `json:"engineStatus"`
}

// HandleGetStats returns system statistics
// GET /api/v1/stats
func (h *StatsHandler) HandleGetStats(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Get capsules from StateManager
	capsules := h.StateManager.GetAllCapsules()
	activeCapsules := 0
	for _, c := range capsules {
		if c.Status == "running" || c.Status == "RUNNING" {
			activeCapsules++
		}
	}

	// Get memory stats
	var memStats runtime.MemStats
	runtime.ReadMemStats(&memStats)

	response := StatsResponse{
		ActiveCapsules:    activeCapsules,
		TotalCapsules:     len(capsules),
		CPUUsage:          0, // TODO: Implement actual CPU monitoring
		MemoryUsage:       formatBytes(memStats.Alloc),
		MemoryUsageBytes:  memStats.Alloc,
		RequestsPerMinute: 0, // TODO: Implement request counter
		ADEPVersion:       "1.0.0",
		EngineStatus:      "connected",
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	if err := json.NewEncoder(w).Encode(response); err != nil {
		log.Printf("Failed to encode stats: %v", err)
	}
}

// formatBytes converts bytes to human-readable format
func formatBytes(b uint64) string {
	const unit = 1024
	if b < unit {
		return string(rune(b)) + " B"
	}
	div, exp := uint64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	sizes := []string{"KB", "MB", "GB", "TB"}
	return string(rune(b/div)) + " " + sizes[exp]
}

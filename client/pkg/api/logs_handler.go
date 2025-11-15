package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/gorilla/websocket"
)

// LogsHandler handles log streaming requests
type LogsHandler struct {
	upgrader websocket.Upgrader
	// logCollector will be used to get logs from engines
	// For now, we'll implement a simple version
}

// NewLogsHandler creates a new logs handler
func NewLogsHandler() *LogsHandler {
	return &LogsHandler{
		upgrader: websocket.Upgrader{
			ReadBufferSize:  1024,
			WriteBufferSize: 1024,
			CheckOrigin: func(r *http.Request) bool {
				// TODO: Implement proper origin checking in production
				return true
			},
		},
	}
}

// LogEntry represents a single log line with metadata
type LogEntry struct {
	Timestamp int64  `json:"timestamp"`
	Stream    string `json:"stream"` // "stdout" or "stderr"
	Line      string `json:"line"`
}

// StreamLogsHandler handles WebSocket connections for streaming logs
// GET /api/v1/capsules/:id/logs
func (h *LogsHandler) StreamLogsHandler(w http.ResponseWriter, r *http.Request) {
	// Extract capsule ID from URL path
	// Expected format: /api/v1/capsules/{id}/logs
	capsuleID := extractCapsuleIDFromPath(r.URL.Path)
	if capsuleID == "" {
		http.Error(w, "Invalid capsule ID", http.StatusBadRequest)
		return
	}

	// Parse query parameters
	follow := r.URL.Query().Get("follow") == "true"
	tailStr := r.URL.Query().Get("tail")
	tail := 100 // default
	if tailStr != "" {
		if t, err := strconv.Atoi(tailStr); err == nil && t > 0 {
			tail = t
		}
	}

	// Upgrade HTTP connection to WebSocket
	conn, err := h.upgrader.Upgrade(w, r, nil)
	if err != nil {
		http.Error(w, fmt.Sprintf("Failed to upgrade connection: %v", err), http.StatusInternalServerError)
		return
	}
	defer conn.Close()

	// Create context with cancellation
	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()

	// Start streaming logs
	if err := h.streamLogs(ctx, conn, capsuleID, follow, tail); err != nil {
		// Log error but connection may already be closed
		_ = conn.WriteMessage(websocket.CloseMessage,
			websocket.FormatCloseMessage(websocket.CloseInternalServerErr, err.Error()))
	}
}

// streamLogs streams log entries to the WebSocket connection
func (h *LogsHandler) streamLogs(ctx context.Context, conn *websocket.Conn, capsuleID string, follow bool, tail int) error {
	// TODO: Get actual logs from engine via gRPC
	// For now, we'll send mock logs as a placeholder

	// Send historical logs (tail)
	historicalLogs := h.getHistoricalLogs(capsuleID, tail)
	for _, entry := range historicalLogs {
		data, err := json.Marshal(entry)
		if err != nil {
			return fmt.Errorf("failed to marshal log entry: %w", err)
		}

		if err := conn.WriteMessage(websocket.TextMessage, data); err != nil {
			return fmt.Errorf("failed to write log entry: %w", err)
		}
	}

	// If follow mode is enabled, stream new logs
	if follow {
		ticker := time.NewTicker(1 * time.Second)
		defer ticker.Stop()

		for {
			select {
			case <-ctx.Done():
				return nil
			case <-ticker.C:
				// TODO: Get new logs from engine
				// For now, send a heartbeat message
				entry := LogEntry{
					Timestamp: time.Now().Unix(),
					Stream:    "stdout",
					Line:      fmt.Sprintf("[%s] Container running...", time.Now().Format(time.RFC3339)),
				}

				data, err := json.Marshal(entry)
				if err != nil {
					return fmt.Errorf("failed to marshal log entry: %w", err)
				}

				if err := conn.WriteMessage(websocket.TextMessage, data); err != nil {
					return fmt.Errorf("failed to write log entry: %w", err)
				}
			}
		}
	}

	return nil
}

// getHistoricalLogs retrieves historical logs for a capsule
// TODO: Implement actual log retrieval from engine
func (h *LogsHandler) getHistoricalLogs(capsuleID string, tail int) []LogEntry {
	// Mock historical logs
	entries := make([]LogEntry, 0, tail)
	baseTime := time.Now().Add(-1 * time.Hour)

	for i := 0; i < tail && i < 5; i++ {
		entries = append(entries, LogEntry{
			Timestamp: baseTime.Add(time.Duration(i) * time.Minute).Unix(),
			Stream:    "stdout",
			Line:      fmt.Sprintf("Log line %d for capsule %s", i+1, capsuleID),
		})
	}

	return entries
}

// extractCapsuleIDFromPath extracts the capsule ID from the URL path
// Expected format: /api/v1/capsules/{id}/logs
func extractCapsuleIDFromPath(path string) string {
	// Simple path parsing - in production, use a proper router
	// Expected: /api/v1/capsules/{id}/logs
	// Remove /api/v1/capsules/ prefix and /logs suffix
	const prefix = "/api/v1/capsules/"
	const suffix = "/logs"

	if len(path) < len(prefix)+len(suffix) {
		return ""
	}

	if path[:len(prefix)] != prefix {
		return ""
	}

	remaining := path[len(prefix):]
	if len(remaining) < len(suffix) {
		return ""
	}

	if remaining[len(remaining)-len(suffix):] != suffix {
		return ""
	}

	return remaining[:len(remaining)-len(suffix)]
}

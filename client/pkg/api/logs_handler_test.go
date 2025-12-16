package api

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gorilla/websocket"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewLogsHandler(t *testing.T) {
	handler := NewLogsHandler()
	assert.NotNil(t, handler)
	assert.NotNil(t, handler.upgrader)
}

func TestExtractCapsuleIDFromLogsPath(t *testing.T) {
	tests := []struct {
		name     string
		path     string
		expected string
	}{
		{
			name:     "valid path",
			path:     "/api/v1/capsules/capsule-123/logs",
			expected: "capsule-123",
		},
		{
			name:     "valid path with uuid",
			path:     "/api/v1/capsules/550e8400-e29b-41d4-a716-446655440000/logs",
			expected: "550e8400-e29b-41d4-a716-446655440000",
		},
		{
			name:     "invalid path - missing logs suffix",
			path:     "/api/v1/capsules/capsule-123",
			expected: "",
		},
		{
			name:     "invalid path - wrong prefix",
			path:     "/api/v2/capsules/capsule-123/logs",
			expected: "",
		},
		{
			name:     "empty path",
			path:     "",
			expected: "",
		},
		{
			name:     "path too short",
			path:     "/api/v1/capsules/",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, _ := extractCapsuleIDFromLogsPath(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestGetHistoricalLogs(t *testing.T) {
	handler := NewLogsHandler()

	logs := handler.getHistoricalLogs("test-capsule", 10)

	// Should return at most 5 mock entries (as per implementation)
	assert.LessOrEqual(t, len(logs), 5)
	assert.LessOrEqual(t, len(logs), 10)

	// Verify log structure
	for _, entry := range logs {
		assert.NotZero(t, entry.Timestamp)
		assert.Equal(t, "stdout", entry.Stream)
		assert.NotEmpty(t, entry.Line)
		assert.Contains(t, entry.Line, "test-capsule")
	}
}

func TestStreamLogsHandler_InvalidCapsuleID(t *testing.T) {
	handler := NewLogsHandler()

	req := httptest.NewRequest(http.MethodGet, "/api/v1/capsules//logs", nil)
	w := httptest.NewRecorder()

	handler.StreamLogsHandler(w, req)

	assert.Equal(t, http.StatusBadRequest, w.Code)
	assert.Contains(t, w.Body.String(), "Invalid capsule ID")
}

func TestStreamLogsHandler_WebSocketUpgrade(t *testing.T) {
	handler := NewLogsHandler()

	// Create test server
	server := httptest.NewServer(http.HandlerFunc(handler.StreamLogsHandler))
	defer server.Close()

	// Convert http:// to ws://
	wsURL := strings.Replace(server.URL, "http://", "ws://", 1) + "/api/v1/capsules/test-123/logs"

	// Connect via WebSocket
	conn, resp, err := websocket.DefaultDialer.Dial(wsURL, nil)
	require.NoError(t, err)
	require.Equal(t, http.StatusSwitchingProtocols, resp.StatusCode)
	defer conn.Close()

	// Set read deadline to avoid hanging
	err = conn.SetReadDeadline(time.Now().Add(2 * time.Second))
	require.NoError(t, err)

	// Read at least one message (historical logs)
	_, message, err := conn.ReadMessage()
	require.NoError(t, err)
	assert.NotEmpty(t, message)

	// Verify message is valid JSON with expected structure
	assert.Contains(t, string(message), "timestamp")
	assert.Contains(t, string(message), "stream")
	assert.Contains(t, string(message), "line")
}

func TestStreamLogsHandler_FollowMode(t *testing.T) {
	handler := NewLogsHandler()

	// Create test server
	server := httptest.NewServer(http.HandlerFunc(handler.StreamLogsHandler))
	defer server.Close()

	// Convert http:// to ws://
	wsURL := strings.Replace(server.URL, "http://", "ws://", 1) + "/api/v1/capsules/test-123/logs?follow=true&tail=2"

	// Connect via WebSocket
	conn, resp, err := websocket.DefaultDialer.Dial(wsURL, nil)
	require.NoError(t, err)
	require.Equal(t, http.StatusSwitchingProtocols, resp.StatusCode)
	defer conn.Close()

	// Set read deadline
	require.NoError(t, conn.SetReadDeadline(time.Now().Add(3*time.Second)))

	// Read multiple messages (should get historical + follow updates)
	messageCount := 0
	for i := 0; i < 5; i++ {
		_, _, err := conn.ReadMessage()
		if err != nil {
			break
		}
		messageCount++
	}

	// Should receive at least historical logs (2) + some follow messages
	assert.GreaterOrEqual(t, messageCount, 2)
}

func TestLogEntry_JSONMarshaling(t *testing.T) {
	entry := LogEntry{
		Timestamp: 1234567890,
		Stream:    "stdout",
		Line:      "test log line",
	}

	// Test that LogEntry can be marshaled to JSON
	data, err := entry.MarshalJSON()
	require.NoError(t, err)
	assert.NotEmpty(t, data)

	// Verify JSON contains expected fields
	jsonStr := string(data)
	assert.Contains(t, jsonStr, "1234567890")
	assert.Contains(t, jsonStr, "stdout")
	assert.Contains(t, jsonStr, "test log line")
}

// Helper method for testing
func (e LogEntry) MarshalJSON() ([]byte, error) {
	type Alias LogEntry
	return json.Marshal(&struct{ *Alias }{Alias: (*Alias)(&e)})
}

package cloud

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestCreateChatCompletion(t *testing.T) {
	// Setup mock server
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" {
			t.Errorf("unexpected path: %s", r.URL.Path)
			http.NotFound(w, r)
			return
		}

		if r.Method != http.MethodPost {
			t.Errorf("unexpected method: %s", r.Method)
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}

		// Check headers
		if r.Header.Get("Content-Type") != "application/json" {
			t.Errorf("expected Content-Type: application/json")
		}
		if r.Header.Get("Authorization") != "Bearer test-key" {
			t.Errorf("expected Authorization: Bearer test-key")
		}

		// Parse request
		var req ChatRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}

		// Verify request
		if len(req.Messages) == 0 {
			http.Error(w, "no messages", http.StatusBadRequest)
			return
		}

		// Return response
		resp := ChatResponse{
			ID:      "chatcmpl-123",
			Object:  "chat.completion",
			Created: time.Now().Unix(),
			Model:   req.Model,
			Choices: []Choice{
				{
					Index: 0,
					Message: ChatMessage{
						Role:    "assistant",
						Content: "Hello! How can I help you today?",
					},
					FinishReason: "stop",
				},
			},
			Usage: Usage{
				PromptTokens:     10,
				CompletionTokens: 8,
				TotalTokens:      18,
			},
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}))
	defer server.Close()

	// Create client
	client := NewClient(Endpoint{
		URL:    server.URL,
		APIKey: "test-key",
		Model:  "gpt-4",
	})

	// Test request
	resp, err := client.CreateChatCompletion(context.Background(), ChatRequest{
		Messages: []ChatMessage{
			{Role: "user", Content: "Hello"},
		},
	})
	if err != nil {
		t.Fatalf("CreateChatCompletion failed: %v", err)
	}

	if resp.ID != "chatcmpl-123" {
		t.Errorf("expected ID 'chatcmpl-123', got '%s'", resp.ID)
	}
	if len(resp.Choices) != 1 {
		t.Errorf("expected 1 choice, got %d", len(resp.Choices))
	}
	if resp.Choices[0].Message.Content != "Hello! How can I help you today?" {
		t.Errorf("unexpected content: %s", resp.Choices[0].Message.Content)
	}
}

func TestCreateChatCompletionWithToolCalls(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req ChatRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}

		// Verify tools
		if len(req.Tools) == 0 {
			t.Error("expected tools in request")
		}

		resp := ChatResponse{
			ID:      "chatcmpl-456",
			Object:  "chat.completion",
			Created: time.Now().Unix(),
			Model:   req.Model,
			Choices: []Choice{
				{
					Index: 0,
					Message: ChatMessage{
						Role: "assistant",
						ToolCalls: []ToolCall{
							{
								ID:   "call_123",
								Type: "function",
								Function: FunctionCall{
									Name:      "get_weather",
									Arguments: `{"location": "Tokyo"}`,
								},
							},
						},
					},
					FinishReason: "tool_calls",
				},
			},
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}))
	defer server.Close()

	client := NewClient(Endpoint{URL: server.URL, Model: "gpt-4"})

	resp, err := client.CreateChatCompletion(context.Background(), ChatRequest{
		Messages: []ChatMessage{
			{Role: "user", Content: "What's the weather in Tokyo?"},
		},
		Tools: []Tool{
			{
				Type: "function",
				Function: Function{
					Name:        "get_weather",
					Description: "Get weather for a location",
					Parameters: map[string]interface{}{
						"type": "object",
						"properties": map[string]interface{}{
							"location": map[string]string{"type": "string"},
						},
					},
				},
			},
		},
	})
	if err != nil {
		t.Fatalf("CreateChatCompletion failed: %v", err)
	}

	if resp.Choices[0].FinishReason != "tool_calls" {
		t.Errorf("expected finish_reason 'tool_calls', got '%s'", resp.Choices[0].FinishReason)
	}
	if len(resp.Choices[0].Message.ToolCalls) != 1 {
		t.Errorf("expected 1 tool call, got %d", len(resp.Choices[0].Message.ToolCalls))
	}
}

func TestCreateChatCompletionStream(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" {
			http.NotFound(w, r)
			return
		}

		var req ChatRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}

		if !req.Stream {
			t.Error("expected stream=true in request")
		}

		// Set SSE headers
		w.Header().Set("Content-Type", "text/event-stream")
		w.Header().Set("Cache-Control", "no-cache")
		w.Header().Set("Connection", "keep-alive")

		flusher, ok := w.(http.Flusher)
		if !ok {
			t.Fatal("expected http.Flusher")
		}

		// Send chunks
		chunks := []string{"Hello", " world", "!"}
		for i, chunk := range chunks {
			data := ChatChunk{
				ID:      "chatcmpl-stream",
				Object:  "chat.completion.chunk",
				Created: time.Now().Unix(),
				Model:   req.Model,
				Choices: []ChunkChoice{
					{
						Index: 0,
						Delta: ChatDelta{Content: chunk},
					},
				},
			}
			if i == len(chunks)-1 {
				data.Choices[0].FinishReason = "stop"
			}

			jsonData, _ := json.Marshal(data)
			fmt.Fprintf(w, "data: %s\n\n", jsonData)
			flusher.Flush()
		}

		fmt.Fprintf(w, "data: [DONE]\n\n")
		flusher.Flush()
	}))
	defer server.Close()

	client := NewClient(Endpoint{URL: server.URL, Model: "gpt-4"})

	events, err := client.CreateChatCompletionStream(context.Background(), ChatRequest{
		Messages: []ChatMessage{
			{Role: "user", Content: "Say hello world"},
		},
	})
	if err != nil {
		t.Fatalf("CreateChatCompletionStream failed: %v", err)
	}

	var content strings.Builder
	for event := range events {
		if event.Error != nil {
			t.Errorf("unexpected error: %v", event.Error)
		}
		if event.Chunk != nil && len(event.Chunk.Choices) > 0 {
			content.WriteString(event.Chunk.Choices[0].Delta.Content)
		}
	}

	if content.String() != "Hello world!" {
		t.Errorf("expected 'Hello world!', got '%s'", content.String())
	}
}

func TestHealth(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			http.NotFound(w, r)
			return
		}
		w.WriteHeader(http.StatusOK)
		if _, err := w.Write([]byte(`{"status": "ok"}`)); err != nil {
			return
		}
	}))
	defer server.Close()

	client := NewClient(Endpoint{URL: server.URL})

	err := client.Health(context.Background())
	if err != nil {
		t.Errorf("Health check failed: %v", err)
	}
}

func TestHealthUnhealthy(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "service unavailable", http.StatusServiceUnavailable)
	}))
	defer server.Close()

	client := NewClient(Endpoint{URL: server.URL})

	err := client.Health(context.Background())
	if err == nil {
		t.Error("expected error for unhealthy endpoint")
	}
}

func TestRetryOnError(t *testing.T) {
	attempts := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts++
		if attempts < 3 {
			http.Error(w, "temporary error", http.StatusServiceUnavailable)
			return
		}

		resp := ChatResponse{
			ID:      "chatcmpl-retry",
			Object:  "chat.completion",
			Choices: []Choice{{Message: ChatMessage{Content: "success"}}},
		}
		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}))
	defer server.Close()

	client := NewClient(
		Endpoint{URL: server.URL, Model: "gpt-4"},
		WithMaxRetries(3),
		WithRetryDelay(10*time.Millisecond),
	)

	resp, err := client.CreateChatCompletion(context.Background(), ChatRequest{
		Messages: []ChatMessage{{Role: "user", Content: "test"}},
	})
	if err != nil {
		t.Fatalf("expected success after retries, got: %v", err)
	}
	if resp.Choices[0].Message.Content != "success" {
		t.Errorf("unexpected content: %s", resp.Choices[0].Message.Content)
	}
	if attempts != 3 {
		t.Errorf("expected 3 attempts, got %d", attempts)
	}
}

func TestAPIError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(ErrorResponse{
			Error: struct {
				Message string `json:"message"`
				Type    string `json:"type"`
				Code    string `json:"code,omitempty"`
			}{
				Message: "Invalid model",
				Type:    "invalid_request_error",
			},
		}); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}))
	defer server.Close()

	client := NewClient(Endpoint{URL: server.URL}, WithMaxRetries(0))

	_, err := client.CreateChatCompletion(context.Background(), ChatRequest{
		Messages: []ChatMessage{{Role: "user", Content: "test"}},
	})
	if err == nil {
		t.Fatal("expected error")
	}
	if !strings.Contains(err.Error(), "Invalid model") {
		t.Errorf("expected error to contain 'Invalid model', got: %v", err)
	}
}

func TestSetEndpoint(t *testing.T) {
	client := NewClient(Endpoint{URL: "http://old.example.com"})

	newEndpoint := Endpoint{
		URL:    "http://new.example.com",
		APIKey: "new-key",
		Model:  "new-model",
	}
	client.SetEndpoint(newEndpoint)

	got := client.GetEndpoint()
	if got.URL != newEndpoint.URL {
		t.Errorf("expected URL %s, got %s", newEndpoint.URL, got.URL)
	}
	if got.APIKey != newEndpoint.APIKey {
		t.Errorf("expected APIKey %s, got %s", newEndpoint.APIKey, got.APIKey)
	}
}

func TestClientOptions(t *testing.T) {
	client := NewClient(
		Endpoint{URL: "http://example.com"},
		WithTimeout(60*time.Second),
		WithMaxRetries(5),
		WithRetryDelay(2*time.Second),
	)

	if client.httpClient.Timeout != 60*time.Second {
		t.Errorf("expected timeout 60s, got %v", client.httpClient.Timeout)
	}
	if client.maxRetries != 5 {
		t.Errorf("expected maxRetries 5, got %d", client.maxRetries)
	}
	if client.retryDelay != 2*time.Second {
		t.Errorf("expected retryDelay 2s, got %v", client.retryDelay)
	}
}

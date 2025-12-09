// Package cloud provides a client for cloud-based Capsule endpoints.
package cloud

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"
)

const (
	// DefaultTimeout is the default HTTP client timeout.
	DefaultTimeout = 120 * time.Second

	// DefaultMaxRetries is the default number of retries for failed requests.
	DefaultMaxRetries = 3

	// DefaultRetryDelay is the initial delay between retries.
	DefaultRetryDelay = 1 * time.Second
)

// Client provides access to cloud inference endpoints.
type Client interface {
	// CreateChatCompletion sends a chat completion request.
	CreateChatCompletion(ctx context.Context, req ChatRequest) (*ChatResponse, error)

	// CreateChatCompletionStream sends a streaming chat completion request.
	CreateChatCompletionStream(ctx context.Context, req ChatRequest) (<-chan StreamEvent, error)

	// Health checks if the endpoint is healthy.
	Health(ctx context.Context) error

	// SetEndpoint sets the active endpoint.
	SetEndpoint(endpoint Endpoint)

	// GetEndpoint returns the current endpoint.
	GetEndpoint() Endpoint
}

// StreamEvent represents an event from a streaming response.
type StreamEvent struct {
	Chunk *ChatChunk
	Error error
	Done  bool
}

// HTTPClient is the production implementation of Client.
type HTTPClient struct {
	endpoint   Endpoint
	httpClient *http.Client
	mu         sync.RWMutex

	maxRetries int
	retryDelay time.Duration
}

// ClientOption configures the HTTP client.
type ClientOption func(*HTTPClient)

// WithTimeout sets the HTTP client timeout.
func WithTimeout(d time.Duration) ClientOption {
	return func(c *HTTPClient) {
		c.httpClient.Timeout = d
	}
}

// WithMaxRetries sets the maximum number of retries.
func WithMaxRetries(n int) ClientOption {
	return func(c *HTTPClient) {
		c.maxRetries = n
	}
}

// WithRetryDelay sets the initial retry delay.
func WithRetryDelay(d time.Duration) ClientOption {
	return func(c *HTTPClient) {
		c.retryDelay = d
	}
}

// NewClient creates a new cloud client.
func NewClient(endpoint Endpoint, opts ...ClientOption) *HTTPClient {
	c := &HTTPClient{
		endpoint: endpoint,
		httpClient: &http.Client{
			Timeout: DefaultTimeout,
		},
		maxRetries: DefaultMaxRetries,
		retryDelay: DefaultRetryDelay,
	}

	for _, opt := range opts {
		opt(c)
	}

	return c
}

// SetEndpoint sets the active endpoint.
func (c *HTTPClient) SetEndpoint(endpoint Endpoint) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.endpoint = endpoint
}

// GetEndpoint returns the current endpoint.
func (c *HTTPClient) GetEndpoint() Endpoint {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.endpoint
}

// Health checks if the endpoint is healthy.
func (c *HTTPClient) Health(ctx context.Context) error {
	endpoint := c.GetEndpoint()
	healthURL := strings.TrimSuffix(endpoint.URL, "/") + "/health"

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, healthURL, nil)
	if err != nil {
		return fmt.Errorf("creating health request: %w", err)
	}

	if endpoint.APIKey != "" {
		req.Header.Set("Authorization", "Bearer "+endpoint.APIKey)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("health check failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("health check returned status %d: %s", resp.StatusCode, string(body))
	}

	return nil
}

// CreateChatCompletion sends a chat completion request.
func (c *HTTPClient) CreateChatCompletion(ctx context.Context, req ChatRequest) (*ChatResponse, error) {
	endpoint := c.GetEndpoint()

	// Set model if not specified
	if req.Model == "" {
		req.Model = endpoint.Model
	}

	// Set max tokens if not specified
	if req.MaxTokens == 0 && endpoint.MaxTokens > 0 {
		req.MaxTokens = endpoint.MaxTokens
	}

	// Ensure non-streaming
	req.Stream = false

	// Marshal request
	body, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("marshaling request: %w", err)
	}

	// Build URL
	url := strings.TrimSuffix(endpoint.URL, "/") + "/v1/chat/completions"

	// Execute with retries
	var lastErr error
	for attempt := 0; attempt <= c.maxRetries; attempt++ {
		if attempt > 0 {
			delay := c.retryDelay * time.Duration(1<<(attempt-1)) // Exponential backoff
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(delay):
			}
		}

		resp, err := c.doRequest(ctx, url, body)
		if err != nil {
			lastErr = err
			continue
		}

		return resp, nil
	}

	return nil, fmt.Errorf("all retries failed: %w", lastErr)
}

func (c *HTTPClient) doRequest(ctx context.Context, url string, body []byte) (*ChatResponse, error) {
	endpoint := c.GetEndpoint()

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	req.Header.Set("Content-Type", "application/json")
	if endpoint.APIKey != "" {
		req.Header.Set("Authorization", "Bearer "+endpoint.APIKey)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("executing request: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("reading response: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		var errResp ErrorResponse
		if json.Unmarshal(respBody, &errResp) == nil && errResp.Error.Message != "" {
			return nil, fmt.Errorf("API error: %s (type: %s)", errResp.Error.Message, errResp.Error.Type)
		}
		return nil, fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(respBody))
	}

	var chatResp ChatResponse
	if err := json.Unmarshal(respBody, &chatResp); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return &chatResp, nil
}

// CreateChatCompletionStream sends a streaming chat completion request.
func (c *HTTPClient) CreateChatCompletionStream(ctx context.Context, req ChatRequest) (<-chan StreamEvent, error) {
	endpoint := c.GetEndpoint()

	// Set model if not specified
	if req.Model == "" {
		req.Model = endpoint.Model
	}

	// Enable streaming
	req.Stream = true

	// Marshal request
	body, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("marshaling request: %w", err)
	}

	// Build URL
	url := strings.TrimSuffix(endpoint.URL, "/") + "/v1/chat/completions"

	// Create request
	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("Accept", "text/event-stream")
	if endpoint.APIKey != "" {
		httpReq.Header.Set("Authorization", "Bearer "+endpoint.APIKey)
	}

	// Execute request
	resp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("executing request: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		defer resp.Body.Close()
		respBody, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(respBody))
	}

	// Create channel for streaming events
	events := make(chan StreamEvent)

	go func() {
		defer close(events)
		defer resp.Body.Close()

		reader := bufio.NewReader(resp.Body)

		for {
			select {
			case <-ctx.Done():
				events <- StreamEvent{Error: ctx.Err(), Done: true}
				return
			default:
			}

			line, err := reader.ReadBytes('\n')
			if err != nil {
				if err == io.EOF {
					events <- StreamEvent{Done: true}
					return
				}
				events <- StreamEvent{Error: err, Done: true}
				return
			}

			// Parse SSE line
			lineStr := strings.TrimSpace(string(line))
			if lineStr == "" {
				continue
			}

			// Skip comments
			if strings.HasPrefix(lineStr, ":") {
				continue
			}

			// Parse data line
			if !strings.HasPrefix(lineStr, "data: ") {
				continue
			}

			data := strings.TrimPrefix(lineStr, "data: ")

			// Check for stream end
			if data == "[DONE]" {
				events <- StreamEvent{Done: true}
				return
			}

			// Parse chunk
			var chunk ChatChunk
			if err := json.Unmarshal([]byte(data), &chunk); err != nil {
				events <- StreamEvent{Error: fmt.Errorf("parsing chunk: %w", err)}
				continue
			}

			events <- StreamEvent{Chunk: &chunk}
		}
	}()

	return events, nil
}

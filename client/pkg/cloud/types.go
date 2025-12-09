// Package cloud provides a client for cloud-based Capsule endpoints.
// It implements an OpenAI-compatible API client for communicating with
// vLLM and other inference services running in the cloud.
package cloud

import "time"

// Endpoint represents a cloud inference endpoint.
type Endpoint struct {
	// URL is the base URL of the endpoint (e.g., "https://api.runpod.io/v2/xxx")
	URL string `json:"url"`

	// APIKey is the authentication key for the endpoint
	APIKey string `json:"api_key,omitempty"`

	// Model is the model identifier served by this endpoint
	Model string `json:"model"`

	// MaxTokens is the default max tokens for requests
	MaxTokens int `json:"max_tokens,omitempty"`

	// Provider identifies the cloud provider (runpod, vast, lambda)
	Provider string `json:"provider,omitempty"`

	// Region is the geographic region of the endpoint
	Region string `json:"region,omitempty"`

	// Healthy indicates if the endpoint passed health check
	Healthy bool `json:"healthy"`

	// LastChecked is when the endpoint was last health-checked
	LastChecked time.Time `json:"last_checked,omitempty"`

	// Latency is the measured latency to this endpoint
	Latency time.Duration `json:"latency,omitempty"`
}

// ChatRequest represents an OpenAI-compatible chat completion request.
type ChatRequest struct {
	Model       string        `json:"model"`
	Messages    []ChatMessage `json:"messages"`
	MaxTokens   int           `json:"max_tokens,omitempty"`
	Temperature float64       `json:"temperature,omitempty"`
	TopP        float64       `json:"top_p,omitempty"`
	Stream      bool          `json:"stream,omitempty"`
	Stop        []string      `json:"stop,omitempty"`
	User        string        `json:"user,omitempty"`

	// Function calling
	Tools      []Tool `json:"tools,omitempty"`
	ToolChoice any    `json:"tool_choice,omitempty"`
}

// ChatMessage represents a single message in a conversation.
type ChatMessage struct {
	Role       string     `json:"role"` // system, user, assistant, tool
	Content    string     `json:"content,omitempty"`
	Name       string     `json:"name,omitempty"`
	ToolCalls  []ToolCall `json:"tool_calls,omitempty"`
	ToolCallID string     `json:"tool_call_id,omitempty"`
}

// Tool represents a function that can be called by the model.
type Tool struct {
	Type     string   `json:"type"` // "function"
	Function Function `json:"function"`
}

// Function describes a callable function.
type Function struct {
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
	Parameters  any    `json:"parameters,omitempty"` // JSON Schema
}

// ToolCall represents a function call made by the model.
type ToolCall struct {
	ID       string       `json:"id"`
	Type     string       `json:"type"` // "function"
	Function FunctionCall `json:"function"`
}

// FunctionCall contains the function name and arguments.
type FunctionCall struct {
	Name      string `json:"name"`
	Arguments string `json:"arguments"` // JSON string
}

// ChatResponse represents an OpenAI-compatible chat completion response.
type ChatResponse struct {
	ID      string   `json:"id"`
	Object  string   `json:"object"` // "chat.completion"
	Created int64    `json:"created"`
	Model   string   `json:"model"`
	Choices []Choice `json:"choices"`
	Usage   Usage    `json:"usage"`
}

// Choice represents a single completion choice.
type Choice struct {
	Index        int         `json:"index"`
	Message      ChatMessage `json:"message"`
	FinishReason string      `json:"finish_reason"` // stop, length, tool_calls
}

// Usage contains token usage information.
type Usage struct {
	PromptTokens     int `json:"prompt_tokens"`
	CompletionTokens int `json:"completion_tokens"`
	TotalTokens      int `json:"total_tokens"`
}

// ChatChunk represents a streaming chunk of a chat completion.
type ChatChunk struct {
	ID      string        `json:"id"`
	Object  string        `json:"object"` // "chat.completion.chunk"
	Created int64         `json:"created"`
	Model   string        `json:"model"`
	Choices []ChunkChoice `json:"choices"`
}

// ChunkChoice represents a single choice in a streaming chunk.
type ChunkChoice struct {
	Index        int          `json:"index"`
	Delta        ChatDelta    `json:"delta"`
	FinishReason string       `json:"finish_reason,omitempty"`
}

// ChatDelta represents the incremental content in a streaming chunk.
type ChatDelta struct {
	Role      string     `json:"role,omitempty"`
	Content   string     `json:"content,omitempty"`
	ToolCalls []ToolCall `json:"tool_calls,omitempty"`
}

// ErrorResponse represents an API error response.
type ErrorResponse struct {
	Error struct {
		Message string `json:"message"`
		Type    string `json:"type"`
		Code    string `json:"code,omitempty"`
	} `json:"error"`
}

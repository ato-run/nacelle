package middleware

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestAuthMiddleware_Authenticate(t *testing.T) {
	tests := []struct {
		name           string
		enabled        bool
		apiKeys        []string
		requestAPIKey  string
		expectedStatus int
		expectedBody   string
	}{
		{
			name:           "Valid API key",
			enabled:        true,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "secret-key-123",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
		{
			name:           "Invalid API key",
			enabled:        true,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "wrong-key",
			expectedStatus: http.StatusUnauthorized,
			expectedBody:   "Invalid API key\n",
		},
		{
			name:           "Missing API key",
			enabled:        true,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "",
			expectedStatus: http.StatusUnauthorized,
			expectedBody:   "Missing X-API-Key header\n",
		},
		{
			name:           "Multiple valid keys - first key",
			enabled:        true,
			apiKeys:        []string{"key1", "key2", "key3"},
			requestAPIKey:  "key1",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
		{
			name:           "Multiple valid keys - last key",
			enabled:        true,
			apiKeys:        []string{"key1", "key2", "key3"},
			requestAPIKey:  "key3",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
		{
			name:           "Authentication disabled",
			enabled:        false,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
		{
			name:           "No API keys configured",
			enabled:        true,
			apiKeys:        []string{},
			requestAPIKey:  "any-key",
			expectedStatus: http.StatusInternalServerError,
			expectedBody:   "Authentication not configured\n",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create middleware
			auth := NewAuthMiddleware(AuthConfig{
				APIKeys: tt.apiKeys,
				Enabled: tt.enabled,
			})

			// Create test handler
			handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(http.StatusOK)
				w.Write([]byte("OK"))
			})

			// Wrap handler with auth middleware
			wrappedHandler := auth.Authenticate(handler)

			// Create test request
			req := httptest.NewRequest(http.MethodGet, "/test", nil)
			if tt.requestAPIKey != "" {
				req.Header.Set("X-API-Key", tt.requestAPIKey)
			}

			// Record response
			rec := httptest.NewRecorder()
			wrappedHandler(rec, req)

			// Assertions
			assert.Equal(t, tt.expectedStatus, rec.Code)
			assert.Equal(t, tt.expectedBody, rec.Body.String())
		})
	}
}

func TestAuthMiddleware_OptionalAuthenticate(t *testing.T) {
	tests := []struct {
		name           string
		enabled        bool
		apiKeys        []string
		requestAPIKey  string
		expectedStatus int
		expectedBody   string
	}{
		{
			name:           "Valid API key",
			enabled:        true,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "secret-key-123",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
		{
			name:           "Invalid API key",
			enabled:        true,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "wrong-key",
			expectedStatus: http.StatusUnauthorized,
			expectedBody:   "Invalid API key\n",
		},
		{
			name:           "No API key provided - should pass",
			enabled:        true,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
		{
			name:           "Authentication disabled",
			enabled:        false,
			apiKeys:        []string{"secret-key-123"},
			requestAPIKey:  "",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
		{
			name:           "No keys configured but key provided",
			enabled:        true,
			apiKeys:        []string{},
			requestAPIKey:  "any-key",
			expectedStatus: http.StatusInternalServerError,
			expectedBody:   "Authentication not configured\n",
		},
		{
			name:           "No keys configured, no key provided",
			enabled:        true,
			apiKeys:        []string{},
			requestAPIKey:  "",
			expectedStatus: http.StatusOK,
			expectedBody:   "OK",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create middleware
			auth := NewAuthMiddleware(AuthConfig{
				APIKeys: tt.apiKeys,
				Enabled: tt.enabled,
			})

			// Create test handler
			handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(http.StatusOK)
				w.Write([]byte("OK"))
			})

			// Wrap handler with optional auth middleware
			wrappedHandler := auth.OptionalAuthenticate(handler)

			// Create test request
			req := httptest.NewRequest(http.MethodGet, "/test", nil)
			if tt.requestAPIKey != "" {
				req.Header.Set("X-API-Key", tt.requestAPIKey)
			}

			// Record response
			rec := httptest.NewRecorder()
			wrappedHandler(rec, req)

			// Assertions
			assert.Equal(t, tt.expectedStatus, rec.Code)
			assert.Equal(t, tt.expectedBody, rec.Body.String())
		})
	}
}

func TestParseAPIKeysFromEnv(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected []string
	}{
		{
			name:     "Single key",
			input:    "key1",
			expected: []string{"key1"},
		},
		{
			name:     "Multiple keys",
			input:    "key1,key2,key3",
			expected: []string{"key1", "key2", "key3"},
		},
		{
			name:     "Keys with spaces",
			input:    "  key1  ,  key2  ,  key3  ",
			expected: []string{"key1", "key2", "key3"},
		},
		{
			name:     "Empty string",
			input:    "",
			expected: []string{},
		},
		{
			name:     "Empty keys with commas",
			input:    "key1,,key2",
			expected: []string{"key1", "key2"},
		},
		{
			name:     "Only commas",
			input:    ",,,",
			expected: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := ParseAPIKeysFromEnv(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func BenchmarkAuthMiddleware_Authenticate(b *testing.B) {
	auth := NewAuthMiddleware(AuthConfig{
		APIKeys: []string{"key1", "key2", "key3"},
		Enabled: true,
	})

	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	})

	wrappedHandler := auth.Authenticate(handler)
	req := httptest.NewRequest(http.MethodGet, "/test", nil)
	req.Header.Set("X-API-Key", "key2")

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		rec := httptest.NewRecorder()
		wrappedHandler(rec, req)
	}
}

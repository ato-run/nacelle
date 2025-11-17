package middleware

import (
	"crypto/subtle"
	"net/http"
	"strings"
)

// AuthConfig holds authentication configuration
type AuthConfig struct {
	// APIKeys is a list of valid API keys for authentication
	APIKeys []string
	// Enabled controls whether authentication is enforced
	Enabled bool
}

// AuthMiddleware provides API Key authentication for HTTP handlers
type AuthMiddleware struct {
	config AuthConfig
}

// NewAuthMiddleware creates a new authentication middleware
func NewAuthMiddleware(config AuthConfig) *AuthMiddleware {
	return &AuthMiddleware{
		config: config,
	}
}

// Authenticate wraps an HTTP handler with API Key authentication
// It checks for the X-API-Key header and validates it against configured keys
func (m *AuthMiddleware) Authenticate(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		// If authentication is disabled, pass through
		if !m.config.Enabled {
			next(w, r)
			return
		}

		// If no API keys are configured, deny all requests
		if len(m.config.APIKeys) == 0 {
			http.Error(w, "Authentication not configured", http.StatusInternalServerError)
			return
		}

		// Extract API key from header
		apiKey := r.Header.Get("X-API-Key")
		if apiKey == "" {
			http.Error(w, "Missing X-API-Key header", http.StatusUnauthorized)
			return
		}

		// Validate API key using constant-time comparison to prevent timing attacks
		if !m.isValidAPIKey(apiKey) {
			http.Error(w, "Invalid API key", http.StatusUnauthorized)
			return
		}

		// Authentication successful, call next handler
		next(w, r)
	}
}

// isValidAPIKey checks if the provided API key matches any configured key
// Uses constant-time comparison to prevent timing attacks
func (m *AuthMiddleware) isValidAPIKey(providedKey string) bool {
	for _, validKey := range m.config.APIKeys {
		if subtle.ConstantTimeCompare([]byte(providedKey), []byte(validKey)) == 1 {
			return true
		}
	}
	return false
}

// OptionalAuthenticate wraps an HTTP handler with optional API Key authentication
// If X-API-Key header is present, it validates it; otherwise allows the request
func (m *AuthMiddleware) OptionalAuthenticate(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		// If authentication is disabled, pass through
		if !m.config.Enabled {
			next(w, r)
			return
		}

		// Extract API key from header
		apiKey := r.Header.Get("X-API-Key")

		// If API key is provided, validate it
		if apiKey != "" {
			if len(m.config.APIKeys) == 0 {
				http.Error(w, "Authentication not configured", http.StatusInternalServerError)
				return
			}

			if !m.isValidAPIKey(apiKey) {
				http.Error(w, "Invalid API key", http.StatusUnauthorized)
				return
			}
		}

		// Either no API key provided or valid API key provided
		next(w, r)
	}
}

// ParseAPIKeysFromEnv parses API keys from a comma-separated environment variable
func ParseAPIKeysFromEnv(envValue string) []string {
	if envValue == "" {
		return []string{}
	}

	keys := strings.Split(envValue, ",")
	result := make([]string, 0, len(keys))

	for _, key := range keys {
		trimmed := strings.TrimSpace(key)
		if trimmed != "" {
			result = append(result, trimmed)
		}
	}

	return result
}

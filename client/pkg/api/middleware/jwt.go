package middleware

import (
	"context"
	"net/http"
	"strings"

	"github.com/golang-jwt/jwt/v5"
	"github.com/onescluster/coordinator/pkg/errors"
)

// UserContextKey is the context key for authenticated user
type UserContextKey struct{}

// AuthenticatedUser represents the authenticated user from JWT
type AuthenticatedUser struct {
	ID    string
	Email string
	Tier  string
	Role  string
}

// JWTConfig holds JWT configuration
type JWTConfig struct {
	Secret    string
	Issuer    string
	Audience  string
	DevMode   bool
}

// JWTMiddleware validates JWT tokens for HTTP requests
type JWTMiddleware struct {
	config JWTConfig
}

// NewJWTMiddleware creates a new JWT middleware
func NewJWTMiddleware(config JWTConfig) *JWTMiddleware {
	return &JWTMiddleware{config: config}
}

// Handler wraps an http.Handler with JWT validation
func (m *JWTMiddleware) Handler(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Dev Mode Bypass
		if m.config.DevMode {
			// Inject dummy user
			user := &AuthenticatedUser{
				ID:    "dev-user-id",
				Email: "dev@example.com",
				Tier:  "studio", // Give full access in dev
				Role:  "admin",
			}
			ctx := context.WithValue(r.Context(), UserContextKey{}, user)
			next.ServeHTTP(w, r.WithContext(ctx))
			return
		}

		// Extract token from Authorization header
		authHeader := r.Header.Get("Authorization")
		if authHeader == "" {
			errors.WriteError(w, errors.NewUnauthorizedError("Missing authorization header"), r.Header.Get("X-Request-ID"))
			return
		}

		// Check Bearer prefix
		parts := strings.SplitN(authHeader, " ", 2)
		if len(parts) != 2 || !strings.EqualFold(parts[0], "bearer") {
			errors.WriteError(w, errors.NewUnauthorizedError("Invalid authorization header format"), r.Header.Get("X-Request-ID"))
			return
		}

		tokenString := parts[1]

		// Parse and validate token
		token, err := jwt.Parse(tokenString, func(token *jwt.Token) (interface{}, error) {
			// Validate signing method
			if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
				return nil, jwt.ErrSignatureInvalid
			}
			return []byte(m.config.Secret), nil
		})

		if err != nil {
			if err == jwt.ErrTokenExpired {
				errors.WriteError(w, errors.NewTokenExpiredError(), r.Header.Get("X-Request-ID"))
			} else {
				errors.WriteError(w, errors.NewUnauthorizedError("Invalid token"), r.Header.Get("X-Request-ID"))
			}
			return
		}

		if !token.Valid {
			errors.WriteError(w, errors.NewUnauthorizedError("Invalid token"), r.Header.Get("X-Request-ID"))
			return
		}

		// Extract claims
		claims, ok := token.Claims.(jwt.MapClaims)
		if !ok {
			errors.WriteError(w, errors.NewUnauthorizedError("Invalid token claims"), r.Header.Get("X-Request-ID"))
			return
		}

		// Validate issuer if configured
		if m.config.Issuer != "" {
			if iss, _ := claims["iss"].(string); iss != m.config.Issuer {
				errors.WriteError(w, errors.NewUnauthorizedError("Invalid token issuer"), r.Header.Get("X-Request-ID"))
				return
			}
		}

		// Build user from claims
		user := &AuthenticatedUser{
			ID:    getStringClaim(claims, "sub"),
			Email: getStringClaim(claims, "email"),
			Tier:  getStringClaim(claims, "tier"),
			Role:  getStringClaim(claims, "role"),
		}

		// Default tier if not set
		if user.Tier == "" {
			user.Tier = "free"
		}

		// Add user to context
		ctx := context.WithValue(r.Context(), UserContextKey{}, user)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

// GetUser extracts the authenticated user from context
func GetUser(ctx context.Context) (*AuthenticatedUser, bool) {
	user, ok := ctx.Value(UserContextKey{}).(*AuthenticatedUser)
	return user, ok
}

// RequireAuth is a helper that panics if user is not authenticated
func RequireAuth(ctx context.Context) *AuthenticatedUser {
	user, ok := GetUser(ctx)
	if !ok {
		panic("RequireAuth called without authenticated user in context")
	}
	return user
}

func getStringClaim(claims jwt.MapClaims, key string) string {
	if val, ok := claims[key].(string); ok {
		return val
	}
	return ""
}

package middleware

import (
	"context"
	"net/http"
	"strings"

	"github.com/golang-jwt/jwt/v5"
)

// OptionalHandler validates JWT if present, but doesn't require it
func (m *JWTMiddleware) OptionalHandler(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		authHeader := r.Header.Get("Authorization")
		
		// No token - continue without user
		if authHeader == "" {
			next.ServeHTTP(w, r)
			return
		}

		parts := strings.SplitN(authHeader, " ", 2)
		if len(parts) != 2 || !strings.EqualFold(parts[0], "bearer") {
			next.ServeHTTP(w, r)
			return
		}

		tokenString := parts[1]

		token, err := jwt.Parse(tokenString, func(token *jwt.Token) (interface{}, error) {
			if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
				return nil, jwt.ErrSignatureInvalid
			}
			return []byte(m.config.Secret), nil
		})

		if err != nil || !token.Valid {
			// Invalid token - continue without user
			next.ServeHTTP(w, r)
			return
		}

		claims, ok := token.Claims.(jwt.MapClaims)
		if !ok {
			next.ServeHTTP(w, r)
			return
		}

		user := &AuthenticatedUser{
			ID:    getStringClaim(claims, "sub"),
			Email: getStringClaim(claims, "email"),
			Tier:  getStringClaim(claims, "tier"),
			Role:  getStringClaim(claims, "role"),
		}

		if user.Tier == "" {
			user.Tier = "free"
		}

		ctx := context.WithValue(r.Context(), UserContextKey{}, user)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

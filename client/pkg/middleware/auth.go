package middleware

import (
	"context"
	"fmt"
	"strings"

	"github.com/golang-jwt/jwt/v5"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/metadata"
	"google.golang.org/grpc/status"
)

// AuthInterceptor handles JWT validation for gRPC requests
type AuthInterceptor struct {
	jwtSecret string
}

func NewAuthInterceptor(jwtSecret string) *AuthInterceptor {
	return &AuthInterceptor{
		jwtSecret: jwtSecret,
	}
}

// User represents the authenticated user
type User struct {
	ID    string
	Email string
}

type contextKey string

const userContextKey contextKey = "user"

// GetUser retrieves the user from the context
func GetUser(ctx context.Context) (*User, bool) {
	u, ok := ctx.Value(userContextKey).(*User)
	return u, ok
}

// Unary returns a server interceptor for unary RPCs
func (i *AuthInterceptor) Unary() grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		// Skip auth for machine registration and heartbeat
		// TODO: Implement separate Machine Auth (e.g. mTLS or PSK)
		if strings.HasSuffix(info.FullMethod, "RegisterMachine") ||
			strings.HasSuffix(info.FullMethod, "Heartbeat") {
			return handler(ctx, req)
		}

		user, err := i.authorize(ctx)
		if err != nil {
			return nil, err
		}

		// Add user to context
		newCtx := context.WithValue(ctx, userContextKey, user)
		return handler(newCtx, req)
	}
}

// authorize validates the token from metadata
func (i *AuthInterceptor) authorize(ctx context.Context) (*User, error) {
	md, ok := metadata.FromIncomingContext(ctx)
	if !ok {
		return nil, status.Errorf(codes.Unauthenticated, "metadata is not provided")
	}

	values := md["authorization"]
	if len(values) == 0 {
		return nil, status.Errorf(codes.Unauthenticated, "authorization token is not provided")
	}

	accessToken := values[0]
	if !strings.HasPrefix(accessToken, "Bearer ") {
		return nil, status.Errorf(codes.Unauthenticated, "authorization token must be a Bearer token")
	}

	tokenString := strings.TrimPrefix(accessToken, "Bearer ")

	token, err := jwt.Parse(tokenString, func(token *jwt.Token) (interface{}, error) {
		if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
			return nil, fmt.Errorf("unexpected signing method: %v", token.Header["alg"])
		}
		return []byte(i.jwtSecret), nil
	})

	if err != nil {
		return nil, status.Errorf(codes.Unauthenticated, "invalid token: %v", err)
	}

	if claims, ok := token.Claims.(jwt.MapClaims); ok && token.Valid {
		sub, ok := claims["sub"].(string)
		if !ok {
			return nil, status.Errorf(codes.Unauthenticated, "token does not contain subject")
		}

		email, _ := claims["email"].(string)

		return &User{
			ID:    sub,
			Email: email,
		}, nil
	}

	return nil, status.Errorf(codes.Unauthenticated, "invalid token claims")
}

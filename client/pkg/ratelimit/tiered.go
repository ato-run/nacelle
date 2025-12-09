package ratelimit

import (
	"context"
	"net/http"
)

// TierLimits defines rate limits per tier
type TierLimits struct {
	RequestsPerSecond float64
	Burst             int
}

// DefaultTierLimits defines default limits for each tier
var DefaultTierLimits = map[string]TierLimits{
	"free":     {RequestsPerSecond: 1, Burst: 5},
	"everyday": {RequestsPerSecond: 5, Burst: 10},
	"fast":     {RequestsPerSecond: 10, Burst: 20},
	"studio":   {RequestsPerSecond: 50, Burst: 100},
	"":         {RequestsPerSecond: 1, Burst: 5}, // fallback
}

// TieredRateLimiter applies different limits based on user tier
type TieredRateLimiter struct {
	limiters map[string]*RateLimiter
}

// NewTieredRateLimiter creates a new tiered rate limiter
func NewTieredRateLimiter(limits map[string]TierLimits) *TieredRateLimiter {
	if limits == nil {
		limits = DefaultTierLimits
	}

	trl := &TieredRateLimiter{
		limiters: make(map[string]*RateLimiter),
	}

	for tier, limit := range limits {
		trl.limiters[tier] = NewRateLimiter(limit.RequestsPerSecond, limit.Burst)
	}

	return trl
}

// userContextKey is the context key for user info
type userContextKey struct{}

// UserInfo contains user information for rate limiting
type UserInfo struct {
	ID   string
	Tier string
}

// UserFromContext extracts user info from context
func UserFromContext(ctx context.Context) (*UserInfo, bool) {
	user, ok := ctx.Value(userContextKey{}).(*UserInfo)
	return user, ok
}

// ContextWithUser adds user info to context
func ContextWithUser(ctx context.Context, user *UserInfo) context.Context {
	return context.WithValue(ctx, userContextKey{}, user)
}

// Allow checks if a request is allowed based on user tier
func (trl *TieredRateLimiter) Allow(tier, key string) bool {
	limiter, ok := trl.limiters[tier]
	if !ok {
		limiter = trl.limiters[""]
	}
	return limiter.Allow(key)
}

// Middleware creates HTTP middleware with tier-based limiting
func (trl *TieredRateLimiter) Middleware(getUserInfo func(*http.Request) *UserInfo) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var tier, key string

			if getUserInfo != nil {
				if user := getUserInfo(r); user != nil {
					tier = user.Tier
					key = user.ID
					// Add user to context for downstream handlers
					r = r.WithContext(ContextWithUser(r.Context(), user))
				}
			}

			// Fallback to IP if no user
			if key == "" {
				key = r.RemoteAddr
			}

			if !trl.Allow(tier, key) {
				w.Header().Set("Retry-After", "1")
				w.Header().Set("X-RateLimit-Tier", tier)
				http.Error(w, "Rate limit exceeded", http.StatusTooManyRequests)
				return
			}

			next.ServeHTTP(w, r)
		})
	}
}

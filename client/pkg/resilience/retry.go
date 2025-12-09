package resilience

import (
	"context"
	"math"
	"math/rand"
	"time"
)

// RetryConfig holds retry configuration
type RetryConfig struct {
	MaxRetries     int
	InitialBackoff time.Duration
	MaxBackoff     time.Duration
	Multiplier     float64
	Jitter         float64
}

// DefaultRetryConfig provides sensible defaults
var DefaultRetryConfig = RetryConfig{
	MaxRetries:     3,
	InitialBackoff: 100 * time.Millisecond,
	MaxBackoff:     10 * time.Second,
	Multiplier:     2.0,
	Jitter:         0.1,
}

// RetryableFunc is a function that can be retried
type RetryableFunc func() error

// IsRetryable determines if an error should be retried
type IsRetryable func(error) bool

// DefaultIsRetryable retries all errors
func DefaultIsRetryable(err error) bool {
	return err != nil
}

// Retry executes a function with exponential backoff
func Retry(ctx context.Context, config RetryConfig, fn RetryableFunc) error {
	return RetryWithCheck(ctx, config, fn, DefaultIsRetryable)
}

// RetryWithCheck executes with a custom retry check
func RetryWithCheck(ctx context.Context, config RetryConfig, fn RetryableFunc, isRetryable IsRetryable) error {
	var lastErr error

	for attempt := 0; attempt <= config.MaxRetries; attempt++ {
		// Check context cancellation
		if err := ctx.Err(); err != nil {
			return err
		}

		// Execute function
		lastErr = fn()
		if lastErr == nil {
			return nil
		}

		// Check if error is retryable
		if !isRetryable(lastErr) {
			return lastErr
		}

		// Don't sleep after last attempt
		if attempt == config.MaxRetries {
			break
		}

		// Calculate backoff with jitter
		backoff := calculateBackoff(attempt, config)

		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(backoff):
		}
	}

	return lastErr
}

func calculateBackoff(attempt int, config RetryConfig) time.Duration {
	backoff := float64(config.InitialBackoff) * math.Pow(config.Multiplier, float64(attempt))

	if backoff > float64(config.MaxBackoff) {
		backoff = float64(config.MaxBackoff)
	}

	// Add jitter: ±jitter%
	jitter := backoff * config.Jitter * (rand.Float64()*2 - 1)
	backoff += jitter

	if backoff < 0 {
		backoff = float64(config.InitialBackoff)
	}

	return time.Duration(backoff)
}

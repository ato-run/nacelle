package util

import (
	"time"

	"github.com/cenkalti/backoff/v4"
	"github.com/sony/gobreaker"
)

// NewCircuitBreaker creates a new CircuitBreaker with default settings
func NewCircuitBreaker(name string) *gobreaker.CircuitBreaker {
	settings := gobreaker.Settings{
		Name:        name,
		MaxRequests: 3,
		Interval:    5 * time.Second,
		Timeout:     10 * time.Second,
		ReadyToTrip: func(counts gobreaker.Counts) bool {
			failureRatio := float64(counts.TotalFailures) / float64(counts.Requests)
			return counts.Requests >= 3 && failureRatio >= 0.6
		},
	}
	return gobreaker.NewCircuitBreaker(settings)
}

// RetryWithBackoff executes an operation with exponential backoff
func RetryWithBackoff(operation func() error, maxRetries uint64) error {
	b := backoff.NewExponentialBackOff()
	b.MaxElapsedTime = 30 * time.Second

	if maxRetries > 0 {
		return backoff.Retry(operation, backoff.WithMaxRetries(b, maxRetries))
	}
	return backoff.Retry(operation, b)
}

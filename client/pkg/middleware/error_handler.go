package middleware

import (
	"net/http"

	"github.com/onescluster/coordinator/pkg/errors"
)

// HandlerFunc is a function that returns an error
type HandlerFunc func(w http.ResponseWriter, r *http.Request) error

// WrapHandler wraps a HandlerFunc with error handling
func WrapHandler(h HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := h(w, r); err != nil {
			// Get Request ID from header
			requestID := r.Header.Get("X-Request-ID")
			errors.HandleError(w, err, requestID)
		}
	}
}

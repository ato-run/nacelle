package errors

import (
	"encoding/json"
	"errors"
	"log"
	"net/http"
	"runtime/debug"
)

// ErrorResponse is the JSON response format
type ErrorResponse struct {
	Error struct {
		Code    ErrorCode         `json:"code"`
		Message string            `json:"message"`
		Details map[string]string `json:"details,omitempty"`
	} `json:"error"`
	RequestID string `json:"request_id,omitempty"`
}

// WriteError writes a structured error response
func WriteError(w http.ResponseWriter, err *AppError, requestID string) {
	response := ErrorResponse{
		RequestID: requestID,
	}
	response.Error.Code = err.Code
	response.Error.Message = err.Message
	response.Error.Details = err.Details

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(err.HTTPStatus)

	if encErr := json.NewEncoder(w).Encode(response); encErr != nil {
		log.Printf("[ERROR] Failed to encode error response: %v", encErr)
	}

	// Log server errors
	if err.HTTPStatus >= 500 {
		log.Printf("[ERROR] RequestID=%s Code=%s Message=%s Cause=%v",
			requestID, err.Code, err.Message, err.Cause)
	}
}

// HandleError converts any error to AppError and writes response
func HandleError(w http.ResponseWriter, err error, requestID string) {
	var appErr *AppError
	if errors.As(err, &appErr) {
		WriteError(w, appErr, requestID)
		return
	}

	// Wrap unknown errors
	internalErr := NewInternalError("An unexpected error occurred").WithCause(err)
	WriteError(w, internalErr, requestID)
}

// RecoveryMiddleware catches panics and returns 500
func RecoveryMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		defer func() {
			if rec := recover(); rec != nil {
				log.Printf("[PANIC] %v\n%s", rec, debug.Stack())

				requestID := r.Header.Get("X-Request-ID")
				err := NewInternalError("An unexpected error occurred")
				WriteError(w, err, requestID)
			}
		}()
		next.ServeHTTP(w, r)
	})
}

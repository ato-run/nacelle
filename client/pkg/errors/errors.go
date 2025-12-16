package errors

import (
	"fmt"
	"net/http"
)

// ErrorCode represents a unique error identifier
type ErrorCode string

// Error codes
const (
	// Authentication errors (401)
	ErrCodeUnauthorized ErrorCode = "UNAUTHORIZED"
	ErrCodeTokenExpired ErrorCode = "TOKEN_EXPIRED"
	ErrCodeInvalidToken ErrorCode = "INVALID_TOKEN"

	// Authorization errors (403)
	ErrCodeForbidden        ErrorCode = "FORBIDDEN"
	ErrCodeTierLimitReached ErrorCode = "TIER_LIMIT_REACHED"
	ErrCodeQuotaExceeded    ErrorCode = "QUOTA_EXCEEDED"

	// Resource errors (404, 409)
	ErrCodeNotFound      ErrorCode = "NOT_FOUND"
	ErrCodeAlreadyExists ErrorCode = "ALREADY_EXISTS"
	ErrCodeConflict      ErrorCode = "CONFLICT"

	// Validation errors (400)
	ErrCodeValidation   ErrorCode = "VALIDATION_ERROR"
	ErrCodeInvalidInput ErrorCode = "INVALID_INPUT"
	ErrCodeMissingField ErrorCode = "MISSING_FIELD"

	// Scheduling errors (503)
	ErrCodeNoCapacity     ErrorCode = "NO_CAPACITY"
	ErrCodeNoGPUAvailable ErrorCode = "NO_GPU_AVAILABLE"
	ErrCodeMachineOffline ErrorCode = "MACHINE_OFFLINE"

	// Runtime errors (500, 502)
	ErrCodeDeployFailed    ErrorCode = "DEPLOY_FAILED"
	ErrCodeRuntimeNotFound ErrorCode = "RUNTIME_NOT_FOUND"
	ErrCodeArtifactCorrupt ErrorCode = "ARTIFACT_CORRUPT"

	// External service errors (502)
	ErrCodeStripeError   ErrorCode = "STRIPE_ERROR"
	ErrCodeSupabaseError ErrorCode = "SUPABASE_ERROR"
	ErrCodeEngineError   ErrorCode = "ENGINE_ERROR"

	// Internal errors (500)
	ErrCodeInternal      ErrorCode = "INTERNAL_ERROR"
	ErrCodeDatabaseError ErrorCode = "DATABASE_ERROR"

	// Rate limiting (429)
	ErrCodeRateLimited ErrorCode = "RATE_LIMITED"
)

// AppError represents a structured application error
type AppError struct {
	Code       ErrorCode         `json:"code"`
	Message    string            `json:"message"`
	Details    map[string]string `json:"details,omitempty"`
	HTTPStatus int               `json:"-"`
	Cause      error             `json:"-"`
	RequestID  string            `json:"-"`
}

// Error implements the error interface
func (e *AppError) Error() string {
	if e.Cause != nil {
		return fmt.Sprintf("[%s] %s: %v", e.Code, e.Message, e.Cause)
	}
	return fmt.Sprintf("[%s] %s", e.Code, e.Message)
}

// Unwrap returns the underlying error
func (e *AppError) Unwrap() error {
	return e.Cause
}

// WithCause adds an underlying cause
func (e *AppError) WithCause(cause error) *AppError {
	e.Cause = cause
	return e
}

// WithDetails adds detail fields
func (e *AppError) WithDetails(details map[string]string) *AppError {
	e.Details = details
	return e
}

// WithRequestID adds request ID for tracing
func (e *AppError) WithRequestID(requestID string) *AppError {
	e.RequestID = requestID
	return e
}

// --- Constructors ---

// NewUnauthorizedError creates an unauthorized error
func NewUnauthorizedError(message string) *AppError {
	return &AppError{
		Code:       ErrCodeUnauthorized,
		Message:    message,
		HTTPStatus: http.StatusUnauthorized,
	}
}

// NewTokenExpiredError creates a token expired error
func NewTokenExpiredError() *AppError {
	return &AppError{
		Code:       ErrCodeTokenExpired,
		Message:    "Authentication token has expired",
		HTTPStatus: http.StatusUnauthorized,
	}
}

// NewForbiddenError creates a forbidden error
func NewForbiddenError(message string) *AppError {
	return &AppError{
		Code:       ErrCodeForbidden,
		Message:    message,
		HTTPStatus: http.StatusForbidden,
	}
}

// NewNotFoundError creates a not found error
func NewNotFoundError(resource, id string) *AppError {
	return &AppError{
		Code:       ErrCodeNotFound,
		Message:    fmt.Sprintf("%s not found", resource),
		HTTPStatus: http.StatusNotFound,
		Details: map[string]string{
			"resource": resource,
			"id":       id,
		},
	}
}

// NewValidationError creates a validation error
func NewValidationError(message string, field string) *AppError {
	return &AppError{
		Code:       ErrCodeValidation,
		Message:    message,
		HTTPStatus: http.StatusBadRequest,
		Details: map[string]string{
			"field": field,
		},
	}
}

// NewTierLimitError creates a tier limit error
func NewTierLimitError(tier string, limit, current int) *AppError {
	return &AppError{
		Code:       ErrCodeTierLimitReached,
		Message:    fmt.Sprintf("Capsule limit reached for %s tier", tier),
		HTTPStatus: http.StatusForbidden,
		Details: map[string]string{
			"tier":    tier,
			"limit":   fmt.Sprintf("%d", limit),
			"current": fmt.Sprintf("%d", current),
		},
	}
}

// NewNoCapacityError creates a no capacity error
func NewNoCapacityError(reason string) *AppError {
	return &AppError{
		Code:       ErrCodeNoCapacity,
		Message:    "No capacity available for deployment",
		HTTPStatus: http.StatusServiceUnavailable,
		Details: map[string]string{
			"reason": reason,
		},
	}
}

// NewDeployFailedError creates a deployment failure error
func NewDeployFailedError(capsuleID string, cause error) *AppError {
	return &AppError{
		Code:       ErrCodeDeployFailed,
		Message:    "Failed to deploy capsule",
		HTTPStatus: http.StatusInternalServerError,
		Details: map[string]string{
			"capsule_id": capsuleID,
		},
		Cause: cause,
	}
}

// NewInternalError creates an internal error
func NewInternalError(message string) *AppError {
	return &AppError{
		Code:       ErrCodeInternal,
		Message:    message,
		HTTPStatus: http.StatusInternalServerError,
	}
}

// NewExternalServiceError creates an external service error
func NewExternalServiceError(service string, cause error) *AppError {
	var code ErrorCode
	switch service {
	case "stripe":
		code = ErrCodeStripeError
	case "supabase":
		code = ErrCodeSupabaseError
	case "engine":
		code = ErrCodeEngineError
	default:
		code = ErrCodeInternal
	}

	return &AppError{
		Code:       code,
		Message:    fmt.Sprintf("%s service error", service),
		HTTPStatus: http.StatusBadGateway,
		Details: map[string]string{
			"service": service,
		},
		Cause: cause,
	}
}

// NewRateLimitedError creates a rate limited error
func NewRateLimitedError(tier string) *AppError {
	return &AppError{
		Code:       ErrCodeRateLimited,
		Message:    "Rate limit exceeded",
		HTTPStatus: http.StatusTooManyRequests,
		Details: map[string]string{
			"tier": tier,
		},
	}
}

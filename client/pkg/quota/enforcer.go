package quota

import (
	"context"
	"errors"
)

var ErrQuotaExceeded = errors.New("quota exceeded")

type PlanLimits struct {
	MaxCapsules    int
	MaxMemoryMB    int
	MaxCPUCores    float64
	MaxStorageMB   int
	MaxEgressGB    int
	MaxRequestsDay int
}

var Plans = map[string]PlanLimits{
	"free": {
		MaxCapsules:    1,
		MaxMemoryMB:    512,
		MaxCPUCores:    0.5,
		MaxStorageMB:   1024,
		MaxEgressGB:    10,
		MaxRequestsDay: 10000,
	},
	"pro": {
		MaxCapsules:    5,
		MaxMemoryMB:    2048,
		MaxCPUCores:    1.0,
		MaxStorageMB:   10240,
		MaxEgressGB:    100,
		MaxRequestsDay: 100000,
	},
	"enterprise": {
		MaxCapsules:    -1, // unlimited
		MaxMemoryMB:    -1,
		MaxCPUCores:    -1,
		MaxStorageMB:   -1,
		MaxEgressGB:    -1,
		MaxRequestsDay: -1,
	},
}

// DBClientInterface defines what we need from the DB layer
type DBClientInterface interface {
    GetUserPlan(ctx context.Context, userID string) (string, error)
    CountUserCapsules(ctx context.Context, userID string) (int, error)
}

type Enforcer struct {
	db DBClientInterface
}

func NewEnforcer(db DBClientInterface) *Enforcer {
    return &Enforcer{db: db}
}

func (e *Enforcer) CheckDeployAllowed(ctx context.Context, userID string) error {
    planName, err := e.db.GetUserPlan(ctx, userID)
    if err != nil {
        return err
    }
    
    // Default to free if unknown
    if _, ok := Plans[planName]; !ok {
        planName = "free"
    }

	limits := Plans[planName]
	if limits.MaxCapsules == -1 {
		return nil // unlimited
	}

	count, err := e.db.CountUserCapsules(ctx, userID)
	if err != nil {
		return err
	}

	if count >= limits.MaxCapsules {
		return ErrQuotaExceeded
	}

	return nil
}

func (e *Enforcer) GetResourceLimits(ctx context.Context, userID string) (*PlanLimits, error) {
	planName, err := e.db.GetUserPlan(ctx, userID)
	if err != nil {
		return nil, err
	}
    
    if _, ok := Plans[planName]; !ok {
        planName = "free"
    }

	limits := Plans[planName]
	return &limits, nil
}

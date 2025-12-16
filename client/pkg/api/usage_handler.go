package api

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/onescluster/coordinator/pkg/billing"
	"github.com/onescluster/coordinator/pkg/supabase"
)

type UsageHandler struct {
	supabase *supabase.Client
	metering *billing.MeteredBillingService
}

func NewUsageHandler(client *supabase.Client, metering *billing.MeteredBillingService) *UsageHandler {
	return &UsageHandler{
		supabase: client,
		metering: metering,
	}
}

type ReportUsageRequest struct {
	CapsuleID string    `json:"capsule_id"`
	UserID    string    `json:"user_id"`
	Resource  string    `json:"resource"`
	Amount    float64   `json:"amount"`
	StartTime time.Time `json:"start_time"`
	EndTime   time.Time `json:"end_time"`
}

func (h *UsageHandler) HandleReportUsage(w http.ResponseWriter, r *http.Request) {
	// TODO: Authenticate Machine (e.g. via Bearer token matching machine ID or secret)

	var req ReportUsageRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	log := supabase.UsageLog{
		UserID:    req.UserID,
		CapsuleID: req.CapsuleID,
		Resource:  req.Resource,
		Amount:    req.Amount,
		StartTime: req.StartTime,
		EndTime:   req.EndTime,
	}

	if err := h.supabase.LogUsage(log); err != nil {
		http.Error(w, "Failed to log usage", http.StatusInternalServerError)
		return
	}

	// Report to Stripe (Async)
	// Only report if resource is "gpu_hour"
	if req.Resource == "gpu_hour" {
		go func() {
			// Use background context for async task
			if err := h.metering.ReportUsage(context.Background(), req.UserID, req.Amount); err != nil {
				// Log error (should use a logger)
				// fmt.Printf("Failed to report usage to Stripe: %v\n", err)
			}
		}()
	}

	w.WriteHeader(http.StatusOK)
}

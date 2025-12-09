package api

import (
	"encoding/json"
	"net/http"

	"github.com/onescluster/coordinator/pkg/billing"
	"github.com/onescluster/coordinator/pkg/middleware"
	"github.com/onescluster/coordinator/pkg/supabase"
)

type BillingHandler struct {
	stripe   *billing.StripeClient
	supabase *supabase.Client
}

func NewBillingHandler(sc *billing.StripeClient, supabase *supabase.Client) *BillingHandler {
	return &BillingHandler{
		stripe:   sc,
		supabase: supabase,
	}
}

type CreateCheckoutRequest struct {
	Tier       string `json:"tier"`
	SuccessURL string `json:"success_url"`
	CancelURL  string `json:"cancel_url"`
}

type CreateCheckoutResponse struct {
	URL string `json:"url"`
}

// POST /api/v1/billing/checkout
func (h *BillingHandler) CreateCheckout(w http.ResponseWriter, r *http.Request) {
	user, ok := middleware.GetUser(r.Context())
	if !ok {
		http.Error(w, "Unauthorized", http.StatusUnauthorized)
		return
	}

	var req CreateCheckoutRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	// Validate tier
	validTiers := map[string]bool{"everyday": true, "fast": true, "studio": true}
	if !validTiers[req.Tier] {
		http.Error(w, "Invalid tier", http.StatusBadRequest)
		return
	}

	// Get or create Stripe customer
	profile, err := h.supabase.GetProfile(r.Context(), user.ID)
	if err != nil {
		http.Error(w, "Failed to get profile", http.StatusInternalServerError)
		return
	}

	customerID := profile.StripeCustomerID
	if customerID == "" {
		// Create new Stripe customer
		cust, err := h.stripe.CreateCustomer(r.Context(), user.ID, user.Email, profile.DisplayName)
		if err != nil {
			http.Error(w, "Failed to create customer", http.StatusInternalServerError)
			return
		}
		customerID = cust.ID

		// Save customer ID to Supabase
		h.supabase.UpdateStripeCustomerID(r.Context(), user.ID, customerID)
	}

	// Create checkout session
	session, err := h.stripe.CreateCheckoutSession(
		r.Context(),
		customerID,
		req.Tier,
		req.SuccessURL,
		req.CancelURL,
	)
	if err != nil {
		http.Error(w, "Failed to create checkout session", http.StatusInternalServerError)
		return
	}

	json.NewEncoder(w).Encode(CreateCheckoutResponse{
		URL: session.URL,
	})
}

// POST /api/v1/billing/portal
func (h *BillingHandler) CreatePortalSession(w http.ResponseWriter, r *http.Request) {
	user, ok := middleware.GetUser(r.Context())
	if !ok {
		http.Error(w, "Unauthorized", http.StatusUnauthorized)
		return
	}

	profile, err := h.supabase.GetProfile(r.Context(), user.ID)
	if err != nil || profile.StripeCustomerID == "" {
		http.Error(w, "No billing account found", http.StatusNotFound)
		return
	}

	var req struct {
		ReturnURL string `json:"return_url"`
	}
	json.NewDecoder(r.Body).Decode(&req)

	session, err := h.stripe.CreatePortalSession(r.Context(), profile.StripeCustomerID, req.ReturnURL)
	if err != nil {
		http.Error(w, "Failed to create portal session", http.StatusInternalServerError)
		return
	}

	json.NewEncoder(w).Encode(map[string]string{
		"url": session.URL,
	})
}

// GET /api/v1/billing/subscription
func (h *BillingHandler) GetSubscription(w http.ResponseWriter, r *http.Request) {
	user, ok := middleware.GetUser(r.Context())
	if !ok {
		http.Error(w, "Unauthorized", http.StatusUnauthorized)
		return
	}

	profile, err := h.supabase.GetProfile(r.Context(), user.ID)
	if err != nil {
		http.Error(w, "Failed to get profile", http.StatusInternalServerError)
		return
	}

	response := map[string]interface{}{
		"tier":              profile.Tier,
		"status":            profile.SubscriptionStatus,
		"current_period_end": profile.SubscriptionPeriodEnd,
		"cancel_at_period_end": profile.SubscriptionStatus == "cancelling",
	}

	// Get usage for current period if on metered plan
	if profile.Tier == "fast" || profile.Tier == "studio" {
		usage, _ := h.supabase.GetCurrentPeriodUsage(r.Context(), user.ID)
		response["gpu_hours_used"] = usage.GPUHours
		response["gpu_hours_included"] = map[string]int{
			"fast":   100,
			"studio": 500,
		}[profile.Tier]
	}

	json.NewEncoder(w).Encode(response)
}

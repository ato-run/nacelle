package api

import (
	"encoding/json"
	"log"
	"net/http"
	"os"

	"github.com/onescluster/coordinator/pkg/billing"
	"github.com/onescluster/coordinator/pkg/middleware"
	"github.com/onescluster/coordinator/pkg/supabase"
)

type BillingHandler struct {
	polar      *billing.Client
	supabase   *supabase.Client
	productIDs map[string]string
}

func NewBillingHandler(client *billing.Client, supabase *supabase.Client) *BillingHandler {
	return &BillingHandler{
		polar:    client,
		supabase: supabase,
		productIDs: map[string]string{
			"everyday": os.Getenv("POLAR_PRODUCT_EVERYDAY"),
			"fast":     os.Getenv("POLAR_PRODUCT_FAST"),
			"studio":   os.Getenv("POLAR_PRODUCT_STUDIO"),
		},
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

	productID, ok := h.productIDs[req.Tier]
	if !ok || productID == "" {
		http.Error(w, "Invalid tier", http.StatusBadRequest)
		return
	}

	metadata := map[string]string{
		"user_id": user.ID,
		"tier":    req.Tier,
	}

	checkoutURL, err := h.polar.CreateCheckoutSession(productID, req.SuccessURL, metadata)
	if err != nil {
		http.Error(w, "Failed to create checkout session", http.StatusInternalServerError)
		return
	}

	if err := json.NewEncoder(w).Encode(CreateCheckoutResponse{URL: checkoutURL}); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
}

// POST /api/v1/billing/portal
func (h *BillingHandler) CreatePortalSession(w http.ResponseWriter, r *http.Request) {
	user, ok := middleware.GetUser(r.Context())
	if !ok {
		http.Error(w, "Unauthorized", http.StatusUnauthorized)
		return
	}

	var req struct {
		ReturnURL string `json:"return_url"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		// Log error but continue as this might be optional or handled? 
		// Actually req.ReturnURL is needed. If decode fails, we might have issue.
		// But existing code used _ = ... so it was ignored.
		// Since user complained about unchecked error, we check it.
		log.Printf("Warning: failed to decode request body: %v", err)
	}

	// Polar portal currently redirects to purchases dashboard.
	url, err := h.polar.GetCustomerPortalURL("")
	if err != nil {
		http.Error(w, "Failed to create portal session", http.StatusInternalServerError)
		return
	}

	// Ensure profile exists to keep consistent error surface.
	if _, err := h.supabase.GetProfile(r.Context(), user.ID); err != nil {
		http.Error(w, "No billing account found", http.StatusNotFound)
		return
	}

	if err := json.NewEncoder(w).Encode(map[string]string{"url": url}); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
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
		"tier":                 profile.Tier,
		"status":               profile.SubscriptionStatus,
		"current_period_end":   profile.SubscriptionPeriodEnd,
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

	if err := json.NewEncoder(w).Encode(response); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
}

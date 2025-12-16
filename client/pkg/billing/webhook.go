package billing

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"time"

	"github.com/onescluster/coordinator/pkg/supabase"
)

// PolarWebhookEvent is a minimal structure for Polar events.
type PolarWebhookEvent struct {
	Type string          `json:"type"`
	Data json.RawMessage `json:"data"`
}

// SubscriptionData captures the subset of subscription fields we care about.
type SubscriptionData struct {
	ID               string            `json:"id"`
	UserID           string            `json:"user_id"`
	Metadata         map[string]string `json:"metadata"`
	Status           string            `json:"status"`
	CurrentPeriodEnd string            `json:"current_period_end"`
}

type WebhookHandler struct {
	client        *Client
	supabase      *supabase.Client
	webhookSecret string
}

func NewWebhookHandler(client *Client, supabaseClient *supabase.Client, webhookSecret string) *WebhookHandler {
	return &WebhookHandler{
		client:        client,
		supabase:      supabaseClient,
		webhookSecret: webhookSecret,
	}
}

func (h *WebhookHandler) HandleWebhook(w http.ResponseWriter, r *http.Request) {
	event, err := h.client.HandleWebhook(r, h.webhookSecret)
	if err != nil {
		log.Printf("Polar webhook verification failed: %v", err)
		http.Error(w, "invalid webhook", http.StatusBadRequest)
		return
	}

	switch event.Type {
	case "subscription.created", "subscription.active", "subscription.updated":
		h.handleSubscriptionUpsert(r, event)
	case "subscription.cancelled", "subscription.inactive", "subscription.expired":
		h.handleSubscriptionCancelled(r, event)
	default:
		log.Printf("Unhandled Polar event: %s", event.Type)
	}

	w.WriteHeader(http.StatusOK)
}

func (h *WebhookHandler) handleSubscriptionUpsert(r *http.Request, event *PolarWebhookEvent) {
	sub, err := ParseSubscription(event.Data)
	if err != nil {
		log.Printf("Failed to parse subscription payload: %v", err)
		return
	}

	userID := sub.UserID
	if userID == "" {
		userID = sub.Metadata["user_id"]
	}
	tier := sub.Metadata["tier"]

	if err := h.supabase.UpdateSubscription(r.Context(), userID, tier, sub.ID, ""); err != nil {
		log.Printf("Failed to persist subscription: %v", err)
		return
	}

	// Update status/period end if provided.
	var periodEnd int64
	if sub.CurrentPeriodEnd != "" {
		if t, err := time.Parse(time.RFC3339, sub.CurrentPeriodEnd); err == nil {
			periodEnd = t.Unix()
		}
	}

	if err := h.supabase.UpdateSubscriptionStatus(r.Context(), userID, tier, "active", periodEnd); err != nil {
		log.Printf("Failed to update subscription status: %v", err)
	}
}

func (h *WebhookHandler) handleSubscriptionCancelled(r *http.Request, event *PolarWebhookEvent) {
	sub, err := ParseSubscription(event.Data)
	if err != nil {
		log.Printf("Failed to parse subscription payload: %v", err)
		return
	}

	userID := sub.UserID
	if userID == "" {
		userID = sub.Metadata["user_id"]
	}

	var periodEnd int64
	if sub.CurrentPeriodEnd != "" {
		if t, err := time.Parse(time.RFC3339, sub.CurrentPeriodEnd); err == nil {
			periodEnd = t.Unix()
		}
	}

	if err := h.supabase.UpdateSubscriptionStatus(r.Context(), userID, "free", "cancelled", periodEnd); err != nil {
		log.Printf("Failed to mark subscription cancelled: %v", err)
	}
}

// ParseSubscription extracts subscription details from the event data.
func ParseSubscription(data json.RawMessage) (*SubscriptionData, error) {
	var sub SubscriptionData
	if err := json.Unmarshal(data, &sub); err != nil {
		return nil, fmt.Errorf("parse subscription: %w", err)
	}
	if sub.Metadata == nil {
		sub.Metadata = map[string]string{}
	}
	if id, ok := sub.Metadata["user_id"]; ok && sub.UserID == "" {
		sub.UserID = id
	}
	return &sub, nil
}

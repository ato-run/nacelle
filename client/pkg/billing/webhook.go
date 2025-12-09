package billing

import (
	"context"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"os"

	"github.com/stripe/stripe-go/v76"
	"github.com/stripe/stripe-go/v76/customer"
	"github.com/stripe/stripe-go/v76/webhook"

	"github.com/onescluster/coordinator/pkg/supabase"
)

type WebhookHandler struct {
	stripeClient   *StripeClient
	supabaseClient *supabase.Client
	webhookSecret  string
}

func NewWebhookHandler(sc *StripeClient, supabase *supabase.Client) *WebhookHandler {
	return &WebhookHandler{
		stripeClient:   sc,
		supabaseClient: supabase,
		webhookSecret:  os.Getenv("STRIPE_WEBHOOK_SECRET"),
	}
}

func (h *WebhookHandler) HandleWebhook(w http.ResponseWriter, r *http.Request) {
	const MaxBodyBytes = int64(65536)
	r.Body = http.MaxBytesReader(w, r.Body, MaxBodyBytes)
	
	payload, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, "Error reading body", http.StatusBadRequest)
		return
	}

	sig := r.Header.Get("Stripe-Signature")
	event, err := webhook.ConstructEvent(payload, sig, h.webhookSecret)
	if err != nil {
		log.Printf("Webhook signature verification failed: %v", err)
		http.Error(w, "Invalid signature", http.StatusBadRequest)
		return
	}

	log.Printf("Received webhook event: %s", event.Type)

	switch event.Type {
	case "checkout.session.completed":
		h.handleCheckoutCompleted(event)
	case "customer.subscription.updated":
		h.handleSubscriptionUpdated(event)
	case "customer.subscription.deleted":
		h.handleSubscriptionDeleted(event)
	case "invoice.paid":
		h.handleInvoicePaid(event)
	case "invoice.payment_failed":
		h.handlePaymentFailed(event)
	default:
		log.Printf("Unhandled event type: %s", event.Type)
	}

	w.WriteHeader(http.StatusOK)
}

func (h *WebhookHandler) handleCheckoutCompleted(event stripe.Event) {
	var session stripe.CheckoutSession
	if err := json.Unmarshal(event.Data.Raw, &session); err != nil {
		log.Printf("Error parsing checkout session: %v", err)
		return
	}

	// Get user ID from customer metadata
	cust, err := h.getCustomerWithMetadata(session.Customer.ID)
	if err != nil {
		log.Printf("Error getting customer: %v", err)
		return
	}

	userID := cust.Metadata["user_id"]
	tier := session.Metadata["tier"]

	// Update user tier in Supabase
	if err := h.supabaseClient.UpdateSubscription(
		context.Background(),
		userID,
		tier,
		session.Subscription.ID,
		session.Customer.ID,
	); err != nil {
		log.Printf("Error updating subscription: %v", err)
		return
	}

	log.Printf("User %s upgraded to %s", userID, tier)
}

func (h *WebhookHandler) handleSubscriptionUpdated(event stripe.Event) {
	var sub stripe.Subscription
	if err := json.Unmarshal(event.Data.Raw, &sub); err != nil {
		log.Printf("Error parsing subscription: %v", err)
		return
	}

	// Determine tier from price
	tier := h.getTierFromSubscription(&sub)

	// Get user ID from customer
	cust, err := h.getCustomerWithMetadata(sub.Customer.ID)
	if err != nil {
		log.Printf("Error getting customer: %v", err)
		return
	}

	userID := cust.Metadata["user_id"]

	// Update in Supabase
	status := "active"
	if sub.CancelAtPeriodEnd {
		status = "cancelling"
	}

	if err := h.supabaseClient.UpdateSubscriptionStatus(
		context.Background(),
		userID,
		tier,
		status,
		sub.CurrentPeriodEnd,
	); err != nil {
		log.Printf("Error updating subscription status: %v", err)
	}
}

func (h *WebhookHandler) handleSubscriptionDeleted(event stripe.Event) {
	var sub stripe.Subscription
	if err := json.Unmarshal(event.Data.Raw, &sub); err != nil {
		log.Printf("Error parsing subscription: %v", err)
		return
	}

	cust, _ := h.getCustomerWithMetadata(sub.Customer.ID)
	userID := cust.Metadata["user_id"]

	// Downgrade to free tier
	if err := h.supabaseClient.UpdateSubscription(
		context.Background(),
		userID,
		"free",
		"",
		sub.Customer.ID,
	); err != nil {
		log.Printf("Error downgrading subscription: %v", err)
	}

	log.Printf("User %s downgraded to free", userID)
}

func (h *WebhookHandler) handleInvoicePaid(event stripe.Event) {
	// Optional: Log payment success
}

func (h *WebhookHandler) handlePaymentFailed(event stripe.Event) {
	var invoice stripe.Invoice
	if err := json.Unmarshal(event.Data.Raw, &invoice); err != nil {
		log.Printf("Error parsing invoice: %v", err)
		return
	}

	cust, _ := h.getCustomerWithMetadata(invoice.Customer.ID)
	userID := cust.Metadata["user_id"]

	// Mark subscription as past_due
	if err := h.supabaseClient.UpdateSubscriptionStatus(
		context.Background(),
		userID,
		"", // keep current tier
		"past_due",
		0,
	); err != nil {
		log.Printf("Error updating subscription status: %v", err)
	}

	log.Printf("Payment failed for user %s", userID)
}

func (h *WebhookHandler) getTierFromSubscription(sub *stripe.Subscription) string {
	for _, item := range sub.Items.Data {
		switch item.Price.LookupKey {
		case "everyday_monthly":
			return "everyday"
		case "fast_monthly":
			return "fast"
		case "studio_monthly":
			return "studio"
		}
	}
	return "free"
}

func (h *WebhookHandler) getCustomerWithMetadata(customerID string) (*stripe.Customer, error) {
	return customer.Get(customerID, nil)
}

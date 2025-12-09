package billing

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/stripe/stripe-go/v76"
	"github.com/stripe/stripe-go/v76/checkout/session"
	"github.com/stripe/stripe-go/v76/customer"
	portalsession "github.com/stripe/stripe-go/v76/billingportal/session"
	"github.com/stripe/stripe-go/v76/subscription"
	"github.com/stripe/stripe-go/v76/usagerecord"
)

type StripeClient struct {
	priceIDs map[string]string
}

func NewStripeClient() *StripeClient {
	stripe.Key = os.Getenv("STRIPE_SECRET_KEY")
	
	return &StripeClient{
		priceIDs: map[string]string{
			"everyday": os.Getenv("STRIPE_PRICE_EVERYDAY"),
			"fast":     os.Getenv("STRIPE_PRICE_FAST"),
			"studio":   os.Getenv("STRIPE_PRICE_STUDIO"),
			"gpu_hour": os.Getenv("STRIPE_PRICE_GPU_HOUR"),
		},
	}
}

// CreateCustomer creates a Stripe customer for a new user
func (c *StripeClient) CreateCustomer(ctx context.Context, userID, email, name string) (*stripe.Customer, error) {
	params := &stripe.CustomerParams{
		Email: stripe.String(email),
		Name:  stripe.String(name),
		Metadata: map[string]string{
			"user_id": userID,
		},
	}
	
	return customer.New(params)
}

// CreateCheckoutSession creates a checkout session for plan upgrade
func (c *StripeClient) CreateCheckoutSession(
	ctx context.Context,
	customerID string,
	tier string,
	successURL string,
	cancelURL string,
) (*stripe.CheckoutSession, error) {
	priceID, ok := c.priceIDs[tier]
	if !ok {
		return nil, fmt.Errorf("unknown tier: %s", tier)
	}

	lineItems := []*stripe.CheckoutSessionLineItemParams{
		{
			Price:    stripe.String(priceID),
			Quantity: stripe.Int64(1),
		},
	}

	// Add metered GPU hours for Fast and Studio tiers
	if tier == "fast" || tier == "studio" {
		lineItems = append(lineItems, &stripe.CheckoutSessionLineItemParams{
			Price: stripe.String(c.priceIDs["gpu_hour"]),
		})
	}

	params := &stripe.CheckoutSessionParams{
		Customer:           stripe.String(customerID),
		Mode:               stripe.String(string(stripe.CheckoutSessionModeSubscription)),
		SuccessURL:         stripe.String(successURL),
		CancelURL:          stripe.String(cancelURL),
		LineItems:          lineItems,
		AllowPromotionCodes: stripe.Bool(true),
		BillingAddressCollection: stripe.String("auto"),
		Metadata: map[string]string{
			"tier": tier,
		},
	}

	return session.New(params)
}

// CreatePortalSession creates a customer portal session for subscription management
func (c *StripeClient) CreatePortalSession(
	ctx context.Context,
	customerID string,
	returnURL string,
) (*stripe.BillingPortalSession, error) {
	params := &stripe.BillingPortalSessionParams{
		Customer:  stripe.String(customerID),
		ReturnURL: stripe.String(returnURL),
	}

	return portalsession.New(params)
}

// GetSubscription retrieves the current subscription for a customer
func (c *StripeClient) GetSubscription(ctx context.Context, subscriptionID string) (*stripe.Subscription, error) {
	return subscription.Get(subscriptionID, nil)
}

// CancelSubscription cancels a subscription at period end
func (c *StripeClient) CancelSubscription(ctx context.Context, subscriptionID string) (*stripe.Subscription, error) {
	params := &stripe.SubscriptionParams{
		CancelAtPeriodEnd: stripe.Bool(true),
	}
	return subscription.Update(subscriptionID, params)
}

// ReportGPUUsage reports GPU usage for metered billing
func (c *StripeClient) ReportGPUUsage(
	ctx context.Context,
	subscriptionItemID string,
	gpuHours int64,
	timestamp time.Time,
) (*stripe.UsageRecord, error) {
	params := &stripe.UsageRecordParams{
		SubscriptionItem: stripe.String(subscriptionItemID),
		Quantity:         stripe.Int64(gpuHours),
		Timestamp:        stripe.Int64(timestamp.Unix()),
		Action:           stripe.String(string(stripe.UsageRecordActionIncrement)),
	}

	return usagerecord.New(params)
}

// GetMeteredSubscriptionItemID finds the subscription item for GPU hours
func (c *StripeClient) GetMeteredSubscriptionItemID(sub *stripe.Subscription) string {
	gpuPriceID := c.priceIDs["gpu_hour"]
	for _, item := range sub.Items.Data {
		if item.Price.ID == gpuPriceID {
			return item.ID
		}
	}
	return ""
}

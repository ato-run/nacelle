package billing

import (
	"context"
	"math"
	"time"

	"github.com/onescluster/coordinator/pkg/supabase"
)

type MeteredBillingService struct {
	stripe   *StripeClient
	supabase *supabase.Client
}

func NewMeteredBillingService(stripe *StripeClient, supabase *supabase.Client) *MeteredBillingService {
	return &MeteredBillingService{
		stripe:   stripe,
		supabase: supabase,
	}
}

func (s *MeteredBillingService) ReportUsage(ctx context.Context, userID string, amountHours float64) error {
	// 1. Get Profile to find subscription ID
	profile, err := s.supabase.GetProfile(ctx, userID)
	if err != nil {
		return err
	}

	if profile.StripeCustomerID == "" || profile.Tier == "free" || profile.Tier == "everyday" {
		// No metered billing for these tiers
		return nil
	}

	if profile.StripeSubscriptionID == "" {
		// No active subscription ID found
		return nil
	}

	// 2. Get Subscription from Stripe
	sub, err := s.stripe.GetSubscription(ctx, profile.StripeSubscriptionID)
	if err != nil {
		return err
	}

	// 3. Find Item
	itemID := s.stripe.GetMeteredSubscriptionItemID(sub)
	if itemID == "" {
		// Subscription does not have metered item
		return nil
	}
	
	// 4. Report to Stripe
	// Round up to nearest hour for now (MVP)
	quantity := int64(math.Ceil(amountHours))
	if quantity <= 0 {
		return nil
	}
	
	_, err = s.stripe.ReportGPUUsage(ctx, itemID, quantity, time.Now())
	return err
}

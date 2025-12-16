package billing

import (
	"context"

	"github.com/onescluster/coordinator/pkg/supabase"
)

type MeteredBillingService struct {
	supabase *supabase.Client
}

func NewMeteredBillingService(supabase *supabase.Client) *MeteredBillingService {
	return &MeteredBillingService{
		supabase: supabase,
	}
}

func (s *MeteredBillingService) ReportUsage(ctx context.Context, userID string, amountHours float64) error {
	// Polar metering TBD; for now, ensure profile exists and no-op.
	if _, err := s.supabase.GetProfile(ctx, userID); err != nil {
		return err
	}
	_ = amountHours
	return nil
}

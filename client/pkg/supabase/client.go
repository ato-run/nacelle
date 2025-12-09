package supabase

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

type Client struct {
	url     string
	apiKey  string
	client  *http.Client
}

type UsageLog struct {
	UserID          string    `json:"user_id"`
	CapsuleID       string    `json:"capsule_id"`
	Resource        string    `json:"resource"` // e.g. "gpu_vram"
	Amount          float64   `json:"amount"`   // e.g. GB-hours
	StartTime       time.Time `json:"start_time"`
	EndTime         time.Time `json:"end_time"`
}

type Profile struct {
	ID                    string `json:"id"`
	StripeCustomerID      string `json:"stripe_customer_id"`
	StripeSubscriptionID  string `json:"stripe_subscription_id"`
	Tier                  string `json:"tier"`
	SubscriptionStatus    string `json:"subscription_status"`
	SubscriptionPeriodEnd string `json:"subscription_period_end"` // RFC3339
	DisplayName           string `json:"display_name"`
}

type Usage struct {
	GPUHours int `json:"gpu_hours"`
}

func NewClient(url, apiKey string) *Client {
	return &Client{
		url:    url,
		apiKey: apiKey,
		client: &http.Client{Timeout: 10 * time.Second},
	}
}

func (c *Client) LogUsage(log UsageLog) error {
	if c.url == "" || c.apiKey == "" {
		// Skip if not configured (dev mode)
		return nil
	}

	endpoint := fmt.Sprintf("%s/rest/v1/usage_logs", c.url)
	
	body, err := json.Marshal(log)
	if err != nil {
		return err
	}

	return c.doRequest(context.Background(), "POST", endpoint, body)
}

func (c *Client) GetProfile(ctx context.Context, userID string) (*Profile, error) {
	endpoint := fmt.Sprintf("%s/rest/v1/profiles?id=eq.%s&select=*", c.url, userID)
	
	req, err := http.NewRequestWithContext(ctx, "GET", endpoint, nil)
	if err != nil {
		return nil, err
	}
	
	c.setHeaders(req)
	
	resp, err := c.client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("failed to get profile: %d", resp.StatusCode)
	}
	
	var profiles []Profile
	if err := json.NewDecoder(resp.Body).Decode(&profiles); err != nil {
		return nil, err
	}
	
	if len(profiles) == 0 {
		return nil, fmt.Errorf("profile not found")
	}
	
	return &profiles[0], nil
}

func (c *Client) UpdateSubscription(ctx context.Context, userID, tier, subID, custID string) error {
	endpoint := fmt.Sprintf("%s/rest/v1/profiles?id=eq.%s", c.url, userID)
	
	data := map[string]interface{}{
		"tier":                   tier,
		"stripe_subscription_id": subID,
		"stripe_customer_id":     custID,
		"subscription_status":    "active",
		"updated_at":             time.Now().Format(time.RFC3339),
	}
	
	body, _ := json.Marshal(data)
	return c.doRequest(ctx, "PATCH", endpoint, body)
}

func (c *Client) UpdateStripeCustomerID(ctx context.Context, userID, custID string) error {
	endpoint := fmt.Sprintf("%s/rest/v1/profiles?id=eq.%s", c.url, userID)
	
	data := map[string]string{
		"stripe_customer_id": custID,
	}
	
	body, _ := json.Marshal(data)
	return c.doRequest(ctx, "PATCH", endpoint, body)
}

func (c *Client) UpdateSubscriptionStatus(ctx context.Context, userID, tier, status string, periodEnd int64) error {
	endpoint := fmt.Sprintf("%s/rest/v1/profiles?id=eq.%s", c.url, userID)
	
	data := map[string]interface{}{
		"subscription_status": status,
		"updated_at":          time.Now().Format(time.RFC3339),
	}
	if tier != "" {
		data["tier"] = tier
	}
	if periodEnd > 0 {
		data["subscription_period_end"] = time.Unix(periodEnd, 0).Format(time.RFC3339)
	}
	
	body, _ := json.Marshal(data)
	return c.doRequest(ctx, "PATCH", endpoint, body)
}

func (c *Client) GetCurrentPeriodUsage(ctx context.Context, userID string) (*Usage, error) {
	// TODO: Implement actual query to sum usage_logs
	// For now return dummy data
	return &Usage{GPUHours: 0}, nil
}

func (c *Client) doRequest(ctx context.Context, method, url string, body []byte) error {
	req, err := http.NewRequestWithContext(ctx, method, url, bytes.NewBuffer(body))
	if err != nil {
		return err
	}
	
	c.setHeaders(req)
	
	resp, err := c.client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	
	if resp.StatusCode >= 400 {
		return fmt.Errorf("request failed with status: %d", resp.StatusCode)
	}
	
	return nil
}

func (c *Client) setHeaders(req *http.Request) {
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("apikey", c.apiKey)
	req.Header.Set("Authorization", "Bearer "+c.apiKey)
	req.Header.Set("Prefer", "return=minimal")
}

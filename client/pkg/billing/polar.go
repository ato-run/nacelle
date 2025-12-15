package billing

import (
    "bytes"
    "encoding/json"
    "fmt"
    "io"
    "net/http"
    "time"
)

const (
    PolarAPIBaseURL = "https://api.polar.sh/v1"
    PolarSandboxURL = "https://sandbox-api.polar.sh/v1"
)

// Client is a lightweight Polar API client backed by net/http.
type Client struct {
    accessToken string
    baseURL     string
    httpClient  *http.Client
}

func NewClient(accessToken string, isSandbox bool) *Client {
    baseURL := PolarAPIBaseURL
    if isSandbox {
        baseURL = PolarSandboxURL
    }
    return &Client{
        accessToken: accessToken,
        baseURL:     baseURL,
        httpClient:  &http.Client{Timeout: 10 * time.Second},
    }
}

// CreateCheckoutSession creates a checkout session for a product and returns the checkout URL.
func (c *Client) CreateCheckoutSession(productID, successURL string, metadata map[string]string) (string, error) {
    reqBody := map[string]interface{}{
        "product_id":  productID,
        "success_url": successURL,
        "metadata":    metadata,
    }

    respData, err := c.request("POST", "/checkouts/custom", reqBody)
    if err != nil {
        return "", err
    }

    var result struct {
        URL string `json:"url"`
    }
    if err := json.Unmarshal(respData, &result); err != nil {
        return "", fmt.Errorf("failed to parse checkout response: %w", err)
    }

    return result.URL, nil
}

// GetCustomerPortalURL returns a URL for the customer to manage their subscription.
func (c *Client) GetCustomerPortalURL(_ string) (string, error) {
    // Placeholder: Polar currently directs customers to their purchases page.
    return "https://polar.sh/purchases", nil
}

// HandleWebhook parses a Polar webhook request with minimal verification.
func (c *Client) HandleWebhook(req *http.Request, webhookSecret string) (*PolarWebhookEvent, error) {
    // Minimal shared-secret check if provided (Polar-Webhook-Secret header).
    if webhookSecret != "" {
        if req.Header.Get("Polar-Webhook-Secret") != webhookSecret {
            return nil, fmt.Errorf("invalid webhook secret")
        }
    }

    body, err := io.ReadAll(req.Body)
    if err != nil {
        return nil, err
    }
    defer req.Body.Close()

    var event PolarWebhookEvent
    if err := json.Unmarshal(body, &event); err != nil {
        return nil, fmt.Errorf("failed to parse webhook body: %w", err)
    }

    return &event, nil
}

func (c *Client) request(method, path string, body interface{}) ([]byte, error) {
    var bodyReader io.Reader
    if body != nil {
        jsonBody, err := json.Marshal(body)
        if err != nil {
            return nil, err
        }
        bodyReader = bytes.NewBuffer(jsonBody)
    }

    req, err := http.NewRequest(method, c.baseURL+path, bodyReader)
    if err != nil {
        return nil, err
    }

    req.Header.Set("Authorization", "Bearer "+c.accessToken)
    req.Header.Set("Content-Type", "application/json")

    resp, err := c.httpClient.Do(req)
    if err != nil {
        return nil, err
    }
    defer resp.Body.Close()

    respBytes, err := io.ReadAll(resp.Body)
    if err != nil {
        return nil, err
    }

    if resp.StatusCode >= 400 {
        return nil, fmt.Errorf("polar api error: %s (body: %s)", resp.Status, string(respBytes))
    }

    return respBytes, nil
}
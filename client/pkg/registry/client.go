// Package registry provides a client for the Gumball Capsule Registry API.
package registry

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"time"

	"github.com/onescluster/coordinator/pkg/capsule"
)

const (
	// DefaultRegistryURL is the default Capsule Registry endpoint.
	DefaultRegistryURL = "https://registry.gumball.dev"

	// DefaultTimeout is the default HTTP client timeout.
	DefaultTimeout = 30 * time.Second

	// DefaultCacheTTL is how long to cache registry responses.
	DefaultCacheTTL = 5 * time.Minute
)

// Client provides access to the Capsule Registry API.
type Client interface {
	// List returns capsules matching the given options.
	List(ctx context.Context, opts ListOptions) (*CapsuleListResponse, error)

	// Get returns the full manifest for a capsule.
	Get(ctx context.Context, name string) (*capsule.CapsuleManifest, error)

	// GetVersion returns the manifest for a specific version.
	GetVersion(ctx context.Context, name, version string) (*capsule.CapsuleManifest, error)

	// GetDownloadInfo returns download information for a capsule.
	GetDownloadInfo(ctx context.Context, name, version, platform string) (*DownloadInfo, error)

	// Download downloads and extracts a capsule to the destination path.
	Download(ctx context.Context, name, version, destPath string) error

	// ListVersions returns all available versions of a capsule.
	ListVersions(ctx context.Context, name string) (*VersionListResponse, error)
}

// HTTPClient is the production implementation of Client.
type HTTPClient struct {
	baseURL    string
	httpClient *http.Client
	cache      map[string]CacheEntry
	cacheMu    sync.RWMutex
	cacheTTL   time.Duration
}

// ClientOption configures the HTTP client.
type ClientOption func(*HTTPClient)

// WithBaseURL sets a custom registry URL.
func WithBaseURL(url string) ClientOption {
	return func(c *HTTPClient) {
		c.baseURL = url
	}
}

// WithTimeout sets the HTTP client timeout.
func WithTimeout(d time.Duration) ClientOption {
	return func(c *HTTPClient) {
		c.httpClient.Timeout = d
	}
}

// WithCacheTTL sets the cache duration.
func WithCacheTTL(d time.Duration) ClientOption {
	return func(c *HTTPClient) {
		c.cacheTTL = d
	}
}

// NewClient creates a new Registry client.
func NewClient(opts ...ClientOption) *HTTPClient {
	c := &HTTPClient{
		baseURL: DefaultRegistryURL,
		httpClient: &http.Client{
			Timeout: DefaultTimeout,
		},
		cache:    make(map[string]CacheEntry),
		cacheTTL: DefaultCacheTTL,
	}

	for _, opt := range opts {
		opt(c)
	}

	return c
}

// List returns capsules matching the given options.
func (c *HTTPClient) List(ctx context.Context, opts ListOptions) (*CapsuleListResponse, error) {
	// Build query string
	params := url.Values{}
	if opts.Query != "" {
		params.Set("q", opts.Query)
	}
	if opts.Type != "" {
		params.Set("type", opts.Type)
	}
	if opts.Platform != "" {
		params.Set("platform", opts.Platform)
	}
	if opts.Limit > 0 {
		params.Set("limit", strconv.Itoa(opts.Limit))
	}
	if opts.Offset > 0 {
		params.Set("offset", strconv.Itoa(opts.Offset))
	}

	endpoint := fmt.Sprintf("%s/v1/capsules?%s", c.baseURL, params.Encode())

	// Check cache
	cacheKey := "list:" + params.Encode()
	if cached := c.getFromCache(cacheKey); cached != nil {
		if resp, ok := cached.(*CapsuleListResponse); ok {
			return resp, nil
		}
	}

	// Make request
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("executing request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(body))
	}

	var result CapsuleListResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	// Cache result
	c.setCache(cacheKey, &result)

	return &result, nil
}

// Get returns the full manifest for a capsule.
func (c *HTTPClient) Get(ctx context.Context, name string) (*capsule.CapsuleManifest, error) {
	endpoint := fmt.Sprintf("%s/v1/capsules/%s", c.baseURL, url.PathEscape(name))

	// Check cache
	cacheKey := "get:" + name
	if cached := c.getFromCache(cacheKey); cached != nil {
		if manifest, ok := cached.(*capsule.CapsuleManifest); ok {
			return manifest, nil
		}
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("executing request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("capsule not found: %s", name)
	}
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(body))
	}

	var manifest capsule.CapsuleManifest
	if err := json.NewDecoder(resp.Body).Decode(&manifest); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	c.setCache(cacheKey, &manifest)

	return &manifest, nil
}

// GetVersion returns the manifest for a specific version.
func (c *HTTPClient) GetVersion(ctx context.Context, name, version string) (*capsule.CapsuleManifest, error) {
	endpoint := fmt.Sprintf("%s/v1/capsules/%s?version=%s",
		c.baseURL, url.PathEscape(name), url.QueryEscape(version))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("executing request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("capsule version not found: %s@%s", name, version)
	}
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(body))
	}

	var manifest capsule.CapsuleManifest
	if err := json.NewDecoder(resp.Body).Decode(&manifest); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return &manifest, nil
}

// GetDownloadInfo returns download information for a capsule.
func (c *HTTPClient) GetDownloadInfo(ctx context.Context, name, version, platform string) (*DownloadInfo, error) {
	params := url.Values{}
	if version != "" {
		params.Set("version", version)
	}
	if platform != "" {
		params.Set("platform", platform)
	}

	endpoint := fmt.Sprintf("%s/v1/capsules/%s/download?%s",
		c.baseURL, url.PathEscape(name), params.Encode())

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("executing request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("capsule not found: %s", name)
	}
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(body))
	}

	var info DownloadInfo
	if err := json.NewDecoder(resp.Body).Decode(&info); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return &info, nil
}

// Download downloads and extracts a capsule to the destination path.
func (c *HTTPClient) Download(ctx context.Context, name, version, destPath string) error {
	// Detect platform
	platform := detectPlatform()

	// Get download info
	info, err := c.GetDownloadInfo(ctx, name, version, platform)
	if err != nil {
		return fmt.Errorf("getting download info: %w", err)
	}

	// Create destination directory
	if err := os.MkdirAll(destPath, 0755); err != nil {
		return fmt.Errorf("creating destination directory: %w", err)
	}

	// Download archive
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, info.URL, nil)
	if err != nil {
		return fmt.Errorf("creating download request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("downloading archive: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("download failed with status %d", resp.StatusCode)
	}

	// Save to temp file
	tempFile := filepath.Join(destPath, ".download.tar.gz")
	f, err := os.Create(tempFile)
	if err != nil {
		return fmt.Errorf("creating temp file: %w", err)
	}

	_, err = io.Copy(f, resp.Body)
	f.Close()
	if err != nil {
		os.Remove(tempFile)
		return fmt.Errorf("writing archive: %w", err)
	}

	// TODO: Verify checksum
	// TODO: Extract archive

	// Cleanup
	os.Remove(tempFile)

	return nil
}

// ListVersions returns all available versions of a capsule.
func (c *HTTPClient) ListVersions(ctx context.Context, name string) (*VersionListResponse, error) {
	endpoint := fmt.Sprintf("%s/v1/capsules/%s/versions", c.baseURL, url.PathEscape(name))

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, fmt.Errorf("creating request: %w", err)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("executing request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, fmt.Errorf("capsule not found: %s", name)
	}
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("unexpected status %d: %s", resp.StatusCode, string(body))
	}

	var result VersionListResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("decoding response: %w", err)
	}

	return &result, nil
}

// Cache helpers

func (c *HTTPClient) getFromCache(key string) interface{} {
	c.cacheMu.RLock()
	defer c.cacheMu.RUnlock()

	entry, ok := c.cache[key]
	if !ok || time.Now().After(entry.ExpiresAt) {
		return nil
	}
	return entry.Data
}

func (c *HTTPClient) setCache(key string, data interface{}) {
	c.cacheMu.Lock()
	defer c.cacheMu.Unlock()

	c.cache[key] = CacheEntry{
		Data:      data,
		ExpiresAt: time.Now().Add(c.cacheTTL),
	}
}

// detectPlatform returns the current platform identifier.
func detectPlatform() string {
	// TODO: Use runtime.GOOS and runtime.GOARCH properly
	return "darwin-arm64"
}

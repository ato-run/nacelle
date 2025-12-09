// Package registry provides a client for the Gumball Capsule Registry API.
package registry

import "time"

// CapsuleSummary is a lightweight representation of a Capsule for listing.
type CapsuleSummary struct {
	Name        string   `json:"name"`
	Version     string   `json:"version"`
	Type        string   `json:"type"`
	DisplayName string   `json:"display_name"`
	Description string   `json:"description"`
	Author      string   `json:"author"`
	Tags        []string `json:"tags"`
	Platforms   []string `json:"platforms"`
	Downloads   int      `json:"downloads"`
	UpdatedAt   string   `json:"updated_at"`
}

// CapsuleListResponse is the response from the list capsules endpoint.
type CapsuleListResponse struct {
	Capsules []CapsuleSummary `json:"capsules"`
	Total    int              `json:"total"`
	Limit    int              `json:"limit"`
	Offset   int              `json:"offset"`
}

// VersionInfo represents a specific version of a Capsule.
type VersionInfo struct {
	Version   string `json:"version"`
	CreatedAt string `json:"created_at"`
	IsLatest  bool   `json:"is_latest"`
}

// VersionListResponse is the response from the list versions endpoint.
type VersionListResponse struct {
	Name     string        `json:"name"`
	Versions []VersionInfo `json:"versions"`
}

// DownloadInfo contains information needed to download a Capsule.
type DownloadInfo struct {
	URL           string `json:"url"`
	Checksum      string `json:"checksum"`
	SizeBytes     int64  `json:"size_bytes"`
	ExpiresAt     string `json:"expires_at"`
	CloudEndpoint string `json:"cloud_endpoint,omitempty"` // Optional cloud API endpoint
}

// ListOptions configures the capsule listing request.
type ListOptions struct {
	Query    string
	Type     string // inference, tool, app
	Platform string // darwin-arm64, linux-amd64, etc.
	Limit    int
	Offset   int
}

// CacheEntry stores a cached capsule with expiration.
type CacheEntry struct {
	Data      interface{}
	ExpiresAt time.Time
}

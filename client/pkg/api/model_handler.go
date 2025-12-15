package api

import (
	"log"
	"net/http"
)

// FetchModelRequest represents the HTTP request body for model fetching
type FetchModelRequest struct {
	URL         string `json:"url"`
	Destination string `json:"destination"`
	RigID       string `json:"rig_id,omitempty"` // Optional: target specific rig
}

// FetchModelResponse represents the HTTP response
type FetchModelResponse struct {
	Success         bool   `json:"success"`
	Message         string `json:"message"`
	BytesDownloaded uint64 `json:"bytes_downloaded"`
}

// HandleFetchModel handles POST /api/v1/models/fetch
// This endpoint instructs a specific Agent to download a model file
func HandleFetchModel(w http.ResponseWriter, r *http.Request) {
	// Stubbed out for Phase 4-A migration
	log.Println("HandleFetchModel called but not implemented (migrating to Phase 4)")
	http.Error(w, "Not implemented in Phase 4", http.StatusNotImplemented)
}

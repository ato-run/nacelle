package api

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"time"

	pb "github.com/onescluster/coordinator/pkg/proto"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
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
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// 1. Parse request body
	body, err := io.ReadAll(r.Body)
	if err != nil {
		log.Printf("Failed to read request body: %v", err)
		http.Error(w, "Failed to read request body", http.StatusBadRequest)
		return
	}
	defer r.Body.Close()

	var req FetchModelRequest
	if err := json.Unmarshal(body, &req); err != nil {
		log.Printf("Failed to unmarshal request: %v", err)
		http.Error(w, "Invalid JSON", http.StatusBadRequest)
		return
	}

	// 2. Validate request
	if req.URL == "" {
		http.Error(w, "URL is required", http.StatusBadRequest)
		return
	}
	if req.Destination == "" {
		http.Error(w, "Destination is required", http.StatusBadRequest)
		return
	}
	if req.RigID == "" {
		http.Error(w, "RigID is required", http.StatusBadRequest)
		return
	}

	// 3. Connect to target Agent's gRPC server
	// Note: In production, we should get the Agent's address from the database
	// For MVP, we'll assume the Agent is running on localhost:50051
	agentAddr := fmt.Sprintf("%s:50051", req.RigID)
	conn, err := grpc.NewClient(agentAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		log.Printf("Failed to connect to agent %s: %v", agentAddr, err)
		http.Error(w, fmt.Sprintf("Failed to connect to agent: %v", err), http.StatusInternalServerError)
		return
	}
	defer conn.Close()

	// 4. Create gRPC client and call FetchModel
	client := pb.NewAgentServiceClient(conn)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute) // 5 min timeout for large files
	defer cancel()

	grpcReq := &pb.FetchModelRequest{
		Url:         req.URL,
		Destination: req.Destination,
	}

	resp, err := client.FetchModel(ctx, grpcReq)
	if err != nil {
		log.Printf("FetchModel RPC failed: %v", err)
		http.Error(w, fmt.Sprintf("FetchModel RPC failed: %v", err), http.StatusInternalServerError)
		return
	}

	// 5. Return response
	httpResp := FetchModelResponse{
		Success:         resp.Success,
		Message:         resp.Message,
		BytesDownloaded: resp.BytesDownloaded,
	}

	w.Header().Set("Content-Type", "application/json")
	if !resp.Success {
		w.WriteHeader(http.StatusInternalServerError)
	} else {
		w.WriteHeader(http.StatusOK)
	}

	if err := json.NewEncoder(w).Encode(httpResp); err != nil {
		log.Printf("Failed to encode response: %v", err)
	}
}

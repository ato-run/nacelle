package api

import (
	"encoding/json"
	"log"
	"net/http"

	"github.com/onescluster/coordinator/pkg/db"
)

type RuntimeHandler struct {
	sm *db.StateManager
}

func NewRuntimeHandler(sm *db.StateManager) *RuntimeHandler {
	return &RuntimeHandler{sm: sm}
}

// GET /api/v1/runtimes
func (h *RuntimeHandler) List(w http.ResponseWriter, r *http.Request) {
	// For now, return static list from registry
	// TODO: Fetch from ArtifactRegistry dynamically
	runtimes := []map[string]interface{}{
		{
			"id":             "llama-server",
			"name":           "gumball-llama-server",
			"display_name":   "Llama.cpp Server",
			"description":    "High-performance LLM inference server",
			"latest_version": "b3500",
			"icon_url":       "/icons/llama.png",
			"categories":     []string{"llm", "inference"},
			"requirements": map[string]interface{}{
				"min_vram_gb": 4,
				"platforms":   []string{"darwin-arm64", "linux-x86_64"},
			},
		},
		{
			"id":             "flux-webui",
			"name":           "gumball-flux-webui",
			"display_name":   "Flux.1 WebUI",
			"description":    "Image generation with Flux.1 model",
			"latest_version": "1.0.0",
			"icon_url":       "/icons/flux.png",
			"categories":     []string{"image-gen", "diffusion"},
			"requirements": map[string]interface{}{
				"min_vram_gb": 12,
				"platforms":   []string{"linux-x86_64"},
			},
		},
	}

	w.Header().Set("Content-Type", "application/json")
	if err := json.NewEncoder(w).Encode(runtimes); err != nil {
		log.Printf("Failed to encode runtimes: %v", err)
	}
}

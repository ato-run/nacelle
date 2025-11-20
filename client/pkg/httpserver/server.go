package httpserver

import (
	"context"
	"embed"
	"fmt"
	"io/fs"
	"log"
	"net/http"
	"time"

	"github.com/onescluster/coordinator/pkg/api"
)

//go:embed web/*
var webFS embed.FS

// Server represents the HTTP server for the coordinator UI and API
type Server struct {
	addr          string
	healthHandler *api.HealthHandler
	server        *http.Server
}

// Config holds HTTP server configuration
type Config struct {
	Addr string
}

// NewServer creates a new HTTP server instance
func NewServer(cfg Config) *Server {
	healthHandler := api.NewHealthHandler()

	s := &Server{
		addr:          cfg.Addr,
		healthHandler: healthHandler,
	}

	mux := http.NewServeMux()

	// Serve static web UI from embedded filesystem
	webRoot, err := fs.Sub(webFS, "web")
	if err == nil {
		mux.Handle("/", http.FileServer(http.FS(webRoot)))
	} else {
		log.Printf("Warning: Failed to load embedded web files: %v", err)
		mux.HandleFunc("/", s.handleRoot)
	}

	// Health endpoints
	mux.HandleFunc("/health", healthHandler.HandleHealth)
	mux.HandleFunc("/ready", healthHandler.HandleReadiness)
	mux.HandleFunc("/live", healthHandler.HandleLiveness)

	// API endpoints
	mux.HandleFunc("/api/v1/models/fetch", api.HandleFetchModel)

	s.server = &http.Server{
		Addr:         cfg.Addr,
		Handler:      mux,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 15 * time.Second,
		IdleTimeout:  60 * time.Second,
	}

	return s
}

// Start starts the HTTP server
func (s *Server) Start() error {
	log.Printf("Starting HTTP server on %s", s.addr)
	if err := s.server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
		return fmt.Errorf("HTTP server error: %w", err)
	}
	return nil
}

// Shutdown gracefully shuts down the HTTP server
func (s *Server) Shutdown(ctx context.Context) error {
	log.Println("Shutting down HTTP server...")
	return s.server.Shutdown(ctx)
}

// handleRoot serves a minimal HTML page if embedded files are not available
func (s *Server) handleRoot(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path != "/" {
		http.NotFound(w, r)
		return
	}

	html := `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Capsuled Coordinator</title>
</head>
<body>
    <nav>
        <h1>Capsuled Coordinator</h1>
    </nav>
    <main>
        <h2>System Status</h2>
        <p>Status: Operational</p>
        <h2>API Endpoints</h2>
        <ul>
            <li><a href="/health">Health Check</a></li>
            <li><a href="/ready">Readiness Check</a></li>
            <li><a href="/api/nodes">Node Status API</a></li>
        </ul>
    </main>
</body>
</html>`

	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.WriteHeader(http.StatusOK)
	w.Write([]byte(html))
}

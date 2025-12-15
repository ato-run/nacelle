package router

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"
)

// DBClientRouterInterface defines DB needs for Router
type DBClientRouterInterface interface {
	GetCapsuleInternalURL(ctx context.Context, userID, capsuleName string) (string, error)
	GetCustomDomainCapsule(ctx context.Context, domain string) (userID, capsuleName string, err error)
}

type EdgeRouter struct {
	db DBClientRouterInterface

	dashboardURL *url.URL
	apiURL       *url.URL
}

type EdgeRouterConfig struct {
	DashboardURL string
	APIURL       string
}

func NewEdgeRouter(db DBClientRouterInterface, cfg EdgeRouterConfig) *EdgeRouter {
	dashboardURL := cfg.DashboardURL
	if dashboardURL == "" {
		dashboardURL = "http://dashboard:3000"
	}
	apiURL := cfg.APIURL
	if apiURL == "" {
		apiURL = "http://localhost:8080"
	}

	dURL, _ := url.Parse(dashboardURL)
	aURL, _ := url.Parse(apiURL)

	return &EdgeRouter{
		db:           db,
		dashboardURL: dURL,
		apiURL:       aURL,
	}
}

func (r *EdgeRouter) ServeHTTP(w http.ResponseWriter, req *http.Request) {
	host := req.Host
	// Remove port if present
	if idx := strings.Index(host, ":"); idx != -1 {
		host = host[:idx]
	}

	// Pattern Matching
	switch {
	case host == "gum-ball.app" || host == "www.gum-ball.app":
		r.serveMarketing(w, req)

	case host == "app.gum-ball.app":
		r.serveDashboard(w, req)

	case host == "api.gum-ball.app":
		r.serveAPI(w, req)

	case strings.HasSuffix(host, ".gum-ball.app"):
		r.serveUserCapsule(w, req, host)

	default:
		// Custom Domain
		r.serveCustomDomain(w, req, host)
	}
}

func (r *EdgeRouter) serveMarketing(w http.ResponseWriter, req *http.Request) {
	// Proxy to Cloudflare Pages or Serve Static
	// Mocking for now
	_, _ = w.Write([]byte("Gumball Cloud Marketing Page"))
}

func (r *EdgeRouter) serveDashboard(w http.ResponseWriter, req *http.Request) {
	// Proxy to Next.js Dashboard
	if r.dashboardURL == nil {
		http.Error(w, "Dashboard upstream not configured", http.StatusBadGateway)
		return
	}
	r.proxyTo(w, req, r.dashboardURL)
}

func (r *EdgeRouter) serveAPI(w http.ResponseWriter, req *http.Request) {
	// Proxy to Local API Handler (or handle directly if embedded)
	// Here we assume API handles paths under /v1
	if r.apiURL == nil {
		http.Error(w, "API upstream not configured", http.StatusBadGateway)
		return
	}
	r.proxyTo(w, req, r.apiURL)
}

func (r *EdgeRouter) serveUserCapsule(w http.ResponseWriter, req *http.Request, host string) {
	// {capsule}.{user}.gum-ball.app or {user}.gum-ball.app?
	// Requirements say: {user-id}.gum-ball.app or {capsule}.{user-id}.gum-ball.app

	base := strings.TrimSuffix(host, ".gum-ball.app")
	parts := strings.Split(base, ".")

	var userID, capsuleName string

	switch len(parts) {
	case 1:
		// {user}.gum-ball.app -> User Control Plane
		userID = parts[0]
		_, _ = w.Write([]byte(fmt.Sprintf("User Control Plane for %s", userID)))
		return
	case 2:
		// {capsule}.{user}.gum-ball.app -> capsule proxy
		capsuleName = parts[0]
		userID = parts[1]
	default:
		http.Error(w, "Invalid domain", http.StatusBadRequest)
		return
	}

	// Get Backend Address
	internalURL, err := r.db.GetCapsuleInternalURL(req.Context(), userID, capsuleName)
	if err != nil {
		http.Error(w, "Capsule not found", http.StatusNotFound)
		return
	}

	target, err := url.Parse(internalURL)
	if err != nil {
		http.Error(w, "Invalid Upstream URL", http.StatusInternalServerError)
		return
	}

	r.proxyTo(w, req, target)
}

func (r *EdgeRouter) serveCustomDomain(w http.ResponseWriter, req *http.Request, host string) {
	// Check Custom Domain
	userID, capsuleName, err := r.db.GetCustomDomainCapsule(req.Context(), host)
	if err != nil {
		http.Error(w, "Domain not configured", http.StatusNotFound)
		return
	}

	internalURL, err := r.db.GetCapsuleInternalURL(req.Context(), userID, capsuleName)
	if err != nil {
		http.Error(w, "Capsule not found", http.StatusNotFound)
		return
	}
	target, err := url.Parse(internalURL)
	if err != nil {
		http.Error(w, "Invalid Upstream URL", http.StatusInternalServerError)
		return
	}
	r.proxyTo(w, req, target)
}

func (r *EdgeRouter) proxyTo(w http.ResponseWriter, req *http.Request, target *url.URL) {
	proxy := httputil.NewSingleHostReverseProxy(target)

	originalDirector := proxy.Director
	proxy.Director = func(outReq *http.Request) {
		originalDirector(outReq)
		outReq.Host = target.Host
	}

	proxy.ServeHTTP(w, req)
}

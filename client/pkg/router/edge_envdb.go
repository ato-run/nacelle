package router

import (
	"context"
	"errors"
	"fmt"
	"net/url"
	"os"
	"strings"
)

var (
	ErrEdgeRouteNotFound  = errors.New("edge route not found")
	ErrEdgeDomainNotFound = errors.New("edge custom domain not found")
)

// EnvEdgeRouterDB is a minimal DB adapter for EdgeRouter.
// It is intentionally simple for Phase 2 prototyping.
//
// Keys:
//   - routes: "{user}/{capsule}" -> "http(s)://host:port"
//   - domains: "custom.domain" -> "{user}/{capsule}"
type EnvEdgeRouterDB struct {
	routes  map[string]string
	domains map[string]string
}

func NewEnvEdgeRouterDB(routes map[string]string, domains map[string]string) *EnvEdgeRouterDB {
	// Defensive copy to keep immutability expectations.
	r := make(map[string]string, len(routes))
	for k, v := range routes {
		r[k] = v
	}
	d := make(map[string]string, len(domains))
	for k, v := range domains {
		d[k] = v
	}
	return &EnvEdgeRouterDB{routes: r, domains: d}
}

func NewEnvEdgeRouterDBFromEnv() (*EnvEdgeRouterDB, error) {
	routes, err := ParseEdgeRouteMap(os.Getenv("GUMBALL_EDGE_ROUTE_MAP"))
	if err != nil {
		return nil, err
	}
	domains, err := ParseEdgeCustomDomainMap(os.Getenv("GUMBALL_EDGE_CUSTOM_DOMAIN_MAP"))
	if err != nil {
		return nil, err
	}
	return NewEnvEdgeRouterDB(routes, domains), nil
}

func (db *EnvEdgeRouterDB) GetCapsuleInternalURL(ctx context.Context, userID, capsuleName string) (string, error) {
	_ = ctx
	key := userID + "/" + capsuleName
	if v, ok := db.routes[key]; ok {
		return v, nil
	}
	return "", ErrEdgeRouteNotFound
}

func (db *EnvEdgeRouterDB) GetCustomDomainCapsule(ctx context.Context, domain string) (string, string, error) {
	_ = ctx
	ref, ok := db.domains[strings.ToLower(strings.TrimSpace(domain))]
	if !ok {
		return "", "", ErrEdgeDomainNotFound
	}
	user, capsule, ok := strings.Cut(ref, "/")
	if !ok || user == "" || capsule == "" {
		return "", "", fmt.Errorf("invalid custom domain mapping value: %q", ref)
	}
	return user, capsule, nil
}

// ParseEdgeRouteMap parses lines of "user/capsule=url".
// Empty lines and lines starting with '#' are ignored.
func ParseEdgeRouteMap(raw string) (map[string]string, error) {
	out := map[string]string{}
	for i, line := range strings.Split(raw, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		key, value, ok := strings.Cut(line, "=")
		if !ok {
			return nil, fmt.Errorf("invalid route map line %d: missing '='", i+1)
		}
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)
		if key == "" || value == "" {
			return nil, fmt.Errorf("invalid route map line %d: empty key/value", i+1)
		}
		// Minimal URL validation.
		u, err := url.Parse(value)
		if err != nil || u.Scheme == "" || u.Host == "" {
			return nil, fmt.Errorf("invalid route map line %d: invalid url %q", i+1, value)
		}
		out[key] = value
	}
	return out, nil
}

// ParseEdgeCustomDomainMap parses lines of "domain=user/capsule".
// Empty lines and lines starting with '#' are ignored.
func ParseEdgeCustomDomainMap(raw string) (map[string]string, error) {
	out := map[string]string{}
	for i, line := range strings.Split(raw, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		key, value, ok := strings.Cut(line, "=")
		if !ok {
			return nil, fmt.Errorf("invalid custom domain map line %d: missing '='", i+1)
		}
		key = strings.ToLower(strings.TrimSpace(key))
		value = strings.TrimSpace(value)
		if key == "" || value == "" {
			return nil, fmt.Errorf("invalid custom domain map line %d: empty key/value", i+1)
		}
		out[key] = value
	}
	return out, nil
}

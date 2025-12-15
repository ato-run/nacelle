package router

import (
	"context"
	"fmt"
	"strings"

	"github.com/onescluster/coordinator/pkg/store"
)

type StoreEdgeRouterConfig struct {
	PreferLocalhost bool
}

// StoreEdgeRouterDB resolves routing using the Coordinator's SQLite store.
//
// Current strategy (Phase 2 prototype):
// - identify a deployed capsule ID using common naming conventions
// - if a port is recorded, return http://127.0.0.1:{port}
// - otherwise, fall back to the persisted URL
type StoreEdgeRouterDB struct {
	store *store.SQLiteStore
	cfg   StoreEdgeRouterConfig
}

func NewStoreEdgeRouterDB(s *store.SQLiteStore, cfg StoreEdgeRouterConfig) *StoreEdgeRouterDB {
	return &StoreEdgeRouterDB{store: s, cfg: cfg}
}

func (db *StoreEdgeRouterDB) GetCapsuleInternalURL(ctx context.Context, userID, capsuleName string) (string, error) {
	if db.store == nil {
		return "", fmt.Errorf("store not configured")
	}

	// Try a few plausible IDs.
	candidates := []string{
		userID + "." + capsuleName, // Phase 2 naming rule: {user}.{capsule}
		capsuleName + "." + userID, // legacy local naming: {capsule}.{user}
		userID + "/" + capsuleName, // legacy env mapping fallback
		capsuleName,                // bare capsule id
	}
	for _, id := range candidates {
		id = strings.TrimSpace(id)
		if id == "" {
			continue
		}
		c, err := db.store.GetDeployedCapsule(ctx, id)
		if err != nil {
			return "", err
		}
		if c == nil {
			continue
		}
		if db.cfg.PreferLocalhost && c.Port > 0 {
			return fmt.Sprintf("http://127.0.0.1:%d", c.Port), nil
		}
		if c.URL != "" {
			return c.URL, nil
		}
		break
	}
	return "", ErrEdgeRouteNotFound
}

func (db *StoreEdgeRouterDB) GetCustomDomainCapsule(ctx context.Context, domain string) (string, string, error) {
	// Custom domain mapping is not persisted in the local SQLite store yet.
	// Keep behavior explicit rather than guessing.
	_ = ctx
	_ = domain
	return "", "", ErrEdgeDomainNotFound
}

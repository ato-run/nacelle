package router

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/store"
)

func TestStoreEdgeRouterDB_ResolvesInternalURL_FromPort(t *testing.T) {
	tmpDir, err := os.MkdirTemp("", "gumball-router-storedb-*")
	if err != nil {
		t.Fatalf("MkdirTemp: %v", err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(tmpDir) })

	dbPath := filepath.Join(tmpDir, "gumball.db")
	s, err := store.NewSQLiteStore(dbPath)
	if err != nil {
		t.Fatalf("NewSQLiteStore: %v", err)
	}
	t.Cleanup(func() { _ = s.Close() })

	ctx := context.Background()
	if err := s.SaveDeployedCapsule(ctx, &store.DeployedCapsule{
		ID:        "alice.myapp",
		Name:      "myapp",
		URL:       "http://myapp.alice.gum-ball.app",
		Status:    "Running",
		Port:      18080,
		CreatedAt: time.Now(),
	}); err != nil {
		t.Fatalf("SaveDeployedCapsule: %v", err)
	}

	rdb := NewStoreEdgeRouterDB(s, StoreEdgeRouterConfig{PreferLocalhost: true})

	got, err := rdb.GetCapsuleInternalURL(ctx, "alice", "myapp")
	if err != nil {
		t.Fatalf("GetCapsuleInternalURL: %v", err)
	}
	if got != "http://127.0.0.1:18080" {
		t.Fatalf("url = %q, want %q", got, "http://127.0.0.1:18080")
	}
}

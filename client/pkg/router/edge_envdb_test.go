package router

import (
	"context"
	"testing"
)

func TestParseEdgeRouteMap_AllowsBasicPairs(t *testing.T) {
	got, err := ParseEdgeRouteMap("alice/myapp=http://127.0.0.1:18080\n")
	if err != nil {
		t.Fatalf("err = %v", err)
	}
	if got["alice/myapp"] != "http://127.0.0.1:18080" {
		t.Fatalf("map entry = %q", got["alice/myapp"])
	}
}

func TestParseEdgeRouteMap_IgnoresBlankAndCommentLines(t *testing.T) {
	got, err := ParseEdgeRouteMap("\n# comment\nalice/myapp=http://127.0.0.1:18080\n\n")
	if err != nil {
		t.Fatalf("err = %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("len = %d, want 1", len(got))
	}
}

func TestEnvEdgeRouterDB_GetCapsuleInternalURL_NotFound(t *testing.T) {
	db := NewEnvEdgeRouterDB(map[string]string{}, map[string]string{})
	_, err := db.GetCapsuleInternalURL(context.Background(), "alice", "missing")
	if err == nil {
		t.Fatalf("expected error")
	}
}

func TestEnvEdgeRouterDB_CustomDomain_ResolvesToUserCapsule(t *testing.T) {
	db := NewEnvEdgeRouterDB(
		map[string]string{"alice/myapp": "http://127.0.0.1:18080"},
		map[string]string{"alice.example.com": "alice/myapp"},
	)

	user, capsule, err := db.GetCustomDomainCapsule(context.Background(), "alice.example.com")
	if err != nil {
		t.Fatalf("err = %v", err)
	}
	if user != "alice" {
		t.Fatalf("user = %q, want %q", user, "alice")
	}
	if capsule != "myapp" {
		t.Fatalf("capsule = %q, want %q", capsule, "myapp")
	}
}

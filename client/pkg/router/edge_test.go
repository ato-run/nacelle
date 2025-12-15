package router

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

type fakeEdgeRouterDB struct {
	internalURLByUserAndCapsule map[string]string
	customDomainToUserCapsule   map[string][2]string
}

func (f *fakeEdgeRouterDB) GetCapsuleInternalURL(ctx context.Context, userID, capsuleName string) (string, error) {
	key := userID + "/" + capsuleName
	if v, ok := f.internalURLByUserAndCapsule[key]; ok {
		return v, nil
	}
	return "", errNotFound
}

func (f *fakeEdgeRouterDB) GetCustomDomainCapsule(ctx context.Context, domain string) (string, string, error) {
	if v, ok := f.customDomainToUserCapsule[domain]; ok {
		return v[0], v[1], nil
	}
	return "", "", errNotFound
}

var errNotFound = &notFoundError{}

type notFoundError struct{}

func (e *notFoundError) Error() string { return "not found" }

func TestEdgeRouter_MarketingHost_ReturnsStub(t *testing.T) {
	r := NewEdgeRouter(&fakeEdgeRouterDB{}, EdgeRouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "http://gum-ball.app/", nil)
	req.Host = "gum-ball.app"
	w := httptest.NewRecorder()

	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}
	if got := w.Body.String(); got != "Gumball Cloud Marketing Page" {
		t.Fatalf("body = %q, want %q", got, "Gumball Cloud Marketing Page")
	}
}

func TestEdgeRouter_UserCapsuleHost_ProxiesToInternalURL(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Upstream", "ok")
		w.WriteHeader(http.StatusTeapot)
		_, _ = w.Write([]byte("proxied"))
	}))
	defer upstream.Close()

	db := &fakeEdgeRouterDB{
		internalURLByUserAndCapsule: map[string]string{
			"alice/myapp": upstream.URL,
		},
	}
	r := NewEdgeRouter(db, EdgeRouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "http://myapp.alice.gum-ball.app/hello", nil)
	req.Host = "myapp.alice.gum-ball.app"
	w := httptest.NewRecorder()

	r.ServeHTTP(w, req)

	if w.Code != http.StatusTeapot {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusTeapot)
	}
	if got := w.Header().Get("X-Upstream"); got != "ok" {
		t.Fatalf("X-Upstream = %q, want %q", got, "ok")
	}
	if got := w.Body.String(); got != "proxied" {
		t.Fatalf("body = %q, want %q", got, "proxied")
	}
}

func TestEdgeRouter_CustomDomain_ResolvesAndProxies(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("custom"))
	}))
	defer upstream.Close()

	db := &fakeEdgeRouterDB{
		internalURLByUserAndCapsule: map[string]string{
			"alice/myapp": upstream.URL,
		},
		customDomainToUserCapsule: map[string][2]string{
			"alice.example.com": {"alice", "myapp"},
		},
	}
	r := NewEdgeRouter(db, EdgeRouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "http://alice.example.com/", nil)
	req.Host = "alice.example.com"
	w := httptest.NewRecorder()

	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}
	if got := w.Body.String(); got != "custom" {
		t.Fatalf("body = %q, want %q", got, "custom")
	}
}

package reconcile

import (
	"context"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
)

type mockStore struct {
	desired   map[string]string
	workloads []*db.NodeWorkload

	failedCalls  []string
	pendingCalls []string
}

func (m *mockStore) ListDesiredWorkloads(context.Context) (map[string]string, error) {
	copyMap := make(map[string]string, len(m.desired))
	for k, v := range m.desired {
		copyMap[k] = v
	}
	return copyMap, nil
}

func (m *mockStore) ListNodeWorkloads(context.Context) ([]*db.NodeWorkload, error) {
	workloads := make([]*db.NodeWorkload, 0, len(m.workloads))
	for _, wl := range m.workloads {
		if wl == nil {
			continue
		}
		clone := *wl
		workloads = append(workloads, &clone)
	}
	return workloads, nil
}

func (m *mockStore) MarkCapsuleFailed(_ context.Context, capsuleID string) error {
	m.failedCalls = append(m.failedCalls, capsuleID)
	return nil
}

func (m *mockStore) MarkCapsulePending(_ context.Context, capsuleID string) error {
	m.pendingCalls = append(m.pendingCalls, capsuleID)
	return nil
}

func TestReconcilerDetectsOrphanWorkloads(t *testing.T) {
	store := &mockStore{
		desired: map[string]string{
			"wl-1": "node-a",
		},
		workloads: []*db.NodeWorkload{
			{NodeID: "node-a", WorkloadID: "wl-1", Phase: "running"},
			{NodeID: "node-b", WorkloadID: "wl-orphan", Phase: "running"},
		},
	}

	r := New(store, 0)
	if err := r.reconcileOnce(context.Background()); err != nil {
		t.Fatalf("reconcileOnce returned error: %v", err)
	}

	if len(store.failedCalls) != 1 || store.failedCalls[0] != "wl-orphan" {
		t.Fatalf("expected orphan workload to be marked failed, got %#v", store.failedCalls)
	}

	if len(store.pendingCalls) != 0 {
		t.Fatalf("expected no pending calls, got %#v", store.pendingCalls)
	}
}

func TestReconcilerDetectsMissingWorkloads(t *testing.T) {
	store := &mockStore{
		desired: map[string]string{
			"wl-scheduled": "node-a",
			"wl-present":   "node-a",
		},
		workloads: []*db.NodeWorkload{
			{NodeID: "node-a", WorkloadID: "wl-present", Phase: "running"},
		},
	}

	r := New(store, time.Second)
	if err := r.reconcileOnce(context.Background()); err != nil {
		t.Fatalf("reconcileOnce returned error: %v", err)
	}

	if len(store.pendingCalls) != 1 || store.pendingCalls[0] != "wl-scheduled" {
		t.Fatalf("expected missing workload to be marked pending, got %#v", store.pendingCalls)
	}

	if len(store.failedCalls) != 0 {
		t.Fatalf("expected no failed calls, got %#v", store.failedCalls)
	}
}

func TestReconcilerNoChangesWhenStatesMatch(t *testing.T) {
	store := &mockStore{
		desired: map[string]string{
			"wl-1": "node-a",
		},
		workloads: []*db.NodeWorkload{
			{NodeID: "node-a", WorkloadID: "wl-1", Phase: "running"},
		},
	}

	r := New(store, 0)
	if err := r.reconcileOnce(context.Background()); err != nil {
		t.Fatalf("reconcileOnce returned error: %v", err)
	}

	if len(store.pendingCalls) != 0 {
		t.Fatalf("expected no pending calls, got %#v", store.pendingCalls)
	}

	if len(store.failedCalls) != 0 {
		t.Fatalf("expected no failed calls, got %#v", store.failedCalls)
	}
}

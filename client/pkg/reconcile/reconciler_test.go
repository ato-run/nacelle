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

func TestReconcilerStartAndStop(t *testing.T) {
	store := &mockStore{
		desired:   map[string]string{},
		workloads: []*db.NodeWorkload{},
	}

	// Create reconciler with short interval for testing
	r := New(store, 50*time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	// Start reconciler
	stop := r.Start(ctx)

	// Wait for at least one reconciliation cycle
	time.Sleep(100 * time.Millisecond)

	// Stop reconciler
	stop()

	// Verify it can be stopped multiple times safely
	stop()
}

func TestReconcilerContextCancellation(t *testing.T) {
	store := &mockStore{
		desired:   map[string]string{},
		workloads: []*db.NodeWorkload{},
	}

	r := New(store, 50*time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	// Start reconciler
	r.Start(ctx)

	// Wait for context to cancel
	<-ctx.Done()

	// Give it a moment to clean up
	time.Sleep(50 * time.Millisecond)
}

func TestReconcilerDefaultInterval(t *testing.T) {
	store := &mockStore{
		desired:   map[string]string{},
		workloads: []*db.NodeWorkload{},
	}

	// Test with zero interval - should default to 30 seconds
	r := New(store, 0)
	if r.interval != 30*time.Second {
		t.Errorf("expected default interval 30s, got %v", r.interval)
	}

	// Test with negative interval - should default to 30 seconds
	r2 := New(store, -5*time.Second)
	if r2.interval != 30*time.Second {
		t.Errorf("expected default interval 30s, got %v", r2.interval)
	}

	// Test with explicit interval
	r3 := New(store, 5*time.Second)
	if r3.interval != 5*time.Second {
		t.Errorf("expected interval 5s, got %v", r3.interval)
	}
}

func TestReconcilerMultipleOrphansAndMissing(t *testing.T) {
	store := &mockStore{
		desired: map[string]string{
			"wl-1": "node-a",
			"wl-2": "node-b",
			"wl-3": "node-a",
		},
		workloads: []*db.NodeWorkload{
			{NodeID: "node-a", WorkloadID: "wl-1", Phase: "running"},
			{NodeID: "node-c", WorkloadID: "wl-orphan-1", Phase: "running"},
			{NodeID: "node-d", WorkloadID: "wl-orphan-2", Phase: "failed"},
		},
	}

	r := New(store, time.Second)
	if err := r.reconcileOnce(context.Background()); err != nil {
		t.Fatalf("reconcileOnce returned error: %v", err)
	}

	// Should mark 2 orphans as failed
	if len(store.failedCalls) != 2 {
		t.Errorf("expected 2 failed calls, got %d: %#v", len(store.failedCalls), store.failedCalls)
	}

	// Should mark 2 missing workloads as pending (wl-2 and wl-3)
	if len(store.pendingCalls) != 2 {
		t.Errorf("expected 2 pending calls, got %d: %#v", len(store.pendingCalls), store.pendingCalls)
	}
}

func TestReconcilerEmptyStates(t *testing.T) {
	t.Run("both_empty", func(t *testing.T) {
		store := &mockStore{
			desired:   map[string]string{},
			workloads: []*db.NodeWorkload{},
		}

		r := New(store, time.Second)
		if err := r.reconcileOnce(context.Background()); err != nil {
			t.Fatalf("reconcileOnce returned error: %v", err)
		}

		if len(store.failedCalls) != 0 || len(store.pendingCalls) != 0 {
			t.Error("expected no changes when both states are empty")
		}
	})

	t.Run("no_desired_workloads", func(t *testing.T) {
		store := &mockStore{
			desired: map[string]string{},
			workloads: []*db.NodeWorkload{
				{NodeID: "node-a", WorkloadID: "wl-1", Phase: "running"},
			},
		}

		r := New(store, time.Second)
		if err := r.reconcileOnce(context.Background()); err != nil {
			t.Fatalf("reconcileOnce returned error: %v", err)
		}

		// All actual workloads should be marked as orphans
		if len(store.failedCalls) != 1 {
			t.Errorf("expected 1 failed call, got %d", len(store.failedCalls))
		}
	})

	t.Run("no_actual_workloads", func(t *testing.T) {
		store := &mockStore{
			desired: map[string]string{
				"wl-1": "node-a",
			},
			workloads: []*db.NodeWorkload{},
		}

		r := New(store, time.Second)
		if err := r.reconcileOnce(context.Background()); err != nil {
			t.Fatalf("reconcileOnce returned error: %v", err)
		}

		// All desired workloads should be marked as missing
		if len(store.pendingCalls) != 1 {
			t.Errorf("expected 1 pending call, got %d", len(store.pendingCalls))
		}
	})
}

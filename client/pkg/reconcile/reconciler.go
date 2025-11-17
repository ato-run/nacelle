package reconcile

import (
	"context"
	"log"
	"sync"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
)

// Store defines the NodeStore operations required by the reconciler. Both the
// production NodeStore and lightweight mocks should satisfy this interface.
type Store interface {
	ListDesiredWorkloads(ctx context.Context) (map[string]string, error)
	ListNodeWorkloads(ctx context.Context) ([]*db.NodeWorkload, error)
	MarkCapsuleFailed(ctx context.Context, capsuleID string) error
	MarkCapsulePending(ctx context.Context, capsuleID string) error
}

type nodeStoreAdapter struct {
	store *db.NodeStore
}

func (a *nodeStoreAdapter) ListDesiredWorkloads(ctx context.Context) (map[string]string, error) {
	return a.store.ListDesiredWorkloads(ctx)
}

func (a *nodeStoreAdapter) ListNodeWorkloads(ctx context.Context) ([]*db.NodeWorkload, error) {
	return a.store.ListNodeWorkloads(ctx)
}

func (a *nodeStoreAdapter) MarkCapsuleFailed(ctx context.Context, capsuleID string) error {
	return a.store.MarkCapsuleFailed(ctx, capsuleID)
}

func (a *nodeStoreAdapter) MarkCapsulePending(ctx context.Context, capsuleID string) error {
	return a.store.MarkCapsulePending(ctx, capsuleID)
}

// Reconciler periodically compares desired coordinator state with the reality reported by Agents.
// It mitigates drift between reserved resources and actual workloads, preventing VRAM overcommit.
type Reconciler struct {
	store    Store
	interval time.Duration

	once   sync.Once
	stopCh chan struct{}

	// testSyncCh is used only in tests for synchronization (optional, can be nil)
	testSyncCh chan struct{}
	// testDoneCh signals when the reconciler goroutine exits (test-only)
	testDoneCh chan struct{}
}

// New creates a new reconciliation loop with the provided interval.
func New(store Store, interval time.Duration) *Reconciler {
	if interval <= 0 {
		interval = 30 * time.Second
	}
	return &Reconciler{
		store:    store,
		interval: interval,
		stopCh:   make(chan struct{}),
	}
}

// NewWithNodeStore is a helper for callers that already depend on db.NodeStore.
func NewWithNodeStore(nodeStore *db.NodeStore, interval time.Duration) *Reconciler {
	return New(&nodeStoreAdapter{store: nodeStore}, interval)
}

// Start launches the reconciliation loop. It returns a function that can be used to stop the loop.
func (r *Reconciler) Start(ctx context.Context) func() {
	if r.testDoneCh == nil && r.testSyncCh != nil {
		// For tests, create a channel to signal when goroutine exits
		r.testDoneCh = make(chan struct{})
	}

	r.once.Do(func() {
		go func() {
			r.run(ctx)
			if r.testDoneCh != nil {
				close(r.testDoneCh)
			}
		}()
	})

	return func() {
		select {
		case <-r.stopCh:
			return
		default:
			close(r.stopCh)
		}
		if r.testDoneCh != nil {
			// Wait for goroutine to exit when testDoneCh is set
			<-r.testDoneCh
		}
	}
}

func (r *Reconciler) run(ctx context.Context) {
	ticker := time.NewTicker(r.interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-r.stopCh:
			return
		case <-ticker.C:
			if err := r.reconcileOnce(ctx); err != nil {
				log.Printf("⚠️ Reconcile iteration failed: %v", err)
			}
			// Signal test synchronization if channel is set
			if r.testSyncCh != nil {
				select {
				case r.testSyncCh <- struct{}{}:
				default:
					// Non-blocking send to avoid deadlock
				}
			}
		}
	}
}

func (r *Reconciler) reconcileOnce(ctx context.Context) error {
	desired, err := r.store.ListDesiredWorkloads(ctx)
	if err != nil {
		return err
	}

	actual, err := r.store.ListNodeWorkloads(ctx)
	if err != nil {
		return err
	}

	// Detect orphan workloads (present on Agent, absent in Coordinator desired state)
	for _, wl := range actual {
		if _, ok := desired[wl.WorkloadID]; ok {
			delete(desired, wl.WorkloadID)
			continue
		}

		log.Printf("🧹 Orphan workload detected: node=%s workload=%s phase=%s", wl.NodeID, wl.WorkloadID, wl.Phase)
		if err := r.store.MarkCapsuleFailed(ctx, wl.WorkloadID); err != nil {
			log.Printf("⚠️ Failed to mark orphan workload %s as failed: %v", wl.WorkloadID, err)
		}
	}

	// Remaining desired workloads are missing on the Agent
	for workloadID := range desired {
		log.Printf("♻️ Workload missing on Agent, scheduling redeploy: workload=%s", workloadID)
		if err := r.store.MarkCapsulePending(ctx, workloadID); err != nil {
			log.Printf("⚠️ Failed to mark workload %s pending: %v", workloadID, err)
		}
	}

	return nil
}

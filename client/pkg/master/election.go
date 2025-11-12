package master

import (
	"bytes"
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/oklog/ulid/v2"
)

// ElectionStateStore captures the minimal operations the elector needs from the
// state manager. Defining an interface allows tests to provide lightweight mocks
// without depending on the concrete database-backed implementation.
type ElectionStateStore interface {
	SetMaster(masterID string) error
	ExecuteRaw(query string, args ...interface{}) error
}

// HeadscaleQuorumClient exposes the headscale operations required for quorum
// calculations during master election.
type HeadscaleQuorumClient interface {
	GetQuorumSize(ctx context.Context) (int, error)
}

// Elector manages master election for the cluster
type Elector struct {
	nodeID         string
	stateMgr       ElectionStateStore
	headscaleAPI   HeadscaleQuorumClient
	isMaster       bool
	masterID       string
	mu             sync.RWMutex
	retryAttempts  int
	maxRetries     int
	retryDelay     time.Duration
	degradedMode   bool
	lastElectionAt time.Time
}

// ElectorConfig contains configuration for the elector
type ElectorConfig struct {
	NodeID       string
	StateManager ElectionStateStore
	HeadscaleAPI HeadscaleQuorumClient
	MaxRetries   int
	RetryDelay   time.Duration
}

// NewElector creates a new master elector
func NewElector(cfg ElectorConfig) *Elector {
	if cfg.MaxRetries == 0 {
		cfg.MaxRetries = 3
	}
	if cfg.RetryDelay == 0 {
		cfg.RetryDelay = 5 * time.Second
	}

	return &Elector{
		nodeID:       cfg.NodeID,
		stateMgr:     cfg.StateManager,
		headscaleAPI: cfg.HeadscaleAPI,
		maxRetries:   cfg.MaxRetries,
		retryDelay:   cfg.RetryDelay,
	}
}

// ElectMaster performs master election based on ULID comparison
// Returns the elected master's node ID
func (e *Elector) ElectMaster(ctx context.Context, aliveNodes []string) (string, error) {
	e.mu.Lock()
	defer e.mu.Unlock()

	log.Printf("Starting master election. AliveNodes=%d Candidates=%v", len(aliveNodes), aliveNodes)

	if len(aliveNodes) == 0 {
		log.Printf("Master election aborted: no alive nodes available (self=%s)", e.nodeID)
		return "", fmt.Errorf("no alive nodes for election")
	}

	// Get quorum size from headscale
	quorumSize, err := e.getQuorumWithRetry(ctx)
	if err != nil {
		log.Printf("Warning: Failed to get quorum from headscale after %d attempt(s): %v", e.retryAttempts, err)
		// Enter degraded mode if we can't reach headscale
		if e.retryAttempts >= e.maxRetries {
			e.enterDegradedMode()
			return "", fmt.Errorf("entering degraded mode: %w", err)
		}
		return "", err
	}

	// Reset retry counter on success
	wasDegraded := e.degradedMode
	e.retryAttempts = 0
	e.degradedMode = false
	if wasDegraded {
		log.Printf("Recovered from degraded mode: headscale quorum restored")
	}

	// Check if we have quorum
	if len(aliveNodes) < (quorumSize/2 + 1) {
		required := quorumSize/2 + 1
		log.Printf("Warning: Insufficient nodes for quorum. Required: %d, Available: %d, AliveNodes=%v",
			required, len(aliveNodes), aliveNodes)
		return "", fmt.Errorf("insufficient nodes for quorum")
	}

	// Parse all node IDs as ULIDs and find the smallest
	var smallestID string
	var smallestULID ulid.ULID

	for i, nodeID := range aliveNodes {
		parsed, err := ulid.Parse(nodeID)
		if err != nil {
			log.Printf("Warning: Failed to parse node ID %s as ULID (index=%d): %v", nodeID, i, err)
			continue
		}

		if i == 0 || bytes.Compare(parsed[:], smallestULID[:]) < 0 {
			smallestULID = parsed
			smallestID = nodeID
		}
	}

	if smallestID == "" {
		log.Printf("Master election failed: no valid ULIDs found among candidates %v", aliveNodes)
		return "", fmt.Errorf("no valid ULID found among alive nodes")
	}

	// Update master status
	e.masterID = smallestID
	e.isMaster = (smallestID == e.nodeID)
	e.lastElectionAt = time.Now()

	// Persist election result to rqlite
	if err := e.stateMgr.SetMaster(smallestID); err != nil {
		log.Printf("Warning: Failed to persist master election: %v", err)
		// Continue anyway as in-memory state is updated
	}

	// Record election in history
	if err := e.recordElection(smallestID, len(aliveNodes), quorumSize); err != nil {
		log.Printf("Warning: Failed to record election history: %v", err)
	}

	log.Printf("Master election complete. Master: %s (IsMaster: %v, Quorum: %d, AliveNodes: %d)",
		smallestID, e.isMaster, quorumSize, len(aliveNodes))

	return smallestID, nil
}

// getQuorumWithRetry attempts to get quorum size with retry logic
func (e *Elector) getQuorumWithRetry(ctx context.Context) (int, error) {
	var lastErr error

	for attempt := 0; attempt <= e.maxRetries; attempt++ {
		if attempt > 0 {
			log.Printf("Retrying headscale API (attempt %d/%d)", attempt, e.maxRetries)
			time.Sleep(e.retryDelay)
		}

		quorum, err := e.headscaleAPI.GetQuorumSize(ctx)
		if err == nil {
			return quorum, nil
		}

		lastErr = err
		e.retryAttempts = attempt + 1
		log.Printf("Headscale quorum fetch failed (attempt=%d/%d): %v", e.retryAttempts, e.maxRetries, err)
	}

	return 0, fmt.Errorf("failed after %d attempts: %w", e.maxRetries, lastErr)
}

// enterDegradedMode puts the cluster into degraded operation mode
func (e *Elector) enterDegradedMode() {
	if e.degradedMode {
		return
	}
	e.degradedMode = true
	log.Printf("CRITICAL: Entering degraded mode due to headscale API failure (node=%s attempts=%d)", e.nodeID, e.retryAttempts)
	log.Printf("Degraded mode restrictions: No new deployments, existing operations continue")
}

// IsMaster returns whether this node is currently the master
func (e *Elector) IsMaster() bool {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.isMaster
}

// GetMasterID returns the current master's node ID
func (e *Elector) GetMasterID() string {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.masterID
}

// IsDegraded returns whether the cluster is in degraded mode
func (e *Elector) IsDegraded() bool {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.degradedMode
}

// recordElection records the election result in the database
func (e *Elector) recordElection(masterID string, aliveNodes, totalNodes int) error {
	// Using raw SQL as this is a simple insert for audit purposes
	query := fmt.Sprintf(`
		INSERT INTO master_elections (
			elected_master_id,
			alive_nodes_count,
			total_nodes_count,
			elected_at
		) VALUES ('%s', %d, %d, %d)
	`, masterID, aliveNodes, totalNodes, time.Now().Unix())

	return e.stateMgr.ExecuteRaw(query)
}

// ValidateAndRecoverMaster checks if current master is still valid
// If not, triggers a new election
func (e *Elector) ValidateAndRecoverMaster(ctx context.Context, aliveNodes []string) error {
	e.mu.RLock()
	currentMaster := e.masterID
	e.mu.RUnlock()

	if len(aliveNodes) == 0 {
		log.Printf("ValidateAndRecoverMaster invoked with no alive nodes. CurrentMaster=%s", currentMaster)
	}

	// Check if current master is in alive nodes list
	masterAlive := false
	for _, nodeID := range aliveNodes {
		if nodeID == currentMaster {
			masterAlive = true
			break
		}
	}

	if !masterAlive && currentMaster != "" {
		log.Printf("Master %s is no longer alive, triggering re-election", currentMaster)
		if _, err := e.ElectMaster(ctx, aliveNodes); err != nil {
			log.Printf("Warning: Re-election attempt after master loss failed: %v", err)
			return err
		}
	}

	return nil
}

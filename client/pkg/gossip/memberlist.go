package gossip

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/hashicorp/memberlist"
	"github.com/onescluster/coordinator/pkg/db"
)

// Manager manages the gossip protocol and node membership
// NodeStateStore defines the operations the gossip manager needs from the state manager.
type NodeStateStore interface {
	GetNode(id string) (*db.Node, bool)
	UpdateNode(node *db.Node) error
	GetActiveNodes() []*db.Node
}

// MasterElector defines the subset of the master elector API used by the gossip manager.
type MasterElector interface {
	ValidateAndRecoverMaster(ctx context.Context, aliveNodes []string) error
	ElectMaster(ctx context.Context, aliveNodes []string) (string, error)
}

type Manager struct {
	nodeID         string
	bindAddr       string
	list           *memberlist.Memberlist
	stateMgr       NodeStateStore
	elector        MasterElector
	nodeTimeout    time.Duration
	mu             sync.RWMutex
	ctx            context.Context
	cancel         context.CancelFunc
}

// Config contains configuration for the gossip manager
type Config struct {
	NodeID            string
	BindAddr          string
	Peers             []string
	StateManager      NodeStateStore
	Elector           MasterElector
	HeartbeatInterval time.Duration
	NodeTimeout       time.Duration
}

// NewManager creates a new gossip manager
func NewManager(cfg Config) (*Manager, error) {
	ctx, cancel := context.WithCancel(context.Background())

	mgr := &Manager{
		nodeID:      cfg.NodeID,
		bindAddr:    cfg.BindAddr,
		stateMgr:    cfg.StateManager,
		elector:     cfg.Elector,
		nodeTimeout: cfg.NodeTimeout,
		ctx:         ctx,
		cancel:      cancel,
	}

	// Create memberlist configuration
	mlConfig := memberlist.DefaultLANConfig()
	mlConfig.Name = cfg.NodeID
	mlConfig.BindAddr = cfg.BindAddr
	mlConfig.Events = &eventDelegate{manager: mgr}

	// Create memberlist
	list, err := memberlist.Create(mlConfig)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create memberlist: %w", err)
	}

	mgr.list = list

	// Join existing cluster if peers are provided
	if len(cfg.Peers) > 0 {
		log.Printf("Attempting to join cluster with peers: %v", cfg.Peers)
		n, err := list.Join(cfg.Peers)
		if err != nil {
			log.Printf("Warning: Failed to join cluster: %v", err)
		} else {
			log.Printf("Successfully joined cluster, contacted %d nodes", n)
		}
	}

	// Start heartbeat ticker
	if cfg.HeartbeatInterval > 0 {
		go mgr.heartbeatLoop(cfg.HeartbeatInterval)
	}

	return mgr, nil
}

// eventDelegate handles memberlist events
type eventDelegate struct {
	manager *Manager
}

// NotifyJoin is called when a node joins the cluster
func (e *eventDelegate) NotifyJoin(node *memberlist.Node) {
	log.Printf("Node joined: %s (%s)", node.Name, node.Addr)

	// Update node status in state manager
	if err := e.manager.updateNodeStatus(node.Name, "active"); err != nil {
		log.Printf("Failed to update node status: %v", err)
	}

	// Trigger master election validation
	e.manager.triggerElectionCheck()
}

// NotifyLeave is called when a node leaves the cluster gracefully
func (e *eventDelegate) NotifyLeave(node *memberlist.Node) {
	log.Printf("Node left: %s (%s)", node.Name, node.Addr)

	// Update node status in state manager
	if err := e.manager.updateNodeStatus(node.Name, "inactive"); err != nil {
		log.Printf("Failed to update node status: %v", err)
	}

	// Trigger master election validation
	e.manager.triggerElectionCheck()
}

// NotifyUpdate is called when a node's metadata is updated
func (e *eventDelegate) NotifyUpdate(node *memberlist.Node) {
	log.Printf("Node updated: %s (%s)", node.Name, node.Addr)
}

// GetAliveNodes returns a list of currently alive node IDs
func (m *Manager) GetAliveNodes() []string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if m.list == nil {
		return []string{m.nodeID} // At minimum, this node is alive
	}

	members := m.list.Members()
	nodeIDs := make([]string, 0, len(members))

	for _, member := range members {
		nodeIDs = append(nodeIDs, member.Name)
	}

	return nodeIDs
}

// GetMemberCount returns the number of known cluster members
func (m *Manager) GetMemberCount() int {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if m.list == nil {
		return 1
	}

	return m.list.NumMembers()
}

// updateNodeStatus updates a node's status in the state manager
func (m *Manager) updateNodeStatus(nodeID, status string) error {
	node, exists := m.stateMgr.GetNode(nodeID)
	if !exists {
		// Node not in state, might be newly discovered
		log.Printf("Node %s not in state manager, skipping status update", nodeID)
		return nil
	}

	node.Status = db.NodeStatus(status)
	node.LastSeen = time.Now()

	return m.stateMgr.UpdateNode(node)
}

// triggerElectionCheck triggers a master election validation
func (m *Manager) triggerElectionCheck() {
	go func() {
		aliveNodes := m.GetAliveNodes()
		log.Printf("Triggering election check with %d alive nodes", len(aliveNodes))

		ctx, cancel := context.WithTimeout(m.ctx, 30*time.Second)
		defer cancel()

		if err := m.elector.ValidateAndRecoverMaster(ctx, aliveNodes); err != nil {
			log.Printf("Election check failed: %v", err)
		}
	}()
}

// heartbeatLoop periodically sends heartbeats and checks for dead nodes
func (m *Manager) heartbeatLoop(interval time.Duration) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for {
		select {
		case <-m.ctx.Done():
			return
		case <-ticker.C:
			m.performHeartbeat()
		}
	}
}

// performHeartbeat updates this node's heartbeat and checks for dead nodes
func (m *Manager) performHeartbeat() {
	// Update own heartbeat in state
	node, exists := m.stateMgr.GetNode(m.nodeID)
	if exists {
		node.LastSeen = time.Now()
		if err := m.stateMgr.UpdateNode(node); err != nil {
			log.Printf("Failed to update own heartbeat: %v", err)
		}
	}

	// Check for nodes that haven't sent heartbeat within timeout
	m.checkDeadNodes()
}

// checkDeadNodes identifies and marks nodes that have timed out
func (m *Manager) checkDeadNodes() {
	activeNodes := m.stateMgr.GetActiveNodes()
	now := time.Now().Unix()
	timeoutSec := int64(m.nodeTimeout.Seconds())

	for _, node := range activeNodes {
		lastSeenUnix := node.LastSeen.Unix()
		if now-lastSeenUnix > timeoutSec {
			log.Printf("Node %s timed out (last heartbeat: %d seconds ago)",
				node.ID, now-lastSeenUnix)

			node.Status = db.NodeStatusFailed
			if err := m.stateMgr.UpdateNode(node); err != nil {
				log.Printf("Failed to mark node as failed: %v", err)
			}

			// Trigger election check when a node is marked as failed
			m.triggerElectionCheck()
		}
	}
}

// Shutdown gracefully shuts down the gossip manager
func (m *Manager) Shutdown() error {
	log.Printf("Shutting down gossip manager")

	m.cancel()

	if m.list != nil {
		if err := m.list.Leave(10 * time.Second); err != nil {
			log.Printf("Error leaving cluster: %v", err)
		}

		if err := m.list.Shutdown(); err != nil {
			return fmt.Errorf("failed to shutdown memberlist: %w", err)
		}
	}

	return nil
}

// TriggerElection manually triggers a master election
func (m *Manager) TriggerElection() error {
	aliveNodes := m.GetAliveNodes()
	log.Printf("Manual election trigger with %d alive nodes", len(aliveNodes))

	ctx, cancel := context.WithTimeout(m.ctx, 30*time.Second)
	defer cancel()

	_, err := m.elector.ElectMaster(ctx, aliveNodes)
	return err
}

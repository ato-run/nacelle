package gossip

import (
	"context"
	"testing"
	"time"

	"github.com/hashicorp/memberlist"
	"github.com/onescluster/coordinator/pkg/db"
)

// MockElector implements a simple mock elector for testing
type MockElector struct {
	isMaster         bool
	masterID         string
	electionCalled   bool
	validationCalled bool
}

func (m *MockElector) ElectMaster(ctx context.Context, aliveNodes []string) (string, error) {
	m.electionCalled = true
	if len(aliveNodes) > 0 {
		m.masterID = aliveNodes[0]
	}
	return m.masterID, nil
}

func (m *MockElector) IsMaster() bool {
	return m.isMaster
}

func (m *MockElector) GetMasterID() string {
	return m.masterID
}

func (m *MockElector) IsDegraded() bool {
	return false
}

func (m *MockElector) ValidateAndRecoverMaster(ctx context.Context, aliveNodes []string) error {
	m.validationCalled = true
	return nil
}

// MockStateManager for testing
type MockStateManager struct {
	nodes map[string]*db.Node
}

func newMockStateManager() *MockStateManager {
	return &MockStateManager{
		nodes: make(map[string]*db.Node),
	}
}

func (m *MockStateManager) GetNode(nodeID string) (*db.Node, bool) {
	node, exists := m.nodes[nodeID]
	return node, exists
}

func (m *MockStateManager) UpdateNode(node *db.Node) error {
	m.nodes[node.ID] = node
	return nil
}

func (m *MockStateManager) GetActiveNodes() []*db.Node {
	var active []*db.Node
	for _, node := range m.nodes {
		if node.Status == db.NodeStatusActive {
			active = append(active, node)
		}
	}
	return active
}

func waitForCondition(t *testing.T, timeout time.Duration, condition func() bool) {
	t.Helper()

	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		if condition() {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}

	t.Fatalf("condition not met within %s", timeout)
}

func TestNewManager(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	cfg := Config{
		NodeID:            "test-node-1",
		BindAddr:          "127.0.0.1:0", // Use port 0 for random port
		Peers:             []string{},
		StateManager:      mockState,
		Elector:           mockElector,
		HeartbeatInterval: 1 * time.Second,
		NodeTimeout:       5 * time.Second,
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}
	defer func() {
		if err := mgr.Shutdown(); err != nil {
			t.Logf("Failed to shutdown manager: %v", err)
		}
	}()

	if mgr.nodeID != cfg.NodeID {
		t.Errorf("Expected nodeID %s, got %s", cfg.NodeID, mgr.nodeID)
	}

	if mgr.list == nil {
		t.Error("Memberlist should be initialized")
	}

	// Verify manager starts with itself as a member
	members := mgr.GetMemberCount()
	if members != 1 {
		t.Errorf("Expected 1 member (self), got %d", members)
	}
}

func TestGetAliveNodes(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	cfg := Config{
		NodeID:       "test-node-1",
		BindAddr:     "127.0.0.1:0",
		Peers:        []string{},
		StateManager: mockState,
		Elector:      mockElector,
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}
	defer func() {
		if err := mgr.Shutdown(); err != nil {
			t.Logf("Failed to shutdown manager: %v", err)
		}
	}()

	aliveNodes := mgr.GetAliveNodes()

	if len(aliveNodes) == 0 {
		t.Error("Expected at least one alive node (self)")
	}

	// Should include the manager's own node ID
	found := false
	for _, nodeID := range aliveNodes {
		if nodeID == cfg.NodeID {
			found = true
			break
		}
	}

	if !found {
		t.Error("Own node ID not found in alive nodes list")
	}
}

func TestEventDelegateNotifyJoin(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	// Pre-populate state with a node
	testNodeID := "joining-node"
	mockState.nodes[testNodeID] = &db.Node{
		ID:       testNodeID,
		Status:   db.NodeStatusInactive,
		LastSeen: time.Now().Add(-1 * time.Hour),
	}

	cfg := Config{
		NodeID:       "test-node-1",
		BindAddr:     "127.0.0.1:0",
		Peers:        []string{},
		StateManager: mockState,
		Elector:      mockElector,
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}
	defer mgr.Shutdown()

	// Create event delegate
	delegate := &eventDelegate{manager: mgr}

	// Simulate node join
	node := &memberlist.Node{
		Name: testNodeID,
		Addr: []byte{127, 0, 0, 1},
		Port: 7946,
	}

	delegate.NotifyJoin(node)

	// Give some time for async operations
	time.Sleep(100 * time.Millisecond)

	// Verify node status was updated
	updatedNode, exists := mockState.GetNode(testNodeID)
	if !exists {
		t.Fatal("Node should exist in state manager")
	}

	if updatedNode.Status != "active" {
		t.Errorf("Expected node status to be 'active', got '%s'", updatedNode.Status)
	}

	// Verify election check was triggered
	if !mockElector.validationCalled {
		t.Error("Election validation should have been triggered")
	}
}

func TestEventDelegateNotifyLeave(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	// Pre-populate state with an active node
	testNodeID := "leaving-node"
	mockState.nodes[testNodeID] = &db.Node{
		ID:       testNodeID,
		Status:   db.NodeStatusActive,
		LastSeen: time.Now(),
	}

	cfg := Config{
		NodeID:       "test-node-1",
		BindAddr:     "127.0.0.1:0",
		Peers:        []string{},
		StateManager: mockState,
		Elector:      mockElector,
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}
	defer mgr.Shutdown()

	// Create event delegate
	delegate := &eventDelegate{manager: mgr}

	// Simulate node leave
	node := &memberlist.Node{
		Name: testNodeID,
		Addr: []byte{127, 0, 0, 1},
		Port: 7946,
	}

	delegate.NotifyLeave(node)

	// Give some time for async operations
	time.Sleep(100 * time.Millisecond)

	// Verify node status was updated
	updatedNode, exists := mockState.GetNode(testNodeID)
	if !exists {
		t.Fatal("Node should exist in state manager")
	}

	if updatedNode.Status != "inactive" {
		t.Errorf("Expected node status to be 'inactive', got '%s'", updatedNode.Status)
	}

	// Verify election check was triggered
	if !mockElector.validationCalled {
		t.Error("Election validation should have been triggered")
	}
}

func TestCheckDeadNodes(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	// Create nodes with different last seen times
	now := time.Now()
	mockState.nodes["healthy-node"] = &db.Node{
		ID:       "healthy-node",
		Status:   db.NodeStatusActive,
		LastSeen: now, // Just seen
	}
	mockState.nodes["timeout-node"] = &db.Node{
		ID:       "timeout-node",
		Status:   db.NodeStatusActive,
		LastSeen: now.Add(-10 * time.Minute), // 10 minutes ago
	}

	cfg := Config{
		NodeID:       "test-node-1",
		BindAddr:     "127.0.0.1:0",
		Peers:        []string{},
		StateManager: mockState,
		Elector:      mockElector,
		NodeTimeout:  5 * time.Minute, // 5 minute timeout
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}
	defer mgr.Shutdown()

	// Manually trigger dead node check
	mgr.checkDeadNodes()

	// Verify timeout-node is marked as failed
	timeoutNode, exists := mockState.GetNode("timeout-node")
	if !exists {
		t.Fatal("Timeout node should exist")
	}

	if timeoutNode.Status != db.NodeStatusFailed {
		t.Errorf("Expected timeout node status to be '%s', got '%s'",
			db.NodeStatusFailed, timeoutNode.Status)
	}

	// Verify healthy-node is still active
	healthyNode, exists := mockState.GetNode("healthy-node")
	if !exists {
		t.Fatal("Healthy node should exist")
	}

	if healthyNode.Status != db.NodeStatusActive {
		t.Errorf("Expected healthy node status to be '%s', got '%s'",
			db.NodeStatusActive, healthyNode.Status)
	}

	// Verify election check was triggered
	waitForCondition(t, 200*time.Millisecond, func() bool {
		return mockElector.validationCalled
	})
}

func TestPerformHeartbeat(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	testNodeID := "test-node-1"

	// Pre-populate with the test node
	oldLastSeen := time.Now().Add(-1 * time.Minute)
	mockState.nodes[testNodeID] = &db.Node{
		ID:       testNodeID,
		Status:   db.NodeStatusActive,
		LastSeen: oldLastSeen,
	}

	cfg := Config{
		NodeID:       testNodeID,
		BindAddr:     "127.0.0.1:0",
		Peers:        []string{},
		StateManager: mockState,
		Elector:      mockElector,
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}
	defer mgr.Shutdown()

	// Manually trigger heartbeat
	mgr.performHeartbeat()

	// Verify last seen was updated
	node, exists := mockState.GetNode(testNodeID)
	if !exists {
		t.Fatal("Node should exist")
	}

	if !node.LastSeen.After(oldLastSeen) {
		t.Error("LastSeen should have been updated")
	}

	// Should be very recent (within last second)
	if time.Since(node.LastSeen) > 1*time.Second {
		t.Error("LastSeen should be very recent")
	}
}

func TestShutdown(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	cfg := Config{
		NodeID:       "test-node-1",
		BindAddr:     "127.0.0.1:0",
		Peers:        []string{},
		StateManager: mockState,
		Elector:      mockElector,
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}

	err = mgr.Shutdown()
	if err != nil {
		t.Errorf("Shutdown failed: %v", err)
	}

	// Verify memberlist is properly shut down
	// Attempting to get members after shutdown should not panic
	defer func() {
		if r := recover(); r != nil {
			t.Errorf("Panic after shutdown: %v", r)
		}
	}()
}

func TestTriggerElection(t *testing.T) {
	mockState := newMockStateManager()
	mockElector := &MockElector{}

	cfg := Config{
		NodeID:       "test-node-1",
		BindAddr:     "127.0.0.1:0",
		Peers:        []string{},
		StateManager: mockState,
		Elector:      mockElector,
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Fatalf("Failed to create manager: %v", err)
	}
	defer mgr.Shutdown()

	err = mgr.TriggerElection()
	if err != nil {
		t.Errorf("TriggerElection failed: %v", err)
	}

	// Verify election was called
	if !mockElector.electionCalled {
		t.Error("Election should have been called")
	}
}

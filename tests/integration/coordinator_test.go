//go:build integration
// +build integration

package integration

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/oklog/ulid/v2"
	"github.com/onescluster/coordinator/pkg/config"
	"github.com/onescluster/coordinator/pkg/db"
	"github.com/onescluster/coordinator/pkg/gossip"
	"github.com/onescluster/coordinator/pkg/headscale"
	"github.com/onescluster/coordinator/pkg/master"
	"github.com/onescluster/coordinator/tests/integration/testutil"
)

// Integration tests for Coordinator clustering and master election
// These tests require a running rqlite instance

// Test network topology constants
// These represent the simulated network configuration for coordinator clustering
const (
	testLocalAddr    = "127.0.0.1:8080"     // Local coordinator address
	testLocalAddrAlt = "192.168.1.100:8080" // Alternative local address for multi-node tests
)

func getRQLiteAddr() string {
	addr := os.Getenv("RQLITE_ADDR")
	if addr == "" {
		addr = "http://localhost:4001"
	}
	return addr
}

func skipIfNoRQLite(t *testing.T) {
	// Try to connect to rqlite with shorter timeouts for connection check
	cfg := testutil.NewDBConfigWithOverrides(
		[]string{getRQLiteAddr()},
		1,                    // maxRetries
		100*time.Millisecond, // retryDelay
		2*time.Second,        // timeout
	)

	client, err := db.NewClient(cfg)
	if err != nil {
		t.Skipf("Skipping integration test: rqlite not available at %s", getRQLiteAddr())
	}
	defer client.Close()
}

func TestMasterElectionIntegration(t *testing.T) {
	skipIfNoRQLite(t)

	// This test verifies master election with real rqlite backend
	// 1. Initialize multiple coordinator instances
	// 2. Trigger election
	// 3. Verify smallest ULID becomes master
	// 4. Verify state is persisted to rqlite

	t.Run("single_node_election", func(t *testing.T) {
		// Create rqlite client
		cfg := testutil.NewDBConfig([]string{getRQLiteAddr()})

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		// Initialize database schema
		if err := client.Execute(db.CreateTablesSQL()); err != nil {
			t.Fatalf("Failed to create tables: %v", err)
		}

		// Create state manager
		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		// Create node ID
		nodeID := ulid.Make().String()

		// Register this node
		node := &db.Node{
			ID:        nodeID,
			Address:   testLocalAddr,
			Status:    db.NodeStatusActive,
			Resources: db.NodeResources{},
			LastSeen:  time.Now(),
		}

		if err := stateMgr.CreateNode(node); err != nil {
			t.Fatalf("Failed to create node: %v", err)
		}

		// Create mock headscale client
		mockHeadscale := &headscale.Client{}

		// Create elector
		elector := master.NewElector(master.ElectorConfig{
			NodeID:       nodeID,
			StateManager: stateMgr,
			HeadscaleAPI: mockHeadscale,
		})

		// Trigger election with just this node
		ctx := context.Background()
		masterID, err := elector.ElectMaster(ctx, []string{nodeID})

		if err != nil {
			t.Fatalf("Election failed: %v", err)
		}

		if masterID != nodeID {
			t.Errorf("Expected master to be %s, got %s", nodeID, masterID)
		}

		if !elector.IsMaster() {
			t.Error("Expected this node to be master")
		}

		t.Logf("✅ Single node election successful: %s", masterID)
	})

	t.Run("multi_node_election", func(t *testing.T) {
		// Create three nodes and verify smallest ULID is elected
		cfg := testutil.NewDBConfig([]string{getRQLiteAddr()})

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		// Create three node IDs with known order
		now := time.Now()
		node1ID := ulid.MustNew(ulid.Timestamp(now.Add(-3*time.Hour)), ulid.DefaultEntropy()).String()
		node2ID := ulid.MustNew(ulid.Timestamp(now.Add(-2*time.Hour)), ulid.DefaultEntropy()).String()
		node3ID := ulid.MustNew(ulid.Timestamp(now.Add(-1*time.Hour)), ulid.DefaultEntropy()).String()

		// Register all nodes
		for _, nid := range []string{node1ID, node2ID, node3ID} {
			node := &db.Node{
				ID:       nid,
				Address:  testLocalAddr,
				Status:   db.NodeStatusActive,
				LastSeen: time.Now(),
			}
			if err := stateMgr.CreateNode(node); err != nil {
				t.Fatalf("Failed to create node %s: %v", nid, err)
			}
		}

		// Create elector for node2 (middle node)
		mockHeadscale := &headscale.Client{}
		elector := master.NewElector(master.ElectorConfig{
			NodeID:       node2ID,
			StateManager: stateMgr,
			HeadscaleAPI: mockHeadscale,
		})

		// Trigger election with all three nodes
		ctx := context.Background()
		masterID, err := elector.ElectMaster(ctx, []string{node1ID, node2ID, node3ID})

		if err != nil {
			t.Fatalf("Election failed: %v", err)
		}

		// node1ID has the earliest timestamp, so it should be elected
		if masterID != node1ID {
			t.Errorf("Expected master to be %s (earliest), got %s", node1ID, masterID)
		}

		// node2 should NOT be master
		if elector.IsMaster() {
			t.Error("node2 should not be master, node1 should be")
		}

		t.Logf("✅ Multi-node election successful: %s elected from [%s, %s, %s]",
			masterID, node1ID, node2ID, node3ID)
	})
}

func TestStateConsistencyIntegration(t *testing.T) {
	skipIfNoRQLite(t)

	t.Run("node_crud_operations", func(t *testing.T) {
		cfg := testutil.NewDBConfig([]string{getRQLiteAddr()})

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		nodeID := ulid.Make().String()

		// Create node
		node := &db.Node{
			ID:       nodeID,
			Address:  testLocalAddrAlt,
			Status:   db.NodeStatusActive,
			LastSeen: time.Now(),
		}

		if err := stateMgr.CreateNode(node); err != nil {
			t.Fatalf("Failed to create node: %v", err)
		}

		// Read node
		retrieved, exists := stateMgr.GetNode(nodeID)
		if !exists {
			t.Fatal("Node should exist after creation")
		}

		if retrieved.Address != node.Address {
			t.Errorf("Expected address %s, got %s", node.Address, retrieved.Address)
		}

		// Update node
		retrieved.Status = db.NodeStatusFailed
		if err := stateMgr.UpdateNode(retrieved); err != nil {
			t.Fatalf("Failed to update node: %v", err)
		}

		// Verify update
		updated, exists := stateMgr.GetNode(nodeID)
		if !exists {
			t.Fatal("Node should still exist after update")
		}

		if updated.Status != db.NodeStatusFailed {
			t.Errorf("Expected status %s, got %s", db.NodeStatusFailed, updated.Status)
		}

		t.Logf("✅ CRUD operations successful for node %s", nodeID)
	})
}

func TestFailoverScenario(t *testing.T) {
	skipIfNoRQLite(t)

	// This test simulates master failure and verifies automatic re-election
	t.Run("master_failure_recovery", func(t *testing.T) {
		cfg := testutil.NewDBConfig([]string{getRQLiteAddr()})

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		// Create three nodes
		now := time.Now()
		masterID := ulid.MustNew(ulid.Timestamp(now.Add(-3*time.Hour)), ulid.DefaultEntropy()).String()
		node2ID := ulid.MustNew(ulid.Timestamp(now.Add(-2*time.Hour)), ulid.DefaultEntropy()).String()
		node3ID := ulid.MustNew(ulid.Timestamp(now.Add(-1*time.Hour)), ulid.DefaultEntropy()).String()

		for _, nid := range []string{masterID, node2ID, node3ID} {
			node := &db.Node{
				ID:       nid,
				Address:  testLocalAddr,
				Status:   db.NodeStatusActive,
				LastSeen: time.Now(),
			}
			if err := stateMgr.CreateNode(node); err != nil {
				t.Fatalf("Failed to create node: %v", err)
			}
		}

		mockHeadscale := &headscale.Client{}
		elector := master.NewElector(master.ElectorConfig{
			NodeID:       node2ID,
			StateManager: stateMgr,
			HeadscaleAPI: mockHeadscale,
		})

		// Initial election - masterID should win
		ctx := context.Background()
		elected, err := elector.ElectMaster(ctx, []string{masterID, node2ID, node3ID})
		if err != nil {
			t.Fatalf("Initial election failed: %v", err)
		}

		if elected != masterID {
			t.Fatalf("Expected initial master %s, got %s", masterID, elected)
		}

		t.Logf("Initial master elected: %s", masterID)

		// Simulate master failure (remove from alive nodes list)
		aliveAfterFailure := []string{node2ID, node3ID}

		// Trigger re-election
		err = elector.ValidateAndRecoverMaster(ctx, aliveAfterFailure)
		if err != nil {
			t.Fatalf("Failover failed: %v", err)
		}

		// Verify new master
		newMaster := elector.GetMasterID()
		if newMaster == masterID {
			t.Error("Master should have changed after failure")
		}

		// node2ID should be the new master (earliest among remaining)
		if newMaster != node2ID {
			t.Errorf("Expected new master %s, got %s", node2ID, newMaster)
		}

		t.Logf("✅ Failover successful: %s → %s", masterID, newMaster)
	})
}

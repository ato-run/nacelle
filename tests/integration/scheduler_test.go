// +build integration

package integration

import (
	"context"
	"testing"
	"time"

	"github.com/oklog/ulid/v2"
	"github.com/onescluster/coordinator/pkg/db"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
)

// Integration tests for GPU scheduler
// These tests verify the scheduler works correctly with a real database

func TestSchedulerBasicPlacement(t *testing.T) {
	skipIfNoRQLite(t)

	t.Run("schedule_to_node_with_sufficient_vram", func(t *testing.T) {
		cfg := &db.Config{
			Addresses:  []string{getRQLiteAddr()},
			MaxRetries: 3,
			RetryDelay: 1 * time.Second,
			Timeout:    10 * time.Second,
		}

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		// Create nodes with different VRAM capacities
		node1ID := ulid.Make().String()
		node2ID := ulid.Make().String()

		node1 := &db.Node{
			ID:       node1ID,
			Address:  "192.168.1.10:50051",
			Status:   db.NodeStatusActive,
			LastSeen: time.Now(),
			Resources: db.NodeResources{
				TotalVRAMBytes: 8 * gpu.Gigabyte,  // 8GB
				UsedVRAMBytes:  0,
			},
		}

		node2 := &db.Node{
			ID:       node2ID,
			Address:  "192.168.1.20:50051",
			Status:   db.NodeStatusActive,
			LastSeen: time.Now(),
			Resources: db.NodeResources{
				TotalVRAMBytes: 24 * gpu.Gigabyte, // 24GB
				UsedVRAMBytes:  0,
			},
		}

		if err := stateMgr.CreateNode(node1); err != nil {
			t.Fatalf("Failed to create node1: %v", err)
		}
		if err := stateMgr.CreateNode(node2); err != nil {
			t.Fatalf("Failed to create node2: %v", err)
		}

		// Create scheduler
		scheduler := gpu.NewScheduler(stateMgr)

		// Schedule workload requiring 16GB
		ctx := context.Background()
		constraints := gpu.GpuConstraints{
			MinVRAMBytes: 16 * gpu.Gigabyte,
		}

		selected, err := scheduler.SelectNode(ctx, constraints)
		if err != nil {
			t.Fatalf("Failed to select node: %v", err)
		}

		// Should select node2 (24GB) since node1 (8GB) is insufficient
		if selected != node2ID {
			t.Errorf("Expected node2 (%s) to be selected, got %s", node2ID, selected)
		}

		t.Logf("✅ Scheduled to node %s with 24GB VRAM", selected)
	})
}

func TestSchedulerBestFit(t *testing.T) {
	skipIfNoRQLite(t)

	t.Run("best_fit_selects_smallest_sufficient_node", func(t *testing.T) {
		cfg := &db.Config{
			Addresses:  []string{getRQLiteAddr()},
			MaxRetries: 3,
			RetryDelay: 1 * time.Second,
			Timeout:    10 * time.Second,
		}

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		// Create nodes with different VRAM capacities
		nodeSmallID := ulid.Make().String()
		nodeMediumID := ulid.Make().String()
		nodeLargeID := ulid.Make().String()

		nodes := []*db.Node{
			{
				ID:       nodeSmallID,
				Address:  "192.168.1.10:50051",
				Status:   db.NodeStatusActive,
				LastSeen: time.Now(),
				Resources: db.NodeResources{
					TotalVRAMBytes: 8 * gpu.Gigabyte,
					UsedVRAMBytes:  0,
				},
			},
			{
				ID:       nodeMediumID,
				Address:  "192.168.1.20:50051",
				Status:   db.NodeStatusActive,
				LastSeen: time.Now(),
				Resources: db.NodeResources{
					TotalVRAMBytes: 16 * gpu.Gigabyte,
					UsedVRAMBytes:  0,
				},
			},
			{
				ID:       nodeLargeID,
				Address:  "192.168.1.30:50051",
				Status:   db.NodeStatusActive,
				LastSeen: time.Now(),
				Resources: db.NodeResources{
					TotalVRAMBytes: 48 * gpu.Gigabyte,
					UsedVRAMBytes:  0,
				},
			},
		}

		for _, node := range nodes {
			if err := stateMgr.CreateNode(node); err != nil {
				t.Fatalf("Failed to create node: %v", err)
			}
		}

		// Create scheduler with best-fit strategy
		scheduler := gpu.NewScheduler(stateMgr)

		// Schedule workload requiring 12GB - should select 16GB node
		ctx := context.Background()
		constraints := gpu.GpuConstraints{
			MinVRAMBytes: 12 * gpu.Gigabyte,
		}

		selected, err := scheduler.SelectNode(ctx, constraints)
		if err != nil {
			t.Fatalf("Failed to select node: %v", err)
		}

		// Best fit should select nodeMedium (16GB) - smallest that fits
		if selected != nodeMediumID {
			t.Errorf("Expected nodeMedium (%s) to be selected, got %s", nodeMediumID, selected)
		}

		t.Logf("✅ Best-fit selected node %s with 16GB VRAM for 12GB workload", selected)
	})
}

func TestSchedulerVRAMFragmentation(t *testing.T) {
	skipIfNoRQLite(t)

	t.Run("accounts_for_used_vram", func(t *testing.T) {
		cfg := &db.Config{
			Addresses:  []string{getRQLiteAddr()},
			MaxRetries: 3,
			RetryDelay: 1 * time.Second,
			Timeout:    10 * time.Second,
		}

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		// Create two nodes, one partially used
		node1ID := ulid.Make().String()
		node2ID := ulid.Make().String()

		node1 := &db.Node{
			ID:       node1ID,
			Address:  "192.168.1.10:50051",
			Status:   db.NodeStatusActive,
			LastSeen: time.Now(),
			Resources: db.NodeResources{
				TotalVRAMBytes: 24 * gpu.Gigabyte,
				UsedVRAMBytes:  20 * gpu.Gigabyte, // Only 4GB free
			},
		}

		node2 := &db.Node{
			ID:       node2ID,
			Address:  "192.168.1.20:50051",
			Status:   db.NodeStatusActive,
			LastSeen: time.Now(),
			Resources: db.NodeResources{
				TotalVRAMBytes: 16 * gpu.Gigabyte,
				UsedVRAMBytes:  0, // 16GB free
			},
		}

		if err := stateMgr.CreateNode(node1); err != nil {
			t.Fatalf("Failed to create node1: %v", err)
		}
		if err := stateMgr.CreateNode(node2); err != nil {
			t.Fatalf("Failed to create node2: %v", err)
		}

		scheduler := gpu.NewScheduler(stateMgr)

		// Schedule workload requiring 8GB
		ctx := context.Background()
		constraints := gpu.GpuConstraints{
			MinVRAMBytes: 8 * gpu.Gigabyte,
		}

		selected, err := scheduler.SelectNode(ctx, constraints)
		if err != nil {
			t.Fatalf("Failed to select node: %v", err)
		}

		// Should select node2 since node1 only has 4GB free
		if selected != node2ID {
			t.Errorf("Expected node2 (%s) with 16GB free, got %s", node2ID, selected)
		}

		t.Logf("✅ Correctly accounted for used VRAM, selected node with sufficient free space")
	})
}

func TestSchedulerNoSuitableNode(t *testing.T) {
	skipIfNoRQLite(t)

	t.Run("returns_error_when_no_node_fits", func(t *testing.T) {
		cfg := &db.Config{
			Addresses:  []string{getRQLiteAddr()},
			MaxRetries: 3,
			RetryDelay: 1 * time.Second,
			Timeout:    10 * time.Second,
		}

		client, err := db.NewClient(cfg)
		if err != nil {
			t.Fatalf("Failed to create rqlite client: %v", err)
		}
		defer client.Close()

		stateMgr := db.NewStateManager(client)
		if err := stateMgr.Initialize(); err != nil {
			t.Fatalf("Failed to initialize state manager: %v", err)
		}

		// Create nodes with insufficient VRAM
		node1ID := ulid.Make().String()

		node1 := &db.Node{
			ID:       node1ID,
			Address:  "192.168.1.10:50051",
			Status:   db.NodeStatusActive,
			LastSeen: time.Now(),
			Resources: db.NodeResources{
				TotalVRAMBytes: 8 * gpu.Gigabyte,
				UsedVRAMBytes:  0,
			},
		}

		if err := stateMgr.CreateNode(node1); err != nil {
			t.Fatalf("Failed to create node1: %v", err)
		}

		scheduler := gpu.NewScheduler(stateMgr)

		// Try to schedule workload requiring 16GB (more than any node has)
		ctx := context.Background()
		constraints := gpu.GpuConstraints{
			MinVRAMBytes: 16 * gpu.Gigabyte,
		}

		_, err = scheduler.SelectNode(ctx, constraints)
		if err == nil {
			t.Fatal("Expected error when no suitable node exists")
		}

		t.Logf("✅ Correctly returned error: %v", err)
	})
}

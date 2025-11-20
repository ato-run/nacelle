package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/oklog/ulid/v2"
	"github.com/onescluster/coordinator/pkg/config"
	"github.com/onescluster/coordinator/pkg/db"

	"github.com/onescluster/coordinator/pkg/gossip"
	"github.com/onescluster/coordinator/pkg/headscale"
	"github.com/onescluster/coordinator/pkg/httpserver"
	"github.com/onescluster/coordinator/pkg/master"
	"github.com/onescluster/coordinator/pkg/reconcile"
)

func main() {
	// Parse command line flags
	configPath := flag.String("config", "config.yaml", "Path to configuration file")
	flag.Parse()

	fmt.Println("Capsuled Coordinator starting...")
	log.Println("Version: 1.0.0")

	// Load configuration
	log.Printf("Loading configuration from %s", *configPath)
	cfg, err := config.LoadConfig(*configPath)
	if err != nil {
		log.Fatalf("Failed to load configuration: %v", err)
	}

	rootCtx, rootCancel := context.WithCancel(context.Background())
	defer rootCancel()

	// Generate node ID if not provided
	if cfg.Coordinator.NodeID == "" {
		nodeID := ulid.Make().String()
		cfg.Coordinator.NodeID = nodeID
		log.Printf("Generated node ID: %s", nodeID)
	} else {
		log.Printf("Using node ID: %s", cfg.Coordinator.NodeID)
	}

	// Initialize rqlite client
	log.Println("Connecting to rqlite cluster... (SKIPPED FOR TESTING)")
	rqliteClient, err := db.NewClient(&db.Config{
		Addresses:  cfg.RQLite.Addresses,
		MaxRetries: cfg.RQLite.MaxRetries,
		RetryDelay: cfg.RQLite.GetRetryDelay(),
		Timeout:    cfg.RQLite.GetTimeout(),
	})
	if err != nil {
		log.Fatalf("Failed to connect to rqlite: %v", err)
	}
	defer rqliteClient.Close()

	log.Println("Connected to rqlite successfully")

	// Initialize database schema
	log.Println("Initializing database schema...")
	if err := db.InitSchema(rqliteClient); err != nil {
		log.Printf("Warning: Schema initialization failed: %v", err)
		log.Println("Attempting to verify existing schema...")
		if err := db.VerifySchema(rqliteClient); err != nil {
			log.Fatalf("Schema verification failed: %v", err)
		}
	}

	// Create state manager
	log.Println("Initializing state manager...")
	stateManager := db.NewStateManager(rqliteClient)

	// Load cluster state from rqlite
	if err := stateManager.Initialize(); err != nil {
		log.Fatalf("Failed to initialize state manager: %v", err)
	}

	// Initialize cluster state with defaults
	if err := db.InitializeClusterState(stateManager); err != nil {
		log.Fatalf("Failed to initialize cluster state: %v", err)
	}

	// Register this node in the cluster
	log.Println("Registering node in cluster...")
	node := &db.Node{
		ID:            cfg.Coordinator.NodeID,
		Address:       cfg.Coordinator.Address,
		HeadscaleName: cfg.Coordinator.HeadscaleName,
		Status:        db.NodeStatusActive,
		IsMaster:      false, // Will be determined by master election
		LastSeen:      time.Now(),
	}

	// Check if node already exists
	if existingNode, exists := stateManager.GetNode(node.ID); exists {
		log.Printf("Node already registered, updating: %s", node.ID)
		node.CreatedAt = existingNode.CreatedAt
		if err := stateManager.UpdateNode(node); err != nil {
			log.Fatalf("Failed to update node: %v", err)
		}
	} else {
		log.Printf("Registering new node: %s", node.ID)
		if err := stateManager.CreateNode(node); err != nil {
			log.Fatalf("Failed to register node: %v", err)
		}
	}

	// Perform health check
	log.Println("Performing health check...")
	if err := db.HealthCheck(rqliteClient, stateManager); err != nil {
		log.Fatalf("Health check failed: %v", err)
	}

	// Log current cluster state
	stats := stateManager.Stats()
	log.Printf("Cluster state: %+v", stats)

	// Initialize Headscale API client
	log.Println("Initializing Headscale API client...")
	headscaleClient := headscale.NewClient(
		cfg.Headscale.APIURL,
		cfg.Headscale.APIKey,
		cfg.Headscale.GetTimeout(),
	)

	// Verify Headscale connectivity
	healthCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	if err := headscaleClient.IsHealthy(healthCtx); err != nil {
		log.Printf("Warning: Headscale health check failed: %v", err)
		log.Println("Continuing with degraded mode capabilities")
	} else {
		log.Println("Headscale API is healthy")
	}
	cancel()

	// Initialize master elector
	log.Println("Initializing master elector...")
	elector := master.NewElector(master.ElectorConfig{
		NodeID:       cfg.Coordinator.NodeID,
		StateManager: stateManager,
		HeadscaleAPI: headscaleClient,
		MaxRetries:   3,
		RetryDelay:   5 * time.Second,
	})

	// Initialize gossip manager (memberlist)
	log.Println("Starting gossip protocol (memberlist)...")
	gossipMgr, err := gossip.NewManager(gossip.Config{
		NodeID:            cfg.Coordinator.NodeID,
		BindAddr:          cfg.Cluster.GossipBindAddr,
		Peers:             cfg.Cluster.Peers,
		StateManager:      stateManager,
		Elector:           elector,
		HeartbeatInterval: cfg.Cluster.GetHeartbeatInterval(),
		NodeTimeout:       cfg.Cluster.GetNodeTimeout(),
	})
	if err != nil {
		log.Fatalf("Failed to start gossip manager: %v", err)
	}
	defer gossipMgr.Shutdown()

	log.Printf("Gossip protocol started, cluster has %d members", gossipMgr.GetMemberCount())

	// Perform initial master election
	log.Println("Performing initial master election...")
	aliveNodes := gossipMgr.GetAliveNodes()
	electionCtx, electionCancel := context.WithTimeout(context.Background(), 30*time.Second)
	masterID, err := elector.ElectMaster(electionCtx, aliveNodes)
	electionCancel()

	if err != nil {
		log.Printf("Warning: Initial master election failed: %v", err)
		if elector.IsDegraded() {
			log.Println("Cluster is in degraded mode - limited operations available")
		}
	} else {
		isMaster := elector.IsMaster()
		log.Printf("Master election complete - Master: %s (This node is master: %v)", masterID, isMaster)

		// Update node's master status
		node.IsMaster = isMaster
		if err := stateManager.UpdateNode(node); err != nil {
			log.Printf("Warning: Failed to update node master status: %v", err)
		}
	}

	// Start reconciliation loop to detect drift between desired and actual workload state
	reconcileStore := reconcile.NewRQLiteStore(rqliteClient)
	reconcileInterval := cfg.Cluster.GetHeartbeatInterval() * 2
	if reconcileInterval <= 0 {
		reconcileInterval = 30 * time.Second
	}
	reconciler := reconcile.New(reconcileStore, reconcileInterval)
	stopReconciler := reconciler.Start(rootCtx)
	defer stopReconciler()

	// Start HTTP API server for coordinator UI and health checks
	log.Println("Starting HTTP API server...")
	httpAddr := ":8080" // Default address for coordinator UI
	httpSrv := httpserver.NewServer(httpserver.Config{
		Addr: httpAddr,
	})

	// Start HTTP server in background
	go func() {
		if err := httpSrv.Start(); err != nil {
			log.Printf("HTTP server stopped: %v", err)
		}
	}()
	defer func() {
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if err := httpSrv.Shutdown(shutdownCtx); err != nil {
			log.Printf("HTTP server shutdown error: %v", err)
		}
	}()

	log.Printf("HTTP UI and API server listening on %s", httpAddr)

	// TODO: Next steps
	// 1. Initialize Wasmer runtime for adep-logic.wasm
	// 2. Start gRPC client to communicate with agents

	log.Println("Coordinator initialized successfully")
	log.Printf("Node: %s (%s)", node.ID, node.HeadscaleName)
	log.Printf("Address: %s", node.Address)

	// Wait for interrupt signal
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, os.Interrupt, syscall.SIGTERM)

	<-sigChan
	rootCancel()

	log.Println("Shutting down...")

	// Mark node as inactive before shutdown
	node.Status = db.NodeStatusInactive
	if err := stateManager.UpdateNode(node); err != nil {
		log.Printf("Warning: Failed to update node status on shutdown: %v", err)
	}

	log.Println("Coordinator stopped")
}

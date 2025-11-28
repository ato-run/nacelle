package main

import (
	"flag"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"syscall"

	"google.golang.org/grpc"
	"google.golang.org/grpc/reflection"

	"github.com/onescluster/coordinator/pkg/api"
	"github.com/onescluster/coordinator/pkg/db"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
	"github.com/onescluster/coordinator/pkg/service"
	pb "github.com/onescluster/coordinator/pkg/proto"
)

var (
	port = flag.Int("port", 50052, "The server port")
)

func main() {
	flag.Parse()

	lis, err := net.Listen("tcp", fmt.Sprintf(":%d", *port))
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	s := grpc.NewServer()

	// Initialize Store (rqlite)
	rqliteAddr := os.Getenv("RQLITE_ADDR")
	if rqliteAddr == "" {
		rqliteAddr = "http://localhost:4001"
	}

	dbConfig := &db.Config{
		Addresses: []string{rqliteAddr},
	}
	
	dbClient, err := db.NewClient(dbConfig)
	if err != nil {
		log.Printf("Warning: failed to connect to rqlite: %v", err)
		// We continue, but StateManager might fail. 
		// Ideally we should block or retry until DB is available, 
		// but for dev we might want to start even if DB is down (though StateManager needs it).
	}

	stateManager := db.NewStateManager(dbClient)
	// Try to initialize state from DB (load cache)
	if err := stateManager.Initialize(); err != nil {
		log.Printf("Warning: failed to initialize state manager: %v", err)
	}

	// Initialize Service
	coordinatorService := service.NewCoordinatorService(stateManager)
	pb.RegisterCoordinatorServiceServer(s, coordinatorService)

	// Register reflection service on gRPC server.
	reflection.Register(s)

	// Initialize Scheduler
	scheduler := gpu.NewScheduler()

	// Initialize DeployHandler (HTTP)
	deployHandler := api.NewDeployHandler(stateManager, scheduler)

	// Start HTTP server
	go func() {
		http.HandleFunc("/deploy", deployHandler.HandleDeploy)
		log.Println("HTTP server listening at :8080")
		if err := http.ListenAndServe(":8080", nil); err != nil {
			log.Fatalf("failed to serve HTTP: %v", err)
		}
	}()

	log.Printf("Coordinator server listening at %v", lis.Addr())

	// Graceful shutdown
	go func() {
		sigCh := make(chan os.Signal, 1)
		signal.Notify(sigCh, os.Interrupt, syscall.SIGTERM)
		<-sigCh
		log.Println("Shutting down gRPC server...")
		s.GracefulStop()
	}()

	if err := s.Serve(lis); err != nil {
		log.Fatalf("failed to serve: %v", err)
	}
}

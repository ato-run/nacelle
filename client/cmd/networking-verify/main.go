package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"time"

	"google.golang.org/grpc"

	pb "github.com/onescluster/coordinator/pkg/proto"
	"github.com/onescluster/coordinator/pkg/spec"
)

func main() {
	manifestPath := flag.String("manifest", "", "Path to capsule.toml manifest")
	flag.Parse()

	if *manifestPath == "" {
		log.Fatal("Please provide --manifest flag")
	}

	// Parse Manifest using CLI mapper
	capsuleID, runPlan, err := spec.ParseRunPlanFile(*manifestPath)
	if err != nil {
		log.Fatalf("Failed to parse manifest: %v", err)
	}

	// Extract Info for Coordinator API (which doesn't support RunPlan yet)
	// We map RunPlan -> DeployCapsuleRequest
	runtimeName := "unknown"
	if docker := runPlan.GetDocker(); docker != nil {
		runtimeName = docker.Image
	} else if runPlan.GetNative() != nil {
		runtimeName = "native"
	}

	conn, err := grpc.Dial("localhost:50052", grpc.WithInsecure())
	if err != nil {
		log.Fatalf("did not connect: %v", err)
	}
	defer conn.Close()
	c := pb.NewCoordinatorServiceClient(conn)

	ctx, cancel := context.WithTimeout(context.Background(), time.Second*30)
	defer cancel()

	fmt.Printf("Deploying Capsule %s (Runtime: %s)...\n", capsuleID, runtimeName)
	r, err := c.DeployCapsule(ctx, &pb.DeployCapsuleRequest{
		Name:        capsuleID,
		RuntimeName: runtimeName,
		// Passing config if needed, though Coordinator mainly uses Name/RuntimeName for now
		Config: map[string]string{
			"deployed_via": "networking-verify",
		},
	})
	if err != nil {
		log.Fatalf("could not deploy: %v", err)
	}

	fmt.Printf("Success! Capsule ID: %s\n", r.CapsuleId)
	fmt.Printf("Assigned Port: %d\n", r.AssignedPort)
	fmt.Printf("Access URL: %s\n", r.AccessUrl)
}

package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"os"
	"time"

	pb "github.com/onescluster/coordinator/pkg/proto"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func main() {
	addr := flag.String("addr", "localhost:50051", "Engine gRPC address")
	cmd := flag.String("cmd", "", "Command to run: fetch, deploy, or stop")
	capsuleID := flag.String("capsule-id", "", "Capsule ID for stop command")
	url := flag.String("url", "", "URL for fetch")
	dest := flag.String("dest", "", "Destination for fetch")
	manifest := flag.String("manifest", "", "Path to manifest JSON for deploy")
	flag.Parse()

	conn, err := grpc.NewClient(*addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		log.Fatalf("did not connect: %v", err)
	}
	defer conn.Close()

	client := pb.NewAgentServiceClient(conn)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	switch *cmd {
	case "fetch":
		if *url == "" || *dest == "" {
			log.Fatal("url and dest required for fetch")
		}
		fmt.Printf("Fetching %s to %s...\n", *url, *dest)
		resp, err := client.FetchModel(ctx, &pb.FetchModelRequest{
			Url:         *url,
			Destination: *dest,
		})
		if err != nil {
			log.Fatalf("FetchModel failed: %v", err)
		}
		fmt.Printf("Fetch success: %v, Message: %s, Bytes: %d\n", resp.Success, resp.Message, resp.BytesDownloaded)

	case "deploy":
		if *manifest == "" {
			log.Fatal("manifest required for deploy")
		}
		data, err := os.ReadFile(*manifest)
		if err != nil {
			log.Fatalf("Failed to read manifest: %v", err)
		}

		var adepManifest pb.AdePManifest
		if err := json.Unmarshal(data, &adepManifest); err != nil {
			log.Fatalf("Failed to parse manifest JSON: %v", err)
		}

		fmt.Printf("Deploying workload from %s...\n", *manifest)
		resp, err := client.DeployWorkload(ctx, &pb.DeployWorkloadRequest{
			WorkloadId:   "test-workload-" + fmt.Sprintf("%d", time.Now().Unix()),
			Manifest:     &adepManifest,
			ManifestJson: string(data),
		})
		if err != nil {
			log.Fatalf("DeployWorkload failed: %v", err)
		}
		fmt.Printf("Deploy success: %v, Message: %s\n", resp.Success, resp.Message)
		// Print the capsule ID so we can stop it later
		// The response doesn't contain it, but we generated it.
		// Ideally the response should return it, but for now we rely on the log or fixed ID if we change it.
		// Actually, let's just print what we sent.
		// Wait, the request generation was inline. Let's extract it or just print "Check logs for ID".

	case "stop":
		if *capsuleID == "" {
			log.Fatal("capsule-id required for stop")
		}
		fmt.Printf("Stopping capsule %s...\n", *capsuleID)
		resp, err := client.StopWorkload(ctx, &pb.StopWorkloadRequest{
			WorkloadId: *capsuleID,
		})
		if err != nil {
			log.Fatalf("StopWorkload failed: %v", err)
		}
		fmt.Printf("Stop success: %v, Message: %s\n", resp.Success, resp.Message)

	default:
		log.Fatal("Unknown command. Use -cmd fetch, -cmd deploy, or -cmd stop")
	}
}

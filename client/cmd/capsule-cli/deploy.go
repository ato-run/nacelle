package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"github.com/spf13/cobra"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	coordinatorv1 "github.com/onescluster/coordinator/pkg/proto/coordinator/v1"
)

var deployCmd = &cobra.Command{
	Use:   "deploy [path/to/capsule.toml]",
	Short: "Deploy a capsule",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		manifestPath := args[0]
		content, err := os.ReadFile(manifestPath)
		if err != nil {
			return fmt.Errorf("failed to read manifest: %w", err)
		}

		conn, err := grpc.NewClient(coordinatorAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
		if err != nil {
			return fmt.Errorf("failed to connect to coordinator: %w", err)
		}
		defer conn.Close()

		client := coordinatorv1.NewCoordinatorServiceClient(conn)

		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()

		fmt.Printf("Deploying capsule from %s to %s...\n", manifestPath, coordinatorAddr)
		resp, err := client.DeployCapsule(ctx, &coordinatorv1.DeployRequest{
			TomlContent: content,
		})
		if err != nil {
			return fmt.Errorf("deployment failed: %w", err)
		}

		fmt.Printf("✅ Deployed Capsule!\n")
		fmt.Printf("ID:  %s\n", resp.CapsuleId)
		fmt.Printf("URL: %s\n", resp.Url)

		return nil
	},
}

func init() {
	rootCmd.AddCommand(deployCmd)
}

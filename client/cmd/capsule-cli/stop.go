package main

import (
	"context"
	"fmt"
	"time"

	"github.com/spf13/cobra"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	coordinatorv1 "github.com/onescluster/coordinator/pkg/proto/coordinator/v1"
)

var stopCmd = &cobra.Command{
	Use:   "stop [capsule-id]",
	Short: "Stop a capsule",
	Args:  cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		capsuleID := args[0]

		conn, err := grpc.NewClient(coordinatorAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
		if err != nil {
			return fmt.Errorf("failed to connect to coordinator: %w", err)
		}
		defer conn.Close()

		client := coordinatorv1.NewCoordinatorServiceClient(conn)

		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()

		resp, err := client.StopCapsule(ctx, &coordinatorv1.StopRequest{
			CapsuleId: capsuleID,
		})
		if err != nil {
			return fmt.Errorf("stop failed: %w", err)
		}

		if resp.Success {
			fmt.Printf("✅ Stopped capsule: %s\n", capsuleID)
		} else {
			fmt.Printf("⚠️  Stop request processed but may have failed: %s\n", resp.Message)
		}

		return nil
	},
}

func init() {
	rootCmd.AddCommand(stopCmd)
}

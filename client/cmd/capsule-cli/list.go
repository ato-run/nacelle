package main

import (
	"context"
	"fmt"
	"os"
	"text/tabwriter"
	"time"

	"github.com/spf13/cobra"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	coordinatorv1 "github.com/onescluster/coordinator/pkg/proto/coordinator/v1"
)

var listCmd = &cobra.Command{
	Use:   "list",
	Short: "List running capsules",
	RunE: func(cmd *cobra.Command, args []string) error {
		conn, err := grpc.NewClient(coordinatorAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
		if err != nil {
			return fmt.Errorf("failed to connect to coordinator: %w", err)
		}
		defer conn.Close()

		client := coordinatorv1.NewCoordinatorServiceClient(conn)

		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()

		resp, err := client.ListCapsules(ctx, &coordinatorv1.ListRequest{})
		if err != nil {
			return fmt.Errorf("list failed: %w", err)
		}

		if len(resp.Capsules) == 0 {
			fmt.Println("No capsules found.")
			return nil
		}

		w := tabwriter.NewWriter(os.Stdout, 0, 0, 3, ' ', 0)
		fmt.Fprintln(w, "ID\tNAME\tSTATUS\tURL")
		for _, c := range resp.Capsules {
			fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", c.CapsuleId, c.Name, c.Status, c.Url)
		}
		w.Flush()

		return nil
	},
}

func init() {
	rootCmd.AddCommand(listCmd)
}

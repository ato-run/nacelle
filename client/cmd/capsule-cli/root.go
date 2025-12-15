package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var (
	coordinatorAddr string
)

var rootCmd = &cobra.Command{
	Use:   "capsule-cli",
	Short: "CLI for Gumball (Personal Cloud OS) - Phase 4",
	Long:  `capsule-cli interacts with the Gumball Coordinator to deploy, list, and manage Capsules.`,
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func init() {
	// Persistent Flags
	rootCmd.PersistentFlags().StringVar(&coordinatorAddr, "coordinator", "localhost:50050", "Address of the Coordinator gRPC server")
}

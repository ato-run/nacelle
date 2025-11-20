package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
)

// FetchModelRequest represents the request body for model fetching
type FetchModelRequest struct {
	URL         string `json:"url"`
	Destination string `json:"destination"`
	RigID       string `json:"rig_id"`
}

// FetchModelResponse represents the response
type FetchModelResponse struct {
	Success         bool   `json:"success"`
	Message         string `json:"message"`
	BytesDownloaded uint64 `json:"bytes_downloaded"`
}

func main() {
	if len(os.Args) < 2 {
		printUsage()
		os.Exit(1)
	}

	command := os.Args[1]

	switch command {
	case "model":
		handleModelCommand(os.Args[2:])
	case "help", "-h", "--help":
		printUsage()
	default:
		fmt.Fprintf(os.Stderr, "Unknown command: %s\n", command)
		printUsage()
		os.Exit(1)
	}
}

func handleModelCommand(args []string) {
	if len(args) < 1 {
		fmt.Fprintf(os.Stderr, "Usage: rig-client model <subcommand>\n")
		fmt.Fprintf(os.Stderr, "Available subcommands: fetch\n")
		os.Exit(1)
	}

	subcommand := args[0]

	switch subcommand {
	case "fetch":
		handleModelFetch(args[1:])
	default:
		fmt.Fprintf(os.Stderr, "Unknown model subcommand: %s\n", subcommand)
		os.Exit(1)
	}
}

func handleModelFetch(args []string) {
	fs := flag.NewFlagSet("model fetch", flag.ExitOnError)
	rigID := fs.String("rig", "", "Target rig ID (required)")
	coordinatorURL := fs.String("coordinator", "http://localhost:8080", "Coordinator API URL")

	fs.Usage = func() {
		fmt.Fprintf(os.Stderr, "Usage: rig-client model fetch [options] <url> <destination>\n")
		fmt.Fprintf(os.Stderr, "\nFetch a model file from a URL to a destination path on the target rig.\n\n")
		fmt.Fprintf(os.Stderr, "Arguments:\n")
		fmt.Fprintf(os.Stderr, "  <url>          Source URL to download from (e.g., https://huggingface.co/.../config.json)\n")
		fmt.Fprintf(os.Stderr, "  <destination>  Destination file path on the target rig (e.g., /opt/models/llama3/config.json)\n\n")
		fmt.Fprintf(os.Stderr, "Options:\n")
		fs.PrintDefaults()
		fmt.Fprintf(os.Stderr, "\nExample:\n")
		fmt.Fprintf(os.Stderr, "  rig-client model fetch --rig my-rig https://example.com/model.bin /opt/models/llama3/model.bin\n")
	}

	if err := fs.Parse(args); err != nil {
		os.Exit(1)
	}

	if *rigID == "" {
		fmt.Fprintf(os.Stderr, "Error: --rig flag is required\n\n")
		fs.Usage()
		os.Exit(1)
	}

	if fs.NArg() != 2 {
		fmt.Fprintf(os.Stderr, "Error: exactly 2 arguments required (url and destination)\n\n")
		fs.Usage()
		os.Exit(1)
	}

	url := fs.Arg(0)
	destination := fs.Arg(1)

	// Create request
	reqBody := FetchModelRequest{
		URL:         url,
		Destination: destination,
		RigID:       *rigID,
	}

	reqJSON, err := json.Marshal(reqBody)
	if err != nil {
		log.Fatalf("Failed to marshal request: %v", err)
	}

	// Send request to coordinator
	endpoint := fmt.Sprintf("%s/api/v1/models/fetch", *coordinatorURL)
	resp, err := http.Post(endpoint, "application/json", bytes.NewBuffer(reqJSON))
	if err != nil {
		log.Fatalf("Failed to send request: %v", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		log.Fatalf("Failed to read response: %v", err)
	}

	var fetchResp FetchModelResponse
	if err := json.Unmarshal(body, &fetchResp); err != nil {
		log.Fatalf("Failed to unmarshal response: %v", err)
	}

	if !fetchResp.Success {
		fmt.Fprintf(os.Stderr, "❌ Model fetch failed: %s\n", fetchResp.Message)
		os.Exit(1)
	}

	fmt.Printf("✅ Model fetched successfully\n")
	fmt.Printf("   URL: %s\n", url)
	fmt.Printf("   Destination: %s\n", destination)
	fmt.Printf("   Bytes Downloaded: %d (%.2f MB)\n", fetchResp.BytesDownloaded, float64(fetchResp.BytesDownloaded)/(1024*1024))
}

func printUsage() {
	fmt.Fprintf(os.Stderr, "Capsuled Rig Client - CLI tool for managing rigs and models\n\n")
	fmt.Fprintf(os.Stderr, "Usage: rig-client <command> [options]\n\n")
	fmt.Fprintf(os.Stderr, "Available commands:\n")
	fmt.Fprintf(os.Stderr, "  model    Manage model files on rigs\n")
	fmt.Fprintf(os.Stderr, "  help     Show this help message\n\n")
	fmt.Fprintf(os.Stderr, "Use 'rig-client <command> -h' for more information about a command.\n")
}

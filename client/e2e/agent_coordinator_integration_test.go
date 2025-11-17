package e2e

import (
	"context"
	"database/sql"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
	server "github.com/onescluster/coordinator/pkg/grpc"
	pb "github.com/onescluster/coordinator/pkg/proto"
	"google.golang.org/grpc"
	_ "modernc.org/sqlite"
)

const (
	gigabyte      = uint64(1024 * 1024 * 1024)
	testLocalAddr = "127.0.0.1" // Local address for test gRPC server
)

func TestAgentCoordinatorVRAME2E(t *testing.T) {
	t.Parallel()

	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "cluster.db")

	sqlite, err := sql.Open("sqlite", dbPath)
	if err != nil {
		t.Fatalf("failed to open sqlite: %v", err)
	}
	t.Cleanup(func() { sqlite.Close() })

	applyCoordinatorSchema(t, sqlite)

	nodeStore := db.NewNodeStore(sqlite)

	lis, err := net.Listen("tcp", testLocalAddr+":0")
	if err != nil {
		t.Fatalf("failed to listen: %v", err)
	}
	grpcServer := grpc.NewServer()
	pb.RegisterCoordinatorServer(grpcServer, server.NewServer(nodeStore))

	done := make(chan struct{})
	go func() {
		defer close(done)
		if err := grpcServer.Serve(lis); err != nil {
			t.Errorf("grpc server failed: %v", err)
		}
	}()
	t.Cleanup(func() {
		grpcServer.GracefulStop()
		<-done
	})

	coordinatorEndpoint := fmt.Sprintf("http://%s", lis.Addr().String())

	engineDir, err := filepath.Abs(filepath.Join("..", "..", "engine"))
	if err != nil {
		t.Fatalf("failed to resolve engine directory: %v", err)
	}
	buildCmd := exec.Command("cargo", "build", "--quiet", "--bin", "status-reporter-driver")
	buildCmd.Dir = engineDir
	buildCmd.Stdout = os.Stdout
	buildCmd.Stderr = os.Stderr
	if err := buildCmd.Run(); err != nil {
		t.Fatalf("failed to build status-reporter-driver: %v", err)
	}

	driverPath := filepath.Join(engineDir, "target", "debug", "status-reporter-driver")
	rigID := "rig-e2e"
	workloadID := "capsule-e2e"
	reservedVRAM := uint64(12) * gigabyte
	observedVRAM := uint64(16) * gigabyte
	pid := uint32(4242)

	args := []string{
		"--coordinator-endpoint", coordinatorEndpoint,
		"--rig-id", rigID,
		"--workload-id", workloadID,
		"--pid", fmt.Sprintf("%d", pid),
		"--reserved-vram-bytes", fmt.Sprintf("%d", reservedVRAM),
		"--observed-vram-bytes", fmt.Sprintf("%d", observedVRAM),
		"--total-vram-bytes", fmt.Sprintf("%d", 64*gigabyte),
		"--send-count", "2",
		"--interval-ms", "200",
	}

	runCmd := exec.Command(driverPath, args...)
	runCmd.Dir = engineDir
	runCmd.Env = append(os.Environ(), "RUST_LOG=info")
	runCmd.Stdout = os.Stdout
	runCmd.Stderr = os.Stderr

	if err := runCmd.Run(); err != nil {
		t.Fatalf("status-reporter-driver failed: %v", err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	waitForWorkload(t, ctx, nodeStore, workloadID)

	assertNodeUsage(t, sqlite, rigID, observedVRAM)
	assertWorkloadMetrics(t, sqlite, rigID, workloadID, reservedVRAM, observedVRAM, pid)
}

func waitForWorkload(t *testing.T, ctx context.Context, store *db.NodeStore, workloadID string) {
	t.Helper()

	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			t.Fatalf("workload %s not observed before timeout", workloadID)
		case <-ticker.C:
			workloads, err := store.ListNodeWorkloads(context.Background())
			if err != nil {
				t.Fatalf("failed to list workloads: %v", err)
			}
			for _, wl := range workloads {
				if wl.WorkloadID == workloadID {
					return
				}
			}
		}
	}
}

func assertNodeUsage(t *testing.T, dbConn *sql.DB, rigID string, expectedUsed uint64) {
	var used uint64
	err := dbConn.QueryRow("SELECT used_vram_bytes FROM nodes WHERE id = ?", rigID).Scan(&used)
	if err != nil {
		t.Fatalf("query used_vram_bytes failed: %v", err)
	}

	if used != expectedUsed {
		t.Fatalf("used_vram_bytes mismatch: got %d want %d", used, expectedUsed)
	}
}

func assertWorkloadMetrics(t *testing.T, dbConn *sql.DB, rigID, workloadID string, reserved, observed uint64, pid uint32) {
	row := dbConn.QueryRow(`
        SELECT reserved_vram_bytes, observed_vram_bytes, pid
        FROM node_workloads
        WHERE node_id = ? AND workload_id = ?`, rigID, workloadID)

	var reservedDB, observedDB uint64
	var pidDB sql.NullInt64
	if err := row.Scan(&reservedDB, &observedDB, &pidDB); err != nil {
		t.Fatalf("failed to scan node_workloads row: %v", err)
	}

	if reservedDB != reserved {
		t.Fatalf("reserved_vram_bytes mismatch: got %d want %d", reservedDB, reserved)
	}
	if observedDB != observed {
		t.Fatalf("observed_vram_bytes mismatch: got %d want %d", observedDB, observed)
	}
	if !pidDB.Valid || uint32(pidDB.Int64) != pid {
		t.Fatalf("pid mismatch: got %v want %d", pidDB, pid)
	}
}

func applyCoordinatorSchema(t *testing.T, dbConn *sql.DB) {
	t.Helper()

	base := filepath.Clean(filepath.Join("..", "pkg", "db"))
	files := []string{
		filepath.Join(base, "schema.sql"),
		filepath.Join(base, "migrations", "001_add_gpu_columns.sql"),
		filepath.Join(base, "migrations", "002_create_node_workloads.sql"),
		filepath.Join(base, "migrations", "003_add_observed_vram.sql"),
	}

	for _, file := range files {
		content, err := os.ReadFile(file)
		if err != nil {
			t.Fatalf("failed to read %s: %v", file, err)
		}
		statements := splitStatements(string(content))
		for _, stmt := range statements {
			if strings.TrimSpace(stmt) == "" {
				continue
			}
			if _, err := dbConn.Exec(stmt); err != nil {
				t.Fatalf("failed to exec statement from %s: %v", file, err)
			}
		}
	}
}

func splitStatements(sql string) []string {
	parts := strings.Split(sql, ";")
	result := make([]string, 0, len(parts))
	for _, p := range parts {
		stmt := strings.TrimSpace(p)
		if stmt != "" {
			result = append(result, stmt)
		}
	}
	return result
}

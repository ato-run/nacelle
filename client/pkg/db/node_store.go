package db

import (
	"context"
	"database/sql"
	"fmt"
	"log"
	"time"

	pb "github.com/onescluster/coordinator/pkg/proto"
	"github.com/onescluster/coordinator/pkg/scheduler/gpu"
)

// NodeStore handles database operations for node hardware information
type NodeStore struct {
	db *sql.DB
}

// NodeWorkload models the actual workload state reported by an Agent
type NodeWorkload struct {
	NodeID            string
	WorkloadID        string
	Name              string
	ReservedVRAMBytes uint64
	ObservedVRAMBytes uint64
	PID               sql.NullInt64
	Phase             string
	UpdatedAt         time.Time
}

// ListDesiredWorkloads returns the set of workloads that the Coordinator believes should exist.
func (s *NodeStore) ListDesiredWorkloads(ctx context.Context) (map[string]string, error) {
	query := `
		SELECT id, node_id
		FROM capsules
		WHERE status IN (?, ?, ?)`

	rows, err := s.db.QueryContext(ctx, query,
		CapsuleStatusPending,
		CapsuleStatusRunning,
		CapsuleStatusFailed,
	)
	if err != nil {
		return nil, fmt.Errorf("query desired workloads: %w", err)
	}
	defer rows.Close()

	desired := make(map[string]string)
	for rows.Next() {
		var id, nodeID string
		if err := rows.Scan(&id, &nodeID); err != nil {
			return nil, fmt.Errorf("scan desired workload: %w", err)
		}
		desired[id] = nodeID
	}

	if err = rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate desired workloads: %w", err)
	}

	return desired, nil
}

// NewNodeStore creates a new NodeStore instance
func NewNodeStore(db *sql.DB) *NodeStore {
	return &NodeStore{db: db}
}

// UpdateNodeStatus upserts the hardware + workload snapshot reported by an Agent.
//
// This method is called on every heartbeat. It persists:
//  1. Hardware state (total/used VRAM, CUDA version)
//  2. Currently running workloads on the node (node_workloads table)
//  3. Last seen timestamp for node liveness tracking
func (s *NodeStore) UpdateNodeStatus(ctx context.Context, status *pb.RigStatus) error {
	if status == nil {
		return fmt.Errorf("status must not be nil")
	}

	hardware := status.GetHardware()
	var totalVRAM uint64
	var usedVRAM uint64
	var cudaDriverVersion string

	if hardware != nil {
		totalVRAM = hardware.GetTotalVramBytes()
		if totalVRAM == 0 {
			for _, gpuInfo := range hardware.Gpus {
				totalVRAM += gpuInfo.GetVramTotalBytes()
			}
		}

		usedVRAM = hardware.GetUsedVramBytes()
		cudaDriverVersion = hardware.GetSystemCudaVersion()
	}

	// Fallback to sum of workloads if Agent didn't provide used_vram_bytes explicitly
	if usedVRAM == 0 {
		for _, wl := range status.GetRunningWorkloads() {
			usedVRAM += wl.GetReservedVramBytes()
		}
	}

	reportedAt := status.GetReportedAtUnixSeconds()
	var lastSeen int64
	if reportedAt == 0 {
		lastSeen = time.Now().Unix()
	} else {
		lastSeen = int64(reportedAt)
	}

	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin tx: %w", err)
	}
	defer func() {
		if err != nil {
			if rollbackErr := tx.Rollback(); rollbackErr != nil {
				log.Printf("WARN: failed to rollback UpdateNodeStatus tx: %v", rollbackErr)
			}
		}
	}()

	now := time.Now().Unix()

	updateQuery := `
		UPDATE nodes
		SET
			total_vram_bytes = ?,
			used_vram_bytes = ?,
			cuda_driver_version = ?,
			last_seen = ?,
			updated_at = ?
		WHERE id = ?`

	result, err := tx.ExecContext(ctx, updateQuery,
		totalVRAM,
		usedVRAM,
		cudaDriverVersion,
		lastSeen,
		now,
		status.GetRigId(),
	)
	if err != nil {
		return fmt.Errorf("update node hardware: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("rows affected: %w", err)
	}

	if rowsAffected == 0 {
		insertQuery := `
			INSERT INTO nodes (
				id, address, headscale_name, status, is_master, last_seen,
				created_at, updated_at, total_vram_bytes, used_vram_bytes, cuda_driver_version
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`

		if _, err = tx.ExecContext(ctx, insertQuery,
			status.GetRigId(),
			"unknown",
			status.GetRigId(),
			NodeStatusActive,
			0,
			lastSeen,
			now,
			now,
			totalVRAM,
			usedVRAM,
			cudaDriverVersion,
		); err != nil {
			return fmt.Errorf("insert node hardware: %w", err)
		}
	}

	// Replace node_workloads entries for this node
	if _, err = tx.ExecContext(ctx, "DELETE FROM node_workloads WHERE node_id = ?", status.GetRigId()); err != nil {
		return fmt.Errorf("delete node workloads: %w", err)
	}

	if len(status.GetRunningWorkloads()) > 0 {
		insertWL := `
			INSERT INTO node_workloads (
				node_id,
				workload_id,
				name,
				reserved_vram_bytes,
				observed_vram_bytes,
				pid,
				phase,
				updated_at
			)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?)`

		for _, wl := range status.GetRunningWorkloads() {
			phase := wl.GetPhase().String()
			if phase == "" {
				phase = pb.WorkloadPhase_WORKLOAD_PHASE_UNSPECIFIED.String()
			}

			pid := sql.NullInt64{}
			if wl.GetPid() != 0 {
				pid.Int64 = int64(wl.GetPid())
				pid.Valid = true
			}

			if _, err = tx.ExecContext(ctx, insertWL,
				status.GetRigId(),
				wl.GetWorkloadId(),
				defaultWorkloadName(wl),
				wl.GetReservedVramBytes(),
				wl.GetObservedVramBytes(),
				pid,
				phase,
				now,
			); err != nil {
				return fmt.Errorf("insert node workload %s: %w", wl.GetWorkloadId(), err)
			}
		}
	}

	if err = tx.Commit(); err != nil {
		return fmt.Errorf("commit UpdateNodeStatus: %w", err)
	}

	return nil
}

func defaultWorkloadName(wl *pb.WorkloadStatus) string {
	if wl.GetName() != "" {
		return wl.GetName()
	}
	if wl.GetWorkloadId() != "" {
		return wl.GetWorkloadId()
	}
	return "unknown-workload"
}

// UpdateNodeHardware inserts or updates hardware information for a node (UPSERT)
//
// This method is called when an Agent reports its hardware capabilities via gRPC.
// If the node doesn't exist in the database, it will be created.
// If it exists, only hardware-related fields will be updated.
//
// Note: used_vram_bytes is NOT updated here. It's managed by the scheduler when
// capsules are deployed/destroyed (Week 4).
func (s *NodeStore) UpdateNodeHardware(ctx context.Context, info *gpu.RigGpuInfo) error {
	log.Printf("UpdateNodeHardware is deprecated: rig=%s", info.RigID)
	// Preserve backward compatibility by translating to RigStatus
	status := &pb.RigStatus{
		RigId: info.RigID,
		Hardware: &pb.HardwareState{
			TotalVramBytes:    info.TotalVRAMBytes,
			UsedVramBytes:     info.UsedVRAMBytes,
			SystemCudaVersion: info.CudaDriverVersion,
		},
		ReportedAtUnixSeconds: uint64(time.Now().Unix()),
	}
	return s.UpdateNodeStatus(ctx, status)
}

// GetAllGpuRigs retrieves all active nodes with GPU capabilities
//
// This method is used by the scheduler (Week 2) to get a list of available
// nodes for GPU workload placement.
//
// Filters:
// - Only returns nodes that have been seen recently (within last 5 minutes)
// - Only returns nodes with non-zero VRAM
// - Orders by available VRAM (descending) for better scheduler performance
func (s *NodeStore) GetAllGpuRigs(ctx context.Context) ([]*gpu.RigGpuInfo, error) {
	// Get nodes that reported within last 5 minutes and have GPU
	fiveMinutesAgo := time.Now().Add(-5 * time.Minute).Unix()

	query := `
		SELECT id, total_vram_bytes, used_vram_bytes, cuda_driver_version
		FROM nodes
		WHERE last_seen > ?
		  AND total_vram_bytes > 0
		ORDER BY (total_vram_bytes - used_vram_bytes) DESC` // Order by available VRAM

	rows, err := s.db.QueryContext(ctx, query, fiveMinutesAgo)
	if err != nil {
		return nil, fmt.Errorf("failed to query nodes: %w", err)
	}
	defer rows.Close()

	var rigs []*gpu.RigGpuInfo
	for rows.Next() {
		var rig gpu.RigGpuInfo
		err := rows.Scan(
			&rig.RigID,
			&rig.TotalVRAMBytes,
			&rig.UsedVRAMBytes,
			&rig.CudaDriverVersion,
		)
		if err != nil {
			return nil, fmt.Errorf("failed to scan node row: %w", err)
		}
		rigs = append(rigs, &rig)
	}

	if err = rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating rows: %w", err)
	}

	return rigs, nil
}

// ReserveVRAM reserves VRAM on a specific node (called when deploying a capsule)
//
// This is an atomic operation that updates the used_vram_bytes for a node.
// Used by the scheduler in Week 4.
func (s *NodeStore) ReserveVRAM(ctx context.Context, rigID string, vramBytes uint64) error {
	query := `
		UPDATE nodes
		SET
			used_vram_bytes = used_vram_bytes + ?,
			updated_at = ?
		WHERE id = ?`

	_, err := s.db.ExecContext(ctx, query, vramBytes, time.Now().Unix(), rigID)
	if err != nil {
		return fmt.Errorf("failed to reserve VRAM: %w", err)
	}

	return nil
}

// ReplaceNodeWorkloads replaces the stored workload snapshot for a node.
func (s *NodeStore) ReplaceNodeWorkloads(ctx context.Context, nodeID string, workloads []*NodeWorkload) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return fmt.Errorf("begin tx: %w", err)
	}
	defer func() {
		if err != nil {
			if rollbackErr := tx.Rollback(); rollbackErr != nil {
				log.Printf("WARN: failed to rollback ReplaceNodeWorkloads tx: %v", rollbackErr)
			}
		}
	}()

	if _, err = tx.ExecContext(ctx, "DELETE FROM node_workloads WHERE node_id = ?", nodeID); err != nil {
		return fmt.Errorf("delete node workloads: %w", err)
	}

	if len(workloads) > 0 {
		stmt, err := tx.PrepareContext(ctx, `
			INSERT INTO node_workloads (
				node_id,
				workload_id,
				name,
				reserved_vram_bytes,
				observed_vram_bytes,
				pid,
				phase,
				updated_at
			)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
		if err != nil {
			return fmt.Errorf("prepare insert workload: %w", err)
		}
		defer stmt.Close()

		for _, wl := range workloads {
			if wl == nil {
				continue
			}
			if _, err = stmt.ExecContext(ctx,
				nodeID,
				wl.WorkloadID,
				wl.Name,
				wl.ReservedVRAMBytes,
				wl.ObservedVRAMBytes,
				wl.PID,
				wl.Phase,
				wl.UpdatedAt.Unix(),
			); err != nil {
				return fmt.Errorf("insert workload %s: %w", wl.WorkloadID, err)
			}
		}
	}

	if err = tx.Commit(); err != nil {
		return fmt.Errorf("commit ReplaceNodeWorkloads: %w", err)
	}
	return nil
}

// ListNodeWorkloads returns all observed workloads across the cluster.
func (s *NodeStore) ListNodeWorkloads(ctx context.Context) ([]*NodeWorkload, error) {
	rows, err := s.db.QueryContext(ctx, `
		SELECT
			node_id,
			workload_id,
			name,
			reserved_vram_bytes,
			observed_vram_bytes,
			pid,
			phase,
			updated_at
		FROM node_workloads`)
	if err != nil {
		return nil, fmt.Errorf("query node workloads: %w", err)
	}
	defer rows.Close()

	workloads := make([]*NodeWorkload, 0)
	for rows.Next() {
		var wl NodeWorkload
		var updatedAtUnix int64
		if err := rows.Scan(
			&wl.NodeID,
			&wl.WorkloadID,
			&wl.Name,
			&wl.ReservedVRAMBytes,
			&wl.ObservedVRAMBytes,
			&wl.PID,
			&wl.Phase,
			&updatedAtUnix,
		); err != nil {
			return nil, fmt.Errorf("scan node workload: %w", err)
		}
		wl.UpdatedAt = time.Unix(updatedAtUnix, 0)
		workloads = append(workloads, &wl)
	}

	if err = rows.Err(); err != nil {
		return nil, fmt.Errorf("iterate node workloads: %w", err)
	}

	return workloads, nil
}

// ReleaseVRAM releases VRAM on a specific node (called when stopping a capsule)
//
// This is an atomic operation that updates the used_vram_bytes for a node.
// Used by the scheduler in Week 4.
func (s *NodeStore) ReleaseVRAM(ctx context.Context, rigID string, vramBytes uint64) error {
	query := `
		UPDATE nodes
		SET
			used_vram_bytes = CASE
				WHEN used_vram_bytes >= ? THEN used_vram_bytes - ?
				ELSE 0
			END,
			updated_at = ?
		WHERE id = ?`

	_, err := s.db.ExecContext(ctx, query, vramBytes, vramBytes, time.Now().Unix(), rigID)
	if err != nil {
		return fmt.Errorf("failed to release VRAM: %w", err)
	}

	return nil
}

// MarkCapsuleFailed marks a capsule as failed (used when detecting orphan workloads).
func (s *NodeStore) MarkCapsuleFailed(ctx context.Context, capsuleID string) error {
	query := `
		UPDATE capsules
		SET status = ?, updated_at = ?
		WHERE id = ?`

	_, err := s.db.ExecContext(ctx, query, CapsuleStatusFailed, time.Now().Unix(), capsuleID)
	if err != nil {
		return fmt.Errorf("mark capsule %s failed: %w", capsuleID, err)
	}
	return nil
}

// MarkCapsulePending marks a capsule for redeploy (used when Agent misses workload).
func (s *NodeStore) MarkCapsulePending(ctx context.Context, capsuleID string) error {
	query := `
		UPDATE capsules
		SET status = ?, updated_at = ?
		WHERE id = ?`

	_, err := s.db.ExecContext(ctx, query, CapsuleStatusPending, time.Now().Unix(), capsuleID)
	if err != nil {
		return fmt.Errorf("mark capsule %s pending: %w", capsuleID, err)
	}
	return nil
}

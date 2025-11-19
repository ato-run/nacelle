package grpc

import (
	"context"
	"fmt"
	"log"

	"github.com/onescluster/coordinator/pkg/db"
	pb "github.com/onescluster/coordinator/pkg/proto"
)

// Server implements the Coordinator gRPC service
type Server struct {
	pb.UnimplementedCoordinatorServiceServer
	NodeStore *db.NodeStore
}

// NewServer creates a new gRPC server instance
func NewServer(nodeStore *db.NodeStore) *Server {
	return &Server{
		NodeStore: nodeStore,
	}
}

// ReportStatus handles Agent heartbeat reports that contain both hardware state and
// currently running workloads. This replaces the Week 1 hardware-only reporting.
//
// Flow:
// 1. Receive StatusReportRequest from Agent
// 2. Persist hardware metrics and running workloads to NodeStore
// 3. Update scheduler-compatible cache for future placement decisions
// 4. Return acknowledgement
func (s *Server) ReportStatus(ctx context.Context, req *pb.StatusReportRequest) (*pb.StatusReportResponse, error) {
	if req == nil || req.Status == nil {
		return &pb.StatusReportResponse{
			Success: false,
			Message: "status payload is required",
		}, fmt.Errorf("invalid request: empty status")
	}

	status := req.Status
	log.Printf("📡 Received status report from Rig: %s", status.GetRigId())
	if status.GetRigId() == "" {
		return &pb.StatusReportResponse{
			Success: false,
			Message: "status payload must include rig_id",
		}, fmt.Errorf("invalid request: rig_id is required")
	}

	hardware := status.GetHardware()
	if hardware != nil {
		log.Printf("  GPU Count: %d", len(hardware.Gpus))
		if hardware.TotalVramBytes > 0 {
			log.Printf("  Total VRAM: %.2f GB", float64(hardware.TotalVramBytes)/(1024*1024*1024))
		}
		if hardware.UsedVramBytes > 0 {
			log.Printf("  Used VRAM: %.2f GB", float64(hardware.UsedVramBytes)/(1024*1024*1024))
		}
		if hardware.SystemCudaVersion != "" {
			log.Printf("  CUDA Version: %s", hardware.SystemCudaVersion)
		}
	}

	if status.IsMock {
		log.Printf("  Mode: Mock (simulation data)")
	}

	running := status.GetRunningWorkloads()
	if len(running) > 0 {
		log.Printf("  Running workloads: %d", len(running))
	}

	if err := s.NodeStore.UpdateNodeStatus(ctx, status); err != nil {
		log.Printf("❌ Failed to update status for Rig %s: %v", status.GetRigId(), err)
		return &pb.StatusReportResponse{
			Success: false,
			Message: fmt.Sprintf("database error: %v", err),
		}, err
	}

	log.Printf("✅ Successfully updated status for Rig: %s", status.GetRigId())

	return &pb.StatusReportResponse{
		Success: true,
		Message: "Status report received and stored",
	}, nil
}

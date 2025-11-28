package service

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
	pb "github.com/onescluster/coordinator/pkg/proto"
	"google.golang.org/protobuf/types/known/emptypb"
)

type CoordinatorService struct {
	pb.UnimplementedCoordinatorServiceServer
	stateManager *db.StateManager
}

func NewCoordinatorService(stateManager *db.StateManager) *CoordinatorService {
	return &CoordinatorService{
		stateManager: stateManager,
	}
}

func (s *CoordinatorService) RegisterMachine(ctx context.Context, req *pb.RegisterMachineRequest) (*pb.RegisterMachineResponse, error) {
	log.Printf("RegisterMachine: %v", req.Hostname)
	
	machineID := "node-" + fmt.Sprintf("%d", time.Now().UnixNano()) // Generate ID or use existing?
	// In a real system, we might use ULID or UUID.
	// For now, simple ID.
	
	node := &db.Node{
		ID:            machineID,
		Address:       req.TailnetIp, // Use Tailnet IP as address
		HeadscaleName: req.Hostname,
		TailnetIP:     req.TailnetIp,
		Status:        db.NodeStatusActive,
		LastSeen:      time.Now(),
		CreatedAt:     time.Now(),
		UpdatedAt:     time.Now(),
	}
	
	// We also need to save GPUs. UpsertNode only takes Node.
	// But UpdateNodeHeartbeat takes GPUs.
	// RegisterMachineRequest has GPUs.
	// I should probably update UpsertNode to take GPUs or call UpdateNodeHeartbeat immediately after.
	// Or just use UpdateNodeHeartbeat for registration too?
	// RegisterMachine returns auth token.
	
	if err := s.stateManager.UpsertNode(node); err != nil {
		return nil, fmt.Errorf("failed to register node: %w", err)
	}

	// Convert proto GPUs to model GPUs
	var gpus []*db.NodeGpu
	for _, g := range req.Gpus {
		gpus = append(gpus, &db.NodeGpu{
			ID:             fmt.Sprintf("%s-gpu-%d", machineID, g.Index),
			NodeID:         machineID,
			Index:          int(g.Index),
			Name:           g.Name,
			TotalVRAMBytes: g.VramTotalBytes,
			UsedVRAMBytes:  0,
			UpdatedAt:      time.Now(),
		})
	}

	// Update GPUs via Heartbeat logic (reusing it for now as it handles GPU insertion)
	// We pass empty workloads.
	if err := s.stateManager.UpdateNodeHeartbeat(machineID, time.Now(), []*db.NodeWorkload{}, gpus); err != nil {
		log.Printf("Warning: failed to save GPUs during registration: %v", err)
	}

	return &pb.RegisterMachineResponse{
		MachineId: machineID,
		AuthToken: "mock-auth-token", // TODO: Implement auth
	}, nil
}

func (s *CoordinatorService) Heartbeat(ctx context.Context, req *pb.HeartbeatRequest) (*pb.HeartbeatResponse, error) {
	if req.MachineId == "" {
		return nil, fmt.Errorf("machine_id required")
	}

	// Convert proto workloads to model workloads
	var workloads []*db.NodeWorkload
	for _, wl := range req.RunningWorkloads {
		workloads = append(workloads, &db.NodeWorkload{
			NodeID:            req.MachineId,
			WorkloadID:        wl.WorkloadId,
			Name:              wl.Name,
			ReservedVRAMBytes: wl.ReservedVramBytes,
			ObservedVRAMBytes: wl.ObservedVramBytes,
			PID:               int64(wl.Pid),
			Phase:             wl.Phase.String(),
			UpdatedAt:         time.Now(),
		})
	}

	// Convert proto GPU snapshots to model GPUs (if provided)
	var gpus []*db.NodeGpu
	for _, g := range req.GpuSnapshots {
		gpus = append(gpus, &db.NodeGpu{
			ID:             fmt.Sprintf("%s-gpu-%d", req.MachineId, g.Index),
			NodeID:         req.MachineId,
			Index:          int(g.Index),
			Name:           g.Name,
			TotalVRAMBytes: g.VramTotalBytes,
			UsedVRAMBytes:  g.VramTotalBytes - g.VramFreeBytes, // Calculate used
			UpdatedAt:      time.Now(),
		})
	}

	if err := s.stateManager.UpdateNodeHeartbeat(req.MachineId, time.Now(), workloads, gpus); err != nil {
		return nil, fmt.Errorf("failed to update heartbeat: %w", err)
	}

	return &pb.HeartbeatResponse{}, nil
}

func (s *CoordinatorService) ListMachines(ctx context.Context, req *pb.ListMachinesRequest) (*pb.ListMachinesResponse, error) {
	nodes := s.stateManager.GetAllNodes()
	var pbMachines []*pb.Machine
	
	for _, n := range nodes {
		pbMachines = append(pbMachines, &pb.Machine{
			Id:        n.ID,
			Hostname:  n.HeadscaleName,
			TailnetIp: n.TailnetIP,
			Status:    pb.MachineStatus_MACHINE_STATUS_ONLINE, // Map status correctly
			// TODO: Populate GPUs
		})
	}

	return &pb.ListMachinesResponse{
		Machines: pbMachines,
	}, nil
}

func (s *CoordinatorService) GetMachine(ctx context.Context, req *pb.GetMachineRequest) (*pb.Machine, error) {
	node, exists := s.stateManager.GetNode(req.MachineId)
	if !exists {
		return nil, fmt.Errorf("machine not found")
	}
	
	return &pb.Machine{
		Id:        node.ID,
		Hostname:  node.HeadscaleName,
		TailnetIp: node.TailnetIP,
		Status:    pb.MachineStatus_MACHINE_STATUS_ONLINE,
	}, nil
}

func (s *CoordinatorService) DeployCapsule(ctx context.Context, req *pb.DeployCapsuleRequest) (*pb.DeployCapsuleResponse, error) {
	log.Printf("DeployCapsule: %s", req.Name)
	// TODO: Schedule and deploy
	return &pb.DeployCapsuleResponse{
		CapsuleId: "mock-capsule-id",
		MachineId: "mock-machine-id",
		AccessUrl: "http://mock-url",
	}, nil
}

func (s *CoordinatorService) StopCapsule(ctx context.Context, req *pb.StopCapsuleRequest) (*emptypb.Empty, error) {
	log.Printf("StopCapsule: %s", req.CapsuleId)
	return &emptypb.Empty{}, nil
}

func (s *CoordinatorService) GetCapsuleStatus(ctx context.Context, req *pb.GetCapsuleStatusRequest) (*pb.GetCapsuleStatusResponse, error) {
	return &pb.GetCapsuleStatusResponse{}, nil
}

func (s *CoordinatorService) ListCapsules(ctx context.Context, req *pb.ListCapsulesRequest) (*pb.ListCapsulesResponse, error) {
	return &pb.ListCapsulesResponse{}, nil
}

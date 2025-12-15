package service

import (
	"context"
	"fmt"
	"log"
	"os"
	"strings"
	"time"

	"github.com/onescluster/coordinator/pkg/db"
	"github.com/onescluster/coordinator/pkg/networking/caddy"
	"github.com/onescluster/coordinator/pkg/networking/port"
	pb "github.com/onescluster/coordinator/pkg/proto"
	coordinatorv1 "github.com/onescluster/coordinator/pkg/proto/coordinator/v1"
	"github.com/onescluster/coordinator/pkg/spec"
	"github.com/onescluster/coordinator/pkg/store"
)

type routeManager interface {
	AddRoute(capsuleID, host string, upstreamPort int) error
	RemoveRoute(capsuleID string) error
}

var _ routeManager = (*caddy.Client)(nil)

func preferLocalhostURL() bool {
	// Minimal dev switch: keep production behavior unchanged.
	if os.Getenv("GUMBALL_URL_MODE") == "localhost" {
		return true
	}
	return os.Getenv("ENVIRONMENT") == "development"
}

func domainSuffix() string {
	// Use GUMBALL_DOMAIN_SUFFIX if set, otherwise default to "localhost"
	// which resolves to 127.0.0.1 without /etc/hosts modification
	if v := os.Getenv("GUMBALL_DOMAIN_SUFFIX"); v != "" {
		return v
	}
	return "localhost"
}

type CoordinatorService struct {
	coordinatorv1.UnimplementedCoordinatorServiceServer
	dbClient      *db.Client
	stateManager  *db.StateManager
	routeManager  routeManager
	portAllocator *port.Allocator
	engineClient  pb.EngineClient
	store         *store.SQLiteStore
}

func NewCoordinatorService(dbClient *db.Client, stateManager *db.StateManager, routeManager routeManager, portAllocator *port.Allocator, engineClient pb.EngineClient, capsuleStore *store.SQLiteStore) *CoordinatorService {
	return &CoordinatorService{
		dbClient:      dbClient,
		stateManager:  stateManager,
		routeManager:  routeManager,
		portAllocator: portAllocator,
		engineClient:  engineClient,
		store:         capsuleStore,
	}
}

func (s *CoordinatorService) DeployCapsule(ctx context.Context, req *coordinatorv1.DeployRequest) (*coordinatorv1.DeployResponse, error) {
	log.Printf("DeployCapsule requested")

	// 1. Parse TOML
	capsuleID, runPlan, err := spec.ParseRunPlanContent(string(req.TomlContent))
	if err != nil {
		return nil, fmt.Errorf("failed to parse TOML: %w", err)
	}

	if capsuleID == "" {
		return nil, fmt.Errorf("capsule ID missing in spec")
	}

	// Phase 2 naming rule: capsule_id = {user}.{capsule}
	if userID := os.Getenv("GUMBALL_USER_ID"); userID != "" {
		if !strings.Contains(capsuleID, ".") {
			capsuleID = fmt.Sprintf("%s.%s", userID, capsuleID)
		}
	}

	// 2. Allocate Port
	allocatedPort, err := s.portAllocator.Allocate()
	if err != nil {
		return nil, fmt.Errorf("failed to allocate port: %w", err)
	}
	log.Printf("Allocated port %d for capsule %s", allocatedPort, capsuleID)

	// Inject PORT into RunPlan environment
	if runPlan.GetNative() != nil {
		if runPlan.GetNative().Env == nil {
			runPlan.GetNative().Env = make(map[string]string)
		}
		runPlan.GetNative().Env["PORT"] = fmt.Sprintf("%d", allocatedPort)
	} else if runPlan.GetDocker() != nil {
		if runPlan.GetDocker().Env == nil {
			runPlan.GetDocker().Env = make(map[string]string)
		}
		runPlan.GetDocker().Env["PORT"] = fmt.Sprintf("%d", allocatedPort)
		// Also set Ports for DockerCliRuntime to use
		runPlan.GetDocker().Ports = append(runPlan.GetDocker().Ports, &pb.Port{
			ContainerPort: 80, // Default container port
			HostPort:      uint32(allocatedPort),
			Protocol:      "tcp",
		})
	}

	// Inject coordinator-managed egress allowlist so Engine stays policy-agnostic.
	runPlan.EgressAllowlist = mergeAllowlists(runPlan.EgressAllowlist, defaultEgressAllowlist())

	// Public hostname remains {capsule}.{user}.gum-ball.app for compatibility with edge-router parsing.
	// When capsule_id follows {user}.{capsule}, swap order for hostname.
	parts := strings.SplitN(capsuleID, ".", 2)
	hostLabel := capsuleID
	if len(parts) == 2 {
		user := parts[0]
		cap := parts[1]
		hostLabel = fmt.Sprintf("%s.%s", cap, user)
	}
	hostname := fmt.Sprintf("%s.%s", hostLabel, domainSuffix())

	// 3. Configure Caddy Route
	caddyAvailable := false
	if s.routeManager != nil {
		if err := s.routeManager.AddRoute(capsuleID, hostname, allocatedPort); err != nil {
			log.Printf("Warning: failed to configure routing: %v", err)
		} else {
			log.Printf("Configured routing: http://%s -> 127.0.0.1:%d", hostname, allocatedPort)
			caddyAvailable = true
		}
	}

	// 4. Call Engine DeployCapsule
	_, err = s.engineClient.DeployCapsule(ctx, &pb.DeployRequest{
		CapsuleId: capsuleID,
		Manifest: &pb.DeployRequest_RunPlan{
			RunPlan: runPlan,
		},
	})
	if err != nil {
		// Rollback Caddy
		if s.routeManager != nil {
			s.routeManager.RemoveRoute(capsuleID)
		}
		return nil, fmt.Errorf("engine deployment failed: %w", err)
	}

	// Return localhost URL if Caddy is not available or if preference is set
	publicURL := fmt.Sprintf("http://%s", hostname)
	clientURL := publicURL
	if !caddyAvailable || preferLocalhostURL() {
		clientURL = fmt.Sprintf("http://127.0.0.1:%d", allocatedPort)
	}

	// 5. Persist to SQLite
	if s.store != nil {
		capsule := &store.DeployedCapsule{
			ID:        capsuleID,
			Name:      runPlan.Name,
			URL:       publicURL,
			Status:    "Running",
			Port:      allocatedPort,
			CreatedAt: time.Now(),
		}
		if err := s.store.SaveDeployedCapsule(ctx, capsule); err != nil {
			log.Printf("Warning: failed to persist capsule: %v", err)
		}
	}

	return &coordinatorv1.DeployResponse{
		CapsuleId: capsuleID,
		Url:       clientURL,
	}, nil
}

func (s *CoordinatorService) ListCapsules(ctx context.Context, req *coordinatorv1.ListRequest) (*coordinatorv1.ListResponse, error) {
	if s.store == nil {
		return &coordinatorv1.ListResponse{Capsules: []*coordinatorv1.CapsuleInfo{}}, nil
	}

	capsules, err := s.store.ListDeployedCapsules(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to list capsules: %w", err)
	}

	var list []*coordinatorv1.CapsuleInfo
	useLocalhost := preferLocalhostURL()
	for _, c := range capsules {
		url := c.URL
		if useLocalhost && c.Port > 0 {
			url = fmt.Sprintf("http://127.0.0.1:%d", c.Port)
		}
		list = append(list, &coordinatorv1.CapsuleInfo{
			CapsuleId: c.ID,
			Name:      c.Name,
			Status:    c.Status,
			Url:       url,
		})
	}
	return &coordinatorv1.ListResponse{
		Capsules: list,
	}, nil
}

func (s *CoordinatorService) StopCapsule(ctx context.Context, req *coordinatorv1.StopRequest) (*coordinatorv1.StopResponse, error) {
	log.Printf("StopCapsule: %s", req.CapsuleId)

	// 1. Remove Caddy Route
	if s.routeManager != nil {
		if err := s.routeManager.RemoveRoute(req.CapsuleId); err != nil {
			log.Printf("Warning: failed to remove route: %v", err)
		}
	}

	// 2. Call Engine
	_, err := s.engineClient.StopCapsule(ctx, &pb.StopRequest{CapsuleId: req.CapsuleId})
	if err != nil {
		return &coordinatorv1.StopResponse{Success: false, Message: err.Error()}, nil
	}

	// 3. Remove from SQLite
	if s.store != nil {
		if err := s.store.DeleteDeployedCapsule(ctx, req.CapsuleId); err != nil {
			log.Printf("Warning: failed to delete capsule from store: %v", err)
		}
	}

	return &coordinatorv1.StopResponse{Success: true, Message: "Stopped successfully"}, nil
}

func defaultEgressAllowlist() []string {
	raw := strings.TrimSpace(os.Getenv("GUMBALL_EGRESS_ALLOWLIST"))
	if raw == "" {
		return nil
	}

	fields := strings.FieldsFunc(raw, func(r rune) bool {
		return r == ',' || r == '\n' || r == '\t' || r == ' '
	})

	var allowlist []string
	seen := map[string]struct{}{}
	for _, f := range fields {
		entry := strings.TrimSpace(f)
		if entry == "" {
			continue
		}
		if _, ok := seen[entry]; ok {
			continue
		}
		seen[entry] = struct{}{}
		allowlist = append(allowlist, entry)
	}
	return allowlist
}

func mergeAllowlists(base, extra []string) []string {
	if len(extra) == 0 {
		return base
	}

	seen := map[string]struct{}{}
	merged := make([]string, 0, len(base)+len(extra))

	for _, v := range base {
		trimmed := strings.TrimSpace(v)
		if trimmed == "" {
			continue
		}
		if _, ok := seen[trimmed]; ok {
			continue
		}
		seen[trimmed] = struct{}{}
		merged = append(merged, trimmed)
	}

	for _, v := range extra {
		trimmed := strings.TrimSpace(v)
		if trimmed == "" {
			continue
		}
		if _, ok := seen[trimmed]; ok {
			continue
		}
		seen[trimmed] = struct{}{}
		merged = append(merged, trimmed)
	}

	return merged
}

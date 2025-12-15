package service

import (
	"context"
	"reflect"
	"testing"

	"github.com/onescluster/coordinator/pkg/networking/port"
	pb "github.com/onescluster/coordinator/pkg/proto"
	coordinatorv1 "github.com/onescluster/coordinator/pkg/proto/coordinator/v1"
	"google.golang.org/grpc"
)

// stubRouteManager is a lightweight stand-in for the Caddy client to avoid network calls in tests.
type stubRouteManager struct{}

func (s *stubRouteManager) AddRoute(capsuleID, host string, upstreamPort int) error {
	return nil
}

func (s *stubRouteManager) RemoveRoute(capsuleID string) error { return nil }

// stubEngineClient captures DeployCapsule requests for inspection.
type stubEngineClient struct {
	lastReq *pb.DeployRequest
}

func (s *stubEngineClient) DeployCapsule(ctx context.Context, in *pb.DeployRequest, opts ...grpc.CallOption) (*pb.DeployResponse, error) {
	s.lastReq = in
	return &pb.DeployResponse{CapsuleId: in.GetCapsuleId(), LocalUrl: "http://local"}, nil
}

func (s *stubEngineClient) StopCapsule(ctx context.Context, in *pb.StopRequest, opts ...grpc.CallOption) (*pb.StopResponse, error) {
	return nil, nil
}

func (s *stubEngineClient) GetResources(ctx context.Context, in *pb.GetResourcesRequest, opts ...grpc.CallOption) (*pb.ResourceInfo, error) {
	return nil, nil
}

func (s *stubEngineClient) ValidateManifest(ctx context.Context, in *pb.ValidateRequest, opts ...grpc.CallOption) (*pb.ValidationResult, error) {
	return nil, nil
}

func (s *stubEngineClient) GetSystemStatus(ctx context.Context, in *pb.GetSystemStatusRequest, opts ...grpc.CallOption) (*pb.SystemStatus, error) {
	return nil, nil
}

func (s *stubEngineClient) StreamLogs(ctx context.Context, in *pb.LogRequest, opts ...grpc.CallOption) (grpc.ServerStreamingClient[pb.EngineLogEntry], error) {
	return nil, nil
}

func TestDeployCapsuleInjectsDefaultEgressAllowlist(t *testing.T) {
	t.Setenv("GUMBALL_EGRESS_ALLOWLIST", "api.example.com, registry.local")
	t.Setenv("GUMBALL_DOMAIN_SUFFIX", "test.local")

	engine := &stubEngineClient{}
	svc := NewCoordinatorService(nil, nil, &stubRouteManager{}, port.NewAllocator(), engine, nil)

	req := &coordinatorv1.DeployRequest{
		TomlContent: []byte(`
schema_version = "1.0"
name = "demo"
version = "1.0.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/demo:latest"
`),
	}

	_, err := svc.DeployCapsule(context.Background(), req)
	if err != nil {
		t.Fatalf("DeployCapsule returned error: %v", err)
	}

	if engine.lastReq == nil {
		t.Fatalf("engine client did not receive DeployCapsule request")
	}

	got := engine.lastReq.GetRunPlan().GetEgressAllowlist()
	want := []string{"api.example.com", "registry.local"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("egress allowlist mismatch: got %v want %v", got, want)
	}
}

func TestDeployCapsule_UsesUserPrefixedCapsuleID(t *testing.T) {
	t.Setenv("GUMBALL_USER_ID", "alice")
	t.Setenv("GUMBALL_DOMAIN_SUFFIX", "gum-ball.app")

	engine := &stubEngineClient{}
	svc := NewCoordinatorService(nil, nil, &stubRouteManager{}, port.NewAllocator(), engine, nil)

	req := &coordinatorv1.DeployRequest{
		TomlContent: []byte(`
schema_version = "1.0"
name = "demo"
version = "1.0.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/demo:latest"
`),
	}

	resp, err := svc.DeployCapsule(context.Background(), req)
	if err != nil {
		t.Fatalf("DeployCapsule returned error: %v", err)
	}

	if resp.GetCapsuleId() != "alice.demo" {
		t.Fatalf("capsuleId = %q, want %q", resp.GetCapsuleId(), "alice.demo")
	}

	expectedURL := "http://demo.alice.gum-ball.app"
	if resp.GetUrl() != expectedURL {
		t.Fatalf("url = %q, want %q", resp.GetUrl(), expectedURL)
	}

	if engine.lastReq == nil || engine.lastReq.GetCapsuleId() != "alice.demo" {
		t.Fatalf("engine capsule id = %q, want %q", engine.lastReq.GetCapsuleId(), "alice.demo")
	}
}

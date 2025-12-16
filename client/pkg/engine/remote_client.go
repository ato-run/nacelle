package engine

import (
	"context"
	"fmt"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	pb "github.com/onescluster/coordinator/pkg/proto"
)

type RemoteEngineClient struct {
	conn   *grpc.ClientConn
	client pb.EngineClient
	addr   string
}

func NewRemoteEngineClient(addr string) (*RemoteEngineClient, error) {
	conn, err := grpc.NewClient(addr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to engine at %s: %w", addr, err)
	}

	return &RemoteEngineClient{
		conn:   conn,
		client: pb.NewEngineClient(conn),
		addr:   addr,
	}, nil
}

func (c *RemoteEngineClient) DeployCapsule(
	ctx context.Context,
	req *pb.DeployRequest,
) (*pb.DeployResponse, error) {
	return c.client.DeployCapsule(ctx, req)
}

func (c *RemoteEngineClient) StopCapsule(
	ctx context.Context,
	req *pb.StopRequest,
) (*pb.StopResponse, error) {
	return c.client.StopCapsule(ctx, req)
}

func (c *RemoteEngineClient) GetResources(
	ctx context.Context,
) (*pb.ResourceInfo, error) {
	return c.client.GetResources(ctx, &pb.GetResourcesRequest{})
}

func (c *RemoteEngineClient) Close() error {
	return c.conn.Close()
}

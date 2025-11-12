package master

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/oklog/ulid/v2"
	"github.com/onescluster/coordinator/pkg/headscale"
)

type mockStateManager struct {
	masterID      string
	recordedQuery string
	setMasterErr  error
	executeErr    error
}

func newMockStateManager() *mockStateManager {
	return &mockStateManager{}
}

func (m *mockStateManager) SetMaster(masterID string) error {
	if m.setMasterErr != nil {
		return m.setMasterErr
	}
	m.masterID = masterID
	return nil
}

func (m *mockStateManager) ExecuteRaw(query string, _ ...interface{}) error {
	if m.executeErr != nil {
		return m.executeErr
	}
	m.recordedQuery = query
	return nil
}

// MockHeadscaleClient implements a mock headscale client for testing
type MockHeadscaleClient struct {
	quorumSize int
	shouldFail bool
	failCount  int
	callCount  int
}

func (m *MockHeadscaleClient) GetQuorumSize(ctx context.Context) (int, error) {
	m.callCount++

	if m.shouldFail {
		if m.failCount == 0 || m.callCount <= m.failCount {
			return 0, errors.New("headscale API error")
		}
	}

	return m.quorumSize, nil
}

func (m *MockHeadscaleClient) ListNodes(ctx context.Context) ([]headscale.Node, error) {
	return nil, nil
}

func (m *MockHeadscaleClient) GetNodeByName(ctx context.Context, name string) (*headscale.Node, error) {
	return nil, nil
}

func (m *MockHeadscaleClient) IsHealthy(ctx context.Context) error {
	return nil
}

func TestElectMaster(t *testing.T) {
	tests := []struct {
		name           string
		aliveNodes     []string
		selfNodeID     string
		quorumSize     int
		expectError    bool
		expectedMaster string
	}{
		{
			name: "simple election with 3 nodes",
			aliveNodes: []string{
				"01HQZW5G8QRXM3K9P2N1F4J7YE",
				"01HQZW5G8QRXM3K9P2N1F4J7YA",
				"01HQZW5G8QRXM3K9P2N1F4J7YZ",
			},
			selfNodeID:     "01HQZW5G8QRXM3K9P2N1F4J7YE",
			quorumSize:     3,
			expectError:    false,
			expectedMaster: "01HQZW5G8QRXM3K9P2N1F4J7YA", // Smallest ULID
		},
		{
			name:        "no alive nodes",
			aliveNodes:  []string{},
			selfNodeID:  "01HQZW5G8QRXM3K9P2N1F4J7YE",
			quorumSize:  3,
			expectError: true,
		},
		{
			name: "insufficient quorum",
			aliveNodes: []string{
				"01HQZW5G8QRXM3K9P2N1F4J7YE",
			},
			selfNodeID:  "01HQZW5G8QRXM3K9P2N1F4J7YE",
			quorumSize:  5, // Need 3 nodes for quorum (5/2+1), but only have 1
			expectError: true,
		},
		{
			name: "single node cluster",
			aliveNodes: []string{
				"01HQZW5G8QRXM3K9P2N1F4J7YE",
			},
			selfNodeID:     "01HQZW5G8QRXM3K9P2N1F4J7YE",
			quorumSize:     1,
			expectError:    false,
			expectedMaster: "01HQZW5G8QRXM3K9P2N1F4J7YE",
		},
		{
			name: "election with exact quorum",
			aliveNodes: []string{
				"01HQZW5G8QRXM3K9P2N1F4J7YE",
				"01HQZW5G8QRXM3K9P2N1F4J7YA",
				"01HQZW5G8QRXM3K9P2N1F4J7YZ",
			},
			selfNodeID:     "01HQZW5G8QRXM3K9P2N1F4J7YE",
			quorumSize:     5, // Need 3 nodes for quorum (5/2+1), have exactly 3
			expectError:    false,
			expectedMaster: "01HQZW5G8QRXM3K9P2N1F4J7YA",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockState := newMockStateManager()
			mockHeadscale := &MockHeadscaleClient{
				quorumSize: tt.quorumSize,
			}

			selfNodeID := tt.selfNodeID
			if selfNodeID == "" {
				if len(tt.aliveNodes) > 0 {
					selfNodeID = tt.aliveNodes[0]
				} else {
					selfNodeID = "01HQZW5G8QRXM3K9P2N1F4J7YE"
				}
			}

			elector := NewElector(ElectorConfig{
				NodeID:       selfNodeID,
				StateManager: mockState,
				HeadscaleAPI: mockHeadscale,
				MaxRetries:   3,
				RetryDelay:   100 * time.Millisecond,
			})

			ctx := context.Background()
			masterID, err := elector.ElectMaster(ctx, tt.aliveNodes)

			if tt.expectError {
				if err == nil {
					t.Errorf("Expected error but got none")
				}
				return
			}

			if err != nil {
				t.Errorf("Unexpected error: %v", err)
				return
			}

			if masterID != tt.expectedMaster {
				t.Errorf("Expected master %s, got %s", tt.expectedMaster, masterID)
			}

			// Verify state was updated
			if mockState.masterID != tt.expectedMaster {
				t.Errorf("State manager not updated correctly. Expected %s, got %s",
					tt.expectedMaster, mockState.masterID)
			}

			// Verify IsMaster flag is correct
			expectedIsMaster := (selfNodeID == tt.expectedMaster)
			if elector.IsMaster() != expectedIsMaster {
				t.Errorf("IsMaster flag incorrect. Expected %v, got %v",
					expectedIsMaster, elector.IsMaster())
			}
		})
	}
}

func TestElectMasterWithULIDComparison(t *testing.T) {
	// Create ULIDs with known timestamps for deterministic testing
	now := time.Now()

	node1 := ulid.MustNew(ulid.Timestamp(now.Add(-3*time.Hour)), ulid.DefaultEntropy()).String()
	node2 := ulid.MustNew(ulid.Timestamp(now.Add(-2*time.Hour)), ulid.DefaultEntropy()).String()
	node3 := ulid.MustNew(ulid.Timestamp(now.Add(-1*time.Hour)), ulid.DefaultEntropy()).String()

	aliveNodes := []string{node3, node1, node2} // Intentionally out of order

	mockState := newMockStateManager()
	mockHeadscale := &MockHeadscaleClient{
		quorumSize: 3,
	}

	elector := NewElector(ElectorConfig{
		NodeID:       node1,
		StateManager: mockState,
		HeadscaleAPI: mockHeadscale,
	})

	ctx := context.Background()
	masterID, err := elector.ElectMaster(ctx, aliveNodes)

	if err != nil {
		t.Fatalf("Unexpected error: %v", err)
	}

	// node1 has the earliest timestamp, so it should be elected
	if masterID != node1 {
		t.Errorf("Expected node1 (%s) to be elected, but got %s", node1, masterID)
	}
}

func TestElectorRetryLogic(t *testing.T) {
	mockState := newMockStateManager()
	mockHeadscale := &MockHeadscaleClient{
		quorumSize: 3,
		shouldFail: true,
		failCount:  2, // Fail first 2 attempts, succeed on 3rd
	}

	elector := NewElector(ElectorConfig{
		NodeID:       "01HQZW5G8QRXM3K9P2N1F4J7YE",
		StateManager: mockState,
		HeadscaleAPI: mockHeadscale,
		MaxRetries:   3,
		RetryDelay:   10 * time.Millisecond, // Short delay for test speed
	})

	ctx := context.Background()
	aliveNodes := []string{
		"01HQZW5G8QRXM3K9P2N1F4J7YE",
		"01HQZW5G8QRXM3K9P2N1F4J7YA",
		"01HQZW5G8QRXM3K9P2N1F4J7YZ",
	}

	start := time.Now()
	_, err := elector.ElectMaster(ctx, aliveNodes)
	elapsed := time.Since(start)

	if err != nil {
		t.Errorf("Expected election to succeed after retries, but got error: %v", err)
	}

	// Should have called headscale API 3 times (2 failures + 1 success)
	if mockHeadscale.callCount != 3 {
		t.Errorf("Expected 3 API calls, got %d", mockHeadscale.callCount)
	}

	// Should have taken at least 20ms (2 retries * 10ms delay)
	if elapsed < 20*time.Millisecond {
		t.Errorf("Retries didn't happen, elapsed time too short: %v", elapsed)
	}
}

func TestElectorDegradedMode(t *testing.T) {
	mockState := newMockStateManager()
	mockHeadscale := &MockHeadscaleClient{
		quorumSize: 3,
		shouldFail: true,
		failCount:  10, // Always fail
	}

	elector := NewElector(ElectorConfig{
		NodeID:       "01HQZW5G8QRXM3K9P2N1F4J7YE",
		StateManager: mockState,
		HeadscaleAPI: mockHeadscale,
		MaxRetries:   2,
		RetryDelay:   10 * time.Millisecond,
	})

	ctx := context.Background()
	aliveNodes := []string{
		"01HQZW5G8QRXM3K9P2N1F4J7YE",
		"01HQZW5G8QRXM3K9P2N1F4J7YA",
	}

	_, err := elector.ElectMaster(ctx, aliveNodes)

	if err == nil {
		t.Error("Expected error when entering degraded mode")
	}

	if !elector.IsDegraded() {
		t.Error("Expected elector to be in degraded mode")
	}
}

func TestValidateAndRecoverMaster(t *testing.T) {
	mockState := newMockStateManager()
	mockHeadscale := &MockHeadscaleClient{
		quorumSize: 3,
	}

	elector := NewElector(ElectorConfig{
		NodeID:       "01HQZW5G8QRXM3K9P2N1F4J7YE",
		StateManager: mockState,
		HeadscaleAPI: mockHeadscale,
	})

	ctx := context.Background()
	initialNodes := []string{
		"01HQZW5G8QRXM3K9P2N1F4J7YA", // Will become master
		"01HQZW5G8QRXM3K9P2N1F4J7YE",
		"01HQZW5G8QRXM3K9P2N1F4J7YZ",
	}

	// Initial election
	_, err := elector.ElectMaster(ctx, initialNodes)
	if err != nil {
		t.Fatalf("Initial election failed: %v", err)
	}

	initialMaster := elector.GetMasterID()
	if initialMaster != "01HQZW5G8QRXM3K9P2N1F4J7YA" {
		t.Fatalf("Unexpected initial master: %s", initialMaster)
	}

	// Simulate master node failure
	nodesAfterFailure := []string{
		"01HQZW5G8QRXM3K9P2N1F4J7YE",
		"01HQZW5G8QRXM3K9P2N1F4J7YZ",
	}

	err = elector.ValidateAndRecoverMaster(ctx, nodesAfterFailure)
	if err != nil {
		t.Errorf("Recovery failed: %v", err)
	}

	// Master should have changed
	newMaster := elector.GetMasterID()
	if newMaster == initialMaster {
		t.Error("Master should have changed after failure")
	}

	if newMaster != "01HQZW5G8QRXM3K9P2N1F4J7YE" {
		t.Errorf("Expected new master to be 01HQZW5G8QRXM3K9P2N1F4J7YE, got %s", newMaster)
	}
}

func TestInvalidULIDHandling(t *testing.T) {
	mockState := newMockStateManager()
	mockHeadscale := &MockHeadscaleClient{
		quorumSize: 2,
	}

	elector := NewElector(ElectorConfig{
		NodeID:       "invalid-ulid",
		StateManager: mockState,
		HeadscaleAPI: mockHeadscale,
	})

	ctx := context.Background()
	aliveNodes := []string{
		"invalid-ulid",
		"also-not-valid",
	}

	_, err := elector.ElectMaster(ctx, aliveNodes)

	if err == nil {
		t.Error("Expected error when all ULIDs are invalid")
	}
}

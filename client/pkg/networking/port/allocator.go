package port

import (
	"fmt"
	"sync"
)

const (
	MinPort = 20000
	MaxPort = 30000
)

// Allocator manages port assignment for capsules.
type Allocator struct {
	mu        sync.Mutex
	usedPorts map[int]bool
}

// NewAllocator creates a new port allocator.
func NewAllocator() *Allocator {
	return &Allocator{
		usedPorts: make(map[int]bool),
	}
}

// Allocate finds a free port in the range [MinPort, MaxPort].
// It checks the internal map only.
//
// NOTE: We intentionally do NOT pre-bind to probe availability here.
// On macOS, rapidly bind+close in a probe can cause the subsequent real
// server bind to fail with "address already in use" in practice.
func (a *Allocator) Allocate() (int, error) {
	a.mu.Lock()
	defer a.mu.Unlock()

	for port := MinPort; port <= MaxPort; port++ {
		if a.usedPorts[port] {
			continue
		}

		a.usedPorts[port] = true
		return port, nil
	}

	return 0, fmt.Errorf("no available ports in range %d-%d", MinPort, MaxPort)
}

// Release frees a previously allocated port.
func (a *Allocator) Release(port int) {
	a.mu.Lock()
	defer a.mu.Unlock()
	delete(a.usedPorts, port)
}

// Reserve marks a specific port as used. useful for restoring state.
func (a *Allocator) Reserve(port int) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	if a.usedPorts[port] {
		return fmt.Errorf("port %d is already allocated by internal tracker", port)
	}
    // We don't check OS availability strictly here because we might be restoring state 
    // where the process is already running.
	a.usedPorts[port] = true
	return nil
}



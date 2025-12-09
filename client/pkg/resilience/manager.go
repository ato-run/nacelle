package resilience

import (
	"sync"
	"time"
)

// Manager manages multiple circuit breakers
type Manager struct {
	mu       sync.RWMutex
	breakers map[string]*CircuitBreaker
}

// NewManager creates a new resilience manager
func NewManager() *Manager {
	return &Manager{
		breakers: make(map[string]*CircuitBreaker),
	}
}

// GetOrCreate gets or creates a circuit breaker
func (m *Manager) GetOrCreate(name string, maxFailures int, timeout time.Duration) *CircuitBreaker {
	m.mu.Lock()
	defer m.mu.Unlock()

	if cb, ok := m.breakers[name]; ok {
		return cb
	}

	cb := NewCircuitBreaker(name, maxFailures, timeout)
	m.breakers[name] = cb
	return cb
}

// Get retrieves a circuit breaker by name
func (m *Manager) Get(name string) (*CircuitBreaker, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	cb, ok := m.breakers[name]
	return cb, ok
}

// Stats returns stats for all circuit breakers
func (m *Manager) Stats() map[string]State {
	m.mu.RLock()
	defer m.mu.RUnlock()

	stats := make(map[string]State)
	for name, cb := range m.breakers {
		stats[name] = cb.State()
	}
	return stats
}

// Default circuit breakers for common services
var (
	EngineCircuitBreaker   *CircuitBreaker
	SupabaseCircuitBreaker *CircuitBreaker
	StripeCircuitBreaker   *CircuitBreaker
)

func init() {
	EngineCircuitBreaker = NewCircuitBreaker("engine", 5, 30*time.Second)
	SupabaseCircuitBreaker = NewCircuitBreaker("supabase", 3, 15*time.Second)
	StripeCircuitBreaker = NewCircuitBreaker("stripe", 3, 30*time.Second)
}

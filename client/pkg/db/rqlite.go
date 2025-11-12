package db

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/rqlite/gorqlite"
)

// Config holds rqlite connection configuration
type Config struct {
	// Addresses is a list of rqlite node addresses (e.g., ["http://localhost:4001"])
	Addresses []string
	// MaxRetries is the maximum number of connection retry attempts
	MaxRetries int
	// RetryDelay is the delay between retry attempts
	RetryDelay time.Duration
	// Timeout is the timeout for individual requests
	Timeout time.Duration
}

// Client wraps the rqlite connection with retry and reconnection logic
type Client struct {
	config *Config
	conn   *gorqlite.Connection
	mu     sync.RWMutex
}

// NewClient creates a new rqlite client with the given configuration
func NewClient(config *Config) (*Client, error) {
	if len(config.Addresses) == 0 {
		return nil, fmt.Errorf("at least one rqlite address is required")
	}

	// Set default values if not provided
	if config.MaxRetries == 0 {
		config.MaxRetries = 3
	}
	if config.RetryDelay == 0 {
		config.RetryDelay = 2 * time.Second
	}
	if config.Timeout == 0 {
		config.Timeout = 10 * time.Second
	}

	client := &Client{
		config: config,
	}

	// Establish initial connection
	if err := client.connect(); err != nil {
		return nil, fmt.Errorf("failed to connect to rqlite: %w", err)
	}

	return client, nil
}

// connect establishes a connection to rqlite with retry logic
func (c *Client) connect() error {
	var lastErr error

	for attempt := 0; attempt <= c.config.MaxRetries; attempt++ {
		if attempt > 0 {
			time.Sleep(c.config.RetryDelay)
		}

		// Try each address in the configuration
		for _, addr := range c.config.Addresses {
			conn, err := gorqlite.Open(addr)
			if err != nil {
				lastErr = err
				continue
			}

			// Test the connection
			_, err = conn.Leader()
			if err != nil {
				lastErr = err
				continue
			}

			c.mu.Lock()
			c.conn = conn
			c.mu.Unlock()

			return nil
		}
	}

	return fmt.Errorf("failed to connect after %d attempts: %w", c.config.MaxRetries, lastErr)
}

// Execute runs a write query (INSERT, UPDATE, DELETE) with automatic retry on failure
func (c *Client) Execute(query string, args ...interface{}) error {
	c.mu.RLock()
	conn := c.conn
	c.mu.RUnlock()

	if conn == nil {
		if err := c.connect(); err != nil {
			return fmt.Errorf("connection unavailable: %w", err)
		}
		c.mu.RLock()
		conn = c.conn
		c.mu.RUnlock()
	}

	_, err := conn.WriteOne(query)
	if err != nil {
		// Try to reconnect and retry once
		if reconnectErr := c.connect(); reconnectErr != nil {
			return fmt.Errorf("failed to execute query and reconnect: %w", err)
		}
		c.mu.RLock()
		conn = c.conn
		c.mu.RUnlock()
		_, err = conn.WriteOne(query)
	}

	return err
}

// ExecuteMany runs multiple write queries in a transaction
func (c *Client) ExecuteMany(queries []string) error {
	c.mu.RLock()
	conn := c.conn
	c.mu.RUnlock()

	if conn == nil {
		if err := c.connect(); err != nil {
			return fmt.Errorf("connection unavailable: %w", err)
		}
		c.mu.RLock()
		conn = c.conn
		c.mu.RUnlock()
	}

	_, err := conn.Write(queries)
	if err != nil {
		// Try to reconnect and retry once
		if reconnectErr := c.connect(); reconnectErr != nil {
			return fmt.Errorf("failed to execute queries and reconnect: %w", err)
		}
		c.mu.RLock()
		conn = c.conn
		c.mu.RUnlock()
		_, err = conn.Write(queries)
	}

	return err
}

// Query runs a read query (SELECT) with automatic retry on failure
func (c *Client) Query(query string, args ...interface{}) (gorqlite.QueryResult, error) {
	c.mu.RLock()
	conn := c.conn
	c.mu.RUnlock()

	if conn == nil {
		if err := c.connect(); err != nil {
			return gorqlite.QueryResult{}, fmt.Errorf("connection unavailable: %w", err)
		}
		c.mu.RLock()
		conn = c.conn
		c.mu.RUnlock()
	}

	results, err := conn.QueryOne(query)
	if err != nil {
		// Try to reconnect and retry once
		if reconnectErr := c.connect(); reconnectErr != nil {
			return gorqlite.QueryResult{}, fmt.Errorf("failed to query and reconnect: %w", err)
		}
		c.mu.RLock()
		conn = c.conn
		c.mu.RUnlock()
		results, err = conn.QueryOne(query)
	}

	return results, err
}

// QueryContext runs a read query with context support
func (c *Client) QueryContext(ctx context.Context, query string) (gorqlite.QueryResult, error) {
	// Simple implementation - gorqlite doesn't directly support context
	// but we can check if context is cancelled
	select {
	case <-ctx.Done():
		return gorqlite.QueryResult{}, ctx.Err()
	default:
		return c.Query(query)
	}
}

// Leader returns the current leader address
func (c *Client) Leader() (string, error) {
	c.mu.RLock()
	conn := c.conn
	c.mu.RUnlock()

	if conn == nil {
		if err := c.connect(); err != nil {
			return "", fmt.Errorf("connection unavailable: %w", err)
		}
		c.mu.RLock()
		conn = c.conn
		c.mu.RUnlock()
	}

	return conn.Leader()
}

// Close closes the rqlite connection
func (c *Client) Close() error {
	c.mu.Lock()
	defer c.mu.Unlock()

	if c.conn != nil {
		c.conn = nil
	}

	return nil
}

// Ping checks if the connection is alive
func (c *Client) Ping() error {
	_, err := c.Leader()
	return err
}

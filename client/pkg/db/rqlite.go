package db

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/rqlite/gorqlite"

	"github.com/onescluster/coordinator/pkg/util"
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
	operation := func() error {
		// Try each address in the configuration
		for _, addr := range c.config.Addresses {
			conn, err := gorqlite.Open(addr)
			if err != nil {
				continue
			}

			// Test the connection
			_, err = conn.Leader()
			if err != nil {
				continue
			}

			c.mu.Lock()
			c.conn = conn
			c.mu.Unlock()

			return nil
		}
		return fmt.Errorf("failed to connect to any rqlite address")
	}

	return util.RetryWithBackoff(operation, uint64(c.config.MaxRetries))
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

	results, err := conn.Write(queries)
	if err != nil {
		// Attempt a single reconnect and retry to handle transient connection drops.
		if reconnectErr := c.connect(); reconnectErr == nil {
			c.mu.RLock()
			conn = c.conn
			c.mu.RUnlock()
			if retryResults, retryErr := conn.Write(queries); retryErr == nil {
				results = retryResults
				err = nil
			} else {
				results = retryResults
				err = retryErr
			}
		}
	}

	collectErrors := func(res []gorqlite.WriteResult) []string {
		var errs []string
		for i, r := range res {
			if r.Err != nil {
				errMsg := fmt.Sprintf("statement %d error: %v SQL: %s", i, r.Err, queries[i])
				errMsg = strings.ReplaceAll(errMsg, "\n", " ")
				errMsg = strings.ReplaceAll(errMsg, "\t", " ")
				errMsg = strings.Join(strings.Fields(errMsg), " ")
				fmt.Printf("%s\n", errMsg)
				errs = append(errs, errMsg)
			}
		}
		return errs
	}

	if err != nil {
		errDetails := collectErrors(results)
		if len(errDetails) > 0 {
			return fmt.Errorf("%w; details: %s", err, strings.Join(errDetails, " | "))
		}
		return err
	}

	if errs := collectErrors(results); len(errs) > 0 {
		return fmt.Errorf("statement errors: %s", strings.Join(errs, " | "))
	}

	return nil
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

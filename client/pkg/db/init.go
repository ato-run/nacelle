package db

import (
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// InitSchema initializes the database schema in rqlite
func InitSchema(client *Client) error {
	log.Println("Initializing rqlite schema...")

	// Read schema file
	schemaPath := filepath.Join("pkg", "db", "schema.sql")
	schemaBytes, err := os.ReadFile(schemaPath)
	if err != nil {
		return fmt.Errorf("failed to read schema file: %w", err)
	}

	schema := string(schemaBytes)

	// Split schema into individual statements (rqlite doesn't support multi-statement exec)
	statements := splitSQLStatements(schema)

	// Execute schema statements
	if err := client.ExecuteMany(statements); err != nil {
		return fmt.Errorf("failed to execute schema: %w", err)
	}

	log.Println("Schema initialized successfully")
	return nil
}

// VerifySchema verifies that all required tables exist
func VerifySchema(client *Client) error {
	requiredTables := []string{
		"nodes",
		"capsules",
		"node_resources",
		"capsule_resources",
		"master_elections",
		"cluster_metadata",
		"runtimes",
		"runtime_versions",
	}

	for _, table := range requiredTables {
		result, err := client.Query(fmt.Sprintf("SELECT name FROM sqlite_master WHERE type='table' AND name='%s'", table))
		if err != nil {
			return fmt.Errorf("failed to verify table %s: %w", table, err)
		}

		if !result.Next() {
			return fmt.Errorf("required table '%s' does not exist", table)
		}
	}

	log.Println("Schema verification successful")
	return nil
}

// ApplyMigrations applies SQL migrations from the migrations directory
func ApplyMigrations(client *Client) error {
	log.Println("Applying database migrations...")

	migrationsDir := filepath.Join("pkg", "db", "migrations")
	files, err := os.ReadDir(migrationsDir)
	if err != nil {
		return fmt.Errorf("failed to read migrations directory: %w", err)
	}

	// Ensure deterministic order (001, 002, ...)
	sort.Slice(files, func(i, j int) bool { return files[i].Name() < files[j].Name() })

	for _, file := range files {
		if filepath.Ext(file.Name()) != ".sql" {
			continue
		}

		migrationPath := filepath.Join(migrationsDir, file.Name())
		migrationBytes, err := os.ReadFile(migrationPath)
		if err != nil {
			return fmt.Errorf("failed to read migration %s: %w", file.Name(), err)
		}

		// Split migration into individual statements
		statements := splitSQLStatements(string(migrationBytes))

		if err := client.ExecuteMany(statements); err != nil {
			msg := err.Error()
			// Allow idempotent reruns (duplicate columns/tables)
			if strings.Contains(msg, "duplicate column name") || strings.Contains(msg, "already exists") {
				log.Printf("Migration %s had benign duplicate errors, continuing: %v", file.Name(), err)
				continue
			}
			return fmt.Errorf("failed to execute migration %s: %w", file.Name(), err)
		}

		log.Printf("Migration applied: %s", file.Name())
	}

	log.Println("All migrations applied successfully")
	return nil
}

// MigrateFromSQLite migrates data from SQLite to rqlite
// Note: This is a placeholder for future implementation if needed
func MigrateFromSQLite(sqlitePath string, client *Client) error {
	// Check if SQLite file exists
	if _, err := os.Stat(sqlitePath); os.IsNotExist(err) {
		log.Println("No SQLite database found to migrate from")
		return nil
	}

	log.Printf("Migration from SQLite at %s is not yet implemented", sqlitePath)
	// TODO: Implement migration logic if needed
	// This would involve:
	// 1. Opening SQLite database
	// 2. Reading all data
	// 3. Inserting into rqlite
	// 4. Verifying data integrity

	return nil
}

// splitSQLStatements splits a SQL script into individual statements
// Handles semicolon-separated statements and basic comment removal
func splitSQLStatements(sql string) []string {
	var statements []string
	var current string
	lines := strings.Split(sql, "\n")

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)

		// Skip empty lines and comments
		if trimmed == "" || strings.HasPrefix(trimmed, "--") {
			continue
		}

		current += line + "\n"

		// Check if line ends with semicolon (statement terminator)
		if strings.HasSuffix(trimmed, ";") {
			stmt := strings.TrimSpace(current)
			if stmt != "" && stmt != ";" {
				statements = append(statements, stmt)
			}
			current = ""
		}
	}

	// Add any remaining statement
	if current := strings.TrimSpace(current); current != "" && current != ";" {
		statements = append(statements, current)
	}

	return statements
}

// InitializeClusterState initializes the cluster state with default values
func InitializeClusterState(sm *StateManager) error {
	log.Println("Initializing cluster state...")

	// Set default metadata if not exists
	if _, exists := sm.GetMetadata("cluster_version"); !exists {
		if err := sm.SetMetadata("cluster_version", "1.0.0"); err != nil {
			return fmt.Errorf("failed to set cluster_version: %w", err)
		}
	}

	if _, exists := sm.GetMetadata("cluster_name"); !exists {
		if err := sm.SetMetadata("cluster_name", "capsuled-cluster"); err != nil {
			return fmt.Errorf("failed to set cluster_name: %w", err)
		}
	}

	log.Println("Cluster state initialized")
	return nil
}

// HealthCheck performs a comprehensive health check of the state management system
func HealthCheck(client *Client, sm *StateManager) error {
	// Check rqlite connection
	if err := client.Ping(); err != nil {
		return fmt.Errorf("rqlite connection failed: %w", err)
	}

	// Verify schema
	if err := VerifySchema(client); err != nil {
		return fmt.Errorf("schema verification failed: %w", err)
	}

	// Check state manager
	stats := sm.Stats()
	log.Printf("Health check passed - Stats: %+v", stats)

	return nil
}

package db

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/oklog/ulid/v2"
)

// CapsuleStore handles database operations for capsules
type CapsuleStore struct {
	db *sql.DB
}

// NewCapsuleStore creates a new capsule store
func NewCapsuleStore(db *sql.DB) *CapsuleStore {
	return &CapsuleStore{db: db}
}

// Create creates a new capsule in the database
func (s *CapsuleStore) Create(ctx context.Context, capsule *Capsule) error {
	if capsule.ID == "" {
		capsule.ID = ulid.Make().String()
	}

	now := time.Now()
	if capsule.CreatedAt.IsZero() {
		capsule.CreatedAt = now
	}
	capsule.UpdatedAt = now

	query := `
		INSERT INTO capsules (
			id, name, node_id, manifest, status,
			storage_path, bundle_path, network_config,
			created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`

	_, err := s.db.ExecContext(ctx, query,
		capsule.ID,
		capsule.Name,
		capsule.NodeID,
		capsule.Manifest,
		capsule.Status,
		capsule.StoragePath,
		capsule.BundlePath,
		capsule.NetworkConfig,
		capsule.CreatedAt.Unix(),
		capsule.UpdatedAt.Unix(),
	)

	if err != nil {
		return fmt.Errorf("failed to create capsule: %w", err)
	}

	return nil
}

// Get retrieves a capsule by ID
func (s *CapsuleStore) Get(ctx context.Context, id string) (*Capsule, error) {
	query := `
		SELECT id, name, node_id, manifest, status,
		       storage_path, bundle_path, network_config,
		       created_at, updated_at
		FROM capsules
		WHERE id = ?`

	var capsule Capsule
	var createdAt, updatedAt int64

	err := s.db.QueryRowContext(ctx, query, id).Scan(
		&capsule.ID,
		&capsule.Name,
		&capsule.NodeID,
		&capsule.Manifest,
		&capsule.Status,
		&capsule.StoragePath,
		&capsule.BundlePath,
		&capsule.NetworkConfig,
		&createdAt,
		&updatedAt,
	)

	if err == sql.ErrNoRows {
		return nil, fmt.Errorf("capsule not found: %s", id)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to get capsule: %w", err)
	}

	capsule.CreatedAt = time.Unix(createdAt, 0)
	capsule.UpdatedAt = time.Unix(updatedAt, 0)

	return &capsule, nil
}

// List retrieves all capsules, optionally filtered by node ID or status
func (s *CapsuleStore) List(ctx context.Context, nodeID string, status CapsuleStatus) ([]*Capsule, error) {
	query := `
		SELECT id, name, node_id, manifest, status,
		       storage_path, bundle_path, network_config,
		       created_at, updated_at
		FROM capsules
		WHERE 1=1`

	args := []interface{}{}

	if nodeID != "" {
		query += " AND node_id = ?"
		args = append(args, nodeID)
	}

	if status != "" {
		query += " AND status = ?"
		args = append(args, status)
	}

	query += " ORDER BY created_at DESC"

	rows, err := s.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("failed to list capsules: %w", err)
	}
	defer rows.Close()

	capsules := []*Capsule{}
	for rows.Next() {
		var capsule Capsule
		var createdAt, updatedAt int64

		err := rows.Scan(
			&capsule.ID,
			&capsule.Name,
			&capsule.NodeID,
			&capsule.Manifest,
			&capsule.Status,
			&capsule.StoragePath,
			&capsule.BundlePath,
			&capsule.NetworkConfig,
			&createdAt,
			&updatedAt,
		)

		if err != nil {
			return nil, fmt.Errorf("failed to scan capsule: %w", err)
		}

		capsule.CreatedAt = time.Unix(createdAt, 0)
		capsule.UpdatedAt = time.Unix(updatedAt, 0)

		capsules = append(capsules, &capsule)
	}

	if err = rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating capsules: %w", err)
	}

	return capsules, nil
}

// Update updates an existing capsule
func (s *CapsuleStore) Update(ctx context.Context, capsule *Capsule) error {
	capsule.UpdatedAt = time.Now()

	query := `
		UPDATE capsules
		SET name = ?,
		    node_id = ?,
		    manifest = ?,
		    status = ?,
		    storage_path = ?,
		    bundle_path = ?,
		    network_config = ?,
		    updated_at = ?
		WHERE id = ?`

	result, err := s.db.ExecContext(ctx, query,
		capsule.Name,
		capsule.NodeID,
		capsule.Manifest,
		capsule.Status,
		capsule.StoragePath,
		capsule.BundlePath,
		capsule.NetworkConfig,
		capsule.UpdatedAt.Unix(),
		capsule.ID,
	)

	if err != nil {
		return fmt.Errorf("failed to update capsule: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}

	if rowsAffected == 0 {
		return fmt.Errorf("capsule not found: %s", capsule.ID)
	}

	return nil
}

// UpdateStatus updates only the status of a capsule
func (s *CapsuleStore) UpdateStatus(ctx context.Context, id string, status CapsuleStatus) error {
	query := `
		UPDATE capsules
		SET status = ?,
		    updated_at = ?
		WHERE id = ?`

	result, err := s.db.ExecContext(ctx, query,
		status,
		time.Now().Unix(),
		id,
	)

	if err != nil {
		return fmt.Errorf("failed to update capsule status: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}

	if rowsAffected == 0 {
		return fmt.Errorf("capsule not found: %s", id)
	}

	return nil
}

// Delete deletes a capsule by ID
func (s *CapsuleStore) Delete(ctx context.Context, id string) error {
	query := `DELETE FROM capsules WHERE id = ?`

	result, err := s.db.ExecContext(ctx, query, id)
	if err != nil {
		return fmt.Errorf("failed to delete capsule: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}

	if rowsAffected == 0 {
		return fmt.Errorf("capsule not found: %s", id)
	}

	return nil
}

// GetByNodeID retrieves all capsules for a specific node
func (s *CapsuleStore) GetByNodeID(ctx context.Context, nodeID string) ([]*Capsule, error) {
	return s.List(ctx, nodeID, "")
}

// GetByStatus retrieves all capsules with a specific status
func (s *CapsuleStore) GetByStatus(ctx context.Context, status CapsuleStatus) ([]*Capsule, error) {
	return s.List(ctx, "", status)
}

// Count returns the total number of capsules, optionally filtered by status
func (s *CapsuleStore) Count(ctx context.Context, status CapsuleStatus) (int64, error) {
	query := `SELECT COUNT(*) FROM capsules WHERE 1=1`
	args := []interface{}{}

	if status != "" {
		query += " AND status = ?"
		args = append(args, status)
	}

	var count int64
	err := s.db.QueryRowContext(ctx, query, args...).Scan(&count)
	if err != nil {
		return 0, fmt.Errorf("failed to count capsules: %w", err)
	}

	return count, nil
}
